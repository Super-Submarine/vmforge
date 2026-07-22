"""vmforge-storage: CLI for VMForge disk & snapshot management.

Wraps DiskStore; all output meant for machines is JSON (--json), everything
else is terse human-readable text.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import asdict
from pathlib import Path

from .bundle import BundleManager
from .qemu_img import QemuImgError
from .store import DiskStore, StorageError

CONTRACT_VERSION = 1


def _store(args: argparse.Namespace) -> DiskStore:
    return DiskStore(home=args.home)


def _print(args: argparse.Namespace, human: str, machine: object) -> None:
    if args.json:
        json.dump(machine, sys.stdout, indent=2, default=str)
        print()
    else:
        print(human)


def cmd_create(args: argparse.Namespace) -> None:
    path = _store(args).create_disk(
        args.vm, args.disk, args.size,
        preallocation=args.preallocation, cluster_size=args.cluster_size,
    )
    _print(args, f"created {path}", {"path": str(path)})


def cmd_resize(args: argparse.Namespace) -> None:
    _store(args).resize_disk(args.vm, args.disk, args.size, shrink=args.shrink)
    _print(args, f"resized {args.vm}/{args.disk} to {args.size}", {"ok": True})


def cmd_import(args: argparse.Namespace) -> None:
    store = _store(args)
    if args.vm:
        path = store.import_disk(args.src, args.vm, args.disk or Path(args.src).stem,
                                 src_format=args.format)
    else:
        path = store.import_image(args.src, args.name or Path(args.src).stem,
                                  src_format=args.format, compress=args.compress)
    _print(args, f"imported {args.src} -> {path}", {"path": str(path)})


def cmd_clone(args: argparse.Namespace) -> None:
    path = _store(args).clone_disk(args.base, args.vm, args.disk, size=args.size)
    _print(args, f"linked clone {path} (backed by {args.base})", {"path": str(path)})


def cmd_delete(args: argparse.Namespace) -> None:
    _store(args).delete_disk(args.vm, args.disk, force=args.force)
    _print(args, f"deleted {args.vm}/{args.disk}", {"ok": True})


def cmd_info(args: argparse.Namespace) -> None:
    chain = _store(args).disk_info(args.vm, args.disk)
    human = []
    for i, inf in enumerate(chain):
        indent = "  " * i
        human.append(
            f"{indent}{inf.path} format={inf.format} "
            f"virtual={inf.virtual_size} on-disk={inf.actual_size}"
            + (f" backing={inf.backing_file}" if inf.backing_file else "")
        )
    _print(args, "\n".join(human), [inf.raw for inf in chain])


def cmd_check(args: argparse.Namespace) -> None:
    result = _store(args).check_disk(args.vm, args.disk, repair=args.repair)
    status = "clean" if result.ok else "issues found"
    human = (
        f"{status}: corruptions={result.corruptions} leaks={result.leaks}"
        + (f" fixed={result.errors_fixed + result.leaks_fixed}" if args.repair else "")
    )
    _print(args, human, result.raw)
    if not result.ok:
        sys.exit(3)


def cmd_backup(args: argparse.Namespace) -> None:
    result = BundleManager(_store(args)).backup(
        args.vm, args.bundle, snapshot=args.snapshot
    )
    _print(
        args,
        f"backed up {result.vm} ({', '.join(result.disks)}) -> {result.bundle} "
        f"({result.files} files, {result.total_bytes} bytes)",
        {
            "bundle": str(result.bundle),
            "vm": result.vm,
            "disks": result.disks,
            "files": result.files,
            "total_bytes": result.total_bytes,
        },
    )


def cmd_restore(args: argparse.Namespace) -> None:
    result = BundleManager(_store(args)).restore(
        args.bundle, as_vm=args.as_vm, force=args.force
    )
    _print(
        args,
        f"restored {result.vm} under {result.home} "
        f"({len(result.disks)} disk(s), {result.snapshots} snapshot(s); "
        f"health check passed)",
        {
            "vm": result.vm,
            "home": str(result.home),
            "disks": result.disks,
            "snapshots": result.snapshots,
            "checks": result.checks,
        },
    )


def cmd_snapshot_create(args: argparse.Namespace) -> None:
    snap = _store(args).snapshot_create(args.vm, args.disk, args.name)
    _print(args, f"snapshot {snap.name} created at {snap.path}", asdict(snap))


def cmd_snapshot_list(args: argparse.Namespace) -> None:
    store = _store(args)
    snaps = store.snapshot_list(args.vm, args.disk)
    _print(args, store.snapshot_tree(args.vm, args.disk), [asdict(s) for s in snaps])


def cmd_snapshot_revert(args: argparse.Namespace) -> None:
    _store(args).snapshot_revert(args.vm, args.disk, args.name)
    _print(args, f"reverted {args.vm}/{args.disk} to {args.name}", {"ok": True})


def cmd_snapshot_delete(args: argparse.Namespace) -> None:
    _store(args).snapshot_delete(args.vm, args.disk, args.name)
    _print(args, f"deleted snapshot {args.name}", {"ok": True})


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="vmforge-storage",
        description="VMForge qcow2 disk & snapshot-tree management",
    )
    parser.add_argument("--home", help="VMForge home (default: $VMFORGE_HOME or ~/.vmforge)")
    parser.add_argument("--json", action="store_true", help="machine-readable output")
    parser.add_argument(
        "--contract-version",
        action="version",
        version=str(CONTRACT_VERSION),
        help="print the interface contract major version and exit",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("create", help="create a new qcow2 disk")
    p.add_argument("vm")
    p.add_argument("disk")
    p.add_argument("size", help="e.g. 10G")
    p.add_argument("--preallocation", default="off",
                   choices=["off", "metadata", "falloc", "full"])
    p.add_argument("--cluster-size", default=None, help="e.g. 64k")
    p.set_defaults(func=cmd_create)

    p = sub.add_parser("resize", help="resize a disk")
    p.add_argument("vm")
    p.add_argument("disk")
    p.add_argument("size", help="new size, e.g. 20G")
    p.add_argument("--shrink", action="store_true")
    p.set_defaults(func=cmd_resize)

    p = sub.add_parser("import", help="import raw/ISO/vmdk/... image")
    p.add_argument("src")
    p.add_argument("--name", help="shared image name (imports into images/)")
    p.add_argument("--vm", help="import directly as a VM disk")
    p.add_argument("--disk", help="disk name when importing into a VM")
    p.add_argument("--format", help="force source format (raw, vmdk, vdi, ...)")
    p.add_argument("--compress", action="store_true")
    p.set_defaults(func=cmd_import)

    p = sub.add_parser("clone", help="create a linked clone backed by a base image")
    p.add_argument("base", help="image name under images/ or a path")
    p.add_argument("vm")
    p.add_argument("disk")
    p.add_argument("--size", default=None, help="optionally grow the clone, e.g. 20G")
    p.set_defaults(func=cmd_clone)

    p = sub.add_parser("delete", help="delete a disk")
    p.add_argument("vm")
    p.add_argument("disk")
    p.add_argument("--force", action="store_true", help="also delete its snapshots")
    p.set_defaults(func=cmd_delete)

    p = sub.add_parser("info", help="show disk info incl. backing chain")
    p.add_argument("vm")
    p.add_argument("disk")
    p.set_defaults(func=cmd_info)

    p = sub.add_parser("check", help="qemu-img check disk health")
    p.add_argument("vm")
    p.add_argument("disk")
    p.add_argument("--repair", action="store_true")
    p.set_defaults(func=cmd_check)

    p = sub.add_parser("backup", help="export a whole VM to a portable bundle")
    p.add_argument("vm")
    p.add_argument("bundle", help="bundle path (.tar, or .tar.gz/.tgz to compress)")
    p.add_argument("--snapshot", default=None,
                   help="export only the chain up to this snapshot")
    p.set_defaults(func=cmd_backup)

    p = sub.add_parser("restore", help="recreate a VM from a backup bundle")
    p.add_argument("bundle")
    p.add_argument("--as", dest="as_vm", default=None, metavar="NEW_VM",
                   help="restore under a different VM name")
    p.add_argument("--force", action="store_true",
                   help="overwrite an existing VM of the same name")
    p.set_defaults(func=cmd_restore)

    p = sub.add_parser("tree", help="show the snapshot tree (alias of 'snapshot list')")
    p.add_argument("vm")
    p.add_argument("disk")
    p.set_defaults(func=cmd_snapshot_list)

    snap = sub.add_parser("snapshot", help="offline snapshot tree management")
    snap_sub = snap.add_subparsers(dest="snapshot_command", required=True)

    p = snap_sub.add_parser("create", help="freeze current state as a snapshot")
    p.add_argument("vm")
    p.add_argument("disk")
    p.add_argument("name")
    p.set_defaults(func=cmd_snapshot_create)

    p = snap_sub.add_parser("list", help="show the snapshot tree (* = current base)")
    p.add_argument("vm")
    p.add_argument("disk")
    p.set_defaults(func=cmd_snapshot_list)

    p = snap_sub.add_parser("revert", help="discard active state, branch from a snapshot")
    p.add_argument("vm")
    p.add_argument("disk")
    p.add_argument("name")
    p.set_defaults(func=cmd_snapshot_revert)

    p = snap_sub.add_parser("delete", help="delete a snapshot (leaf or single-child)")
    p.add_argument("vm")
    p.add_argument("disk")
    p.add_argument("name")
    p.set_defaults(func=cmd_snapshot_delete)

    return parser


def _error_json(exc: StorageError | QemuImgError) -> dict:
    if isinstance(exc, StorageError):
        return {"error": {"code": exc.code, "message": str(exc)}}
    return {
        "error": {
            "code": "backend",
            "message": str(exc),
            "details": {"stderr": exc.stderr, "returncode": exc.returncode},
        }
    }


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        args.func(args)
    except (StorageError, QemuImgError) as exc:
        json.dump(_error_json(exc), sys.stderr)
        print(file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
