"""Whole-VM portable backup/restore bundles.

A backup bundle is a single tar archive (optionally gzip-compressed, chosen
by file extension) containing everything needed to recreate a VM under any
$VMFORGE_HOME:

    manifest.json                       # schema, per-file sha256 checksums
    disks/<disk>.qcow2                  # active writable overlay(s)
    snapshots/<disk>/<snapshot>.qcow2   # frozen snapshot layers
    images/<image>.qcow2                # shared base images referenced by the chain
    config/<file>                       # VM config files (top-level files in the VM dir)

qcow2 backing-file pointers are absolute paths, so they are meaningless on
another machine. The manifest records the *logical* parent of every layer
(snapshot name, shared image, or nothing) and restore re-points every layer
with `qemu-img rebase -u` after extraction.
"""

from __future__ import annotations

import hashlib
import io
import json
import os
import shutil
import tarfile
import tempfile
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

from . import qemu_img
from .store import DiskStore, StorageError, Snapshot, _validate_name

MANIFEST_SCHEMA_VERSION = 1
MANIFEST_NAME = "manifest.json"


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def _tar_mode(path: Path, write: bool) -> str:
    name = path.name
    compressed = name.endswith((".tar.gz", ".tgz"))
    if write:
        return "w:gz" if compressed else "w"
    return "r:*"


@dataclass
class BackupResult:
    bundle: Path
    vm: str
    disks: list[str]
    files: int
    total_bytes: int


@dataclass
class RestoreResult:
    vm: str
    home: Path
    disks: list[str]
    snapshots: int
    checks: dict[str, bool] = field(default_factory=dict)


def _corrupt(message: str) -> StorageError:
    return StorageError(f"corrupt bundle: {message}", code="invalid_state")


class BundleManager:
    def __init__(self, store: DiskStore) -> None:
        self.store = store

    # ---- backup ---------------------------------------------------------
    def backup(
        self,
        vm: str,
        bundle_path: Path | str,
        *,
        snapshot: Optional[str] = None,
    ) -> BackupResult:
        store = self.store
        _validate_name("vm", vm)
        vm_dir = store.vm_dir(vm)
        if not vm_dir.exists():
            raise StorageError(f"vm not found: {vm_dir}", code="not_found")
        bundle_path = Path(bundle_path)
        if bundle_path.exists():
            raise StorageError(
                f"bundle already exists: {bundle_path}", code="already_exists"
            )

        disks_dir = vm_dir / "disks"
        disk_names = sorted(p.stem for p in disks_dir.glob("*.qcow2")) if disks_dir.exists() else []
        if not disk_names:
            raise StorageError(f"vm {vm!r} has no disks", code="not_found")
        if snapshot is not None and len(disk_names) > 1:
            raise StorageError(
                "--snapshot requires a VM with exactly one disk "
                f"(vm {vm!r} has {len(disk_names)})",
                code="invalid_state",
            )

        manifest: dict = {
            "schema_version": MANIFEST_SCHEMA_VERSION,
            "contract_version": 1,
            "vm": vm,
            "created": datetime.now(timezone.utc).isoformat(),
            "disks": {},
            "images": [],
            "config": [],
        }
        # archive-path -> source file
        sources: dict[str, Path] = {}
        images: dict[str, Path] = {}  # image file name -> source path

        for disk in disk_names:
            entry = self._collect_disk(vm, disk, snapshot, sources, images)
            manifest["disks"][disk] = entry

        for image_name, src in sorted(images.items()):
            arc = f"images/{image_name}"
            sources[arc] = src
            manifest["images"].append(self._file_record(arc, src))

        for path in sorted(vm_dir.iterdir()):
            if path.is_file():
                arc = f"config/{path.name}"
                sources[arc] = path
                manifest["config"].append(self._file_record(arc, path))

        for section in manifest["disks"].values():
            if section["active"] is not None:
                arc = section["active"]["path"]
                section["active"].update(self._file_record(arc, sources[arc]))
            for snap in section["snapshots"]:
                snap.update(self._file_record(snap["path"], sources[snap["path"]]))

        bundle_path.parent.mkdir(parents=True, exist_ok=True)
        total = 0
        try:
            with tarfile.open(bundle_path, _tar_mode(bundle_path, write=True)) as tar:
                data = json.dumps(manifest, indent=2).encode()
                info = tarfile.TarInfo(MANIFEST_NAME)
                info.size = len(data)
                info.mtime = int(datetime.now(timezone.utc).timestamp())
                tar.addfile(info, io.BytesIO(data))
                for arc, src in sorted(sources.items()):
                    tar.add(src, arcname=arc, recursive=False)
                    total += src.stat().st_size
        except BaseException:
            bundle_path.unlink(missing_ok=True)
            raise
        return BackupResult(
            bundle=bundle_path.resolve(),
            vm=vm,
            disks=disk_names,
            files=len(sources) + 1,
            total_bytes=total,
        )

    def _file_record(self, arc: str, src: Path) -> dict:
        return {"path": arc, "sha256": _sha256(src), "size": src.stat().st_size}

    def _collect_disk(
        self,
        vm: str,
        disk: str,
        snapshot: Optional[str],
        sources: dict[str, Path],
        images: dict[str, Path],
    ) -> dict:
        store = self.store
        active = store.disk_path(vm, disk)
        snaps: dict[str, Snapshot] = {s.name: s for s in store.snapshot_list(vm, disk)}
        current = next((s.name for s in snaps.values() if s.current), None)

        if snapshot is not None:
            if snapshot not in snaps:
                raise StorageError(
                    f"snapshot not found: {snapshot!r} (disk {disk!r}, vm {vm!r})",
                    code="not_found",
                )
            wanted: list[str] = []
            cursor: Optional[str] = snapshot
            while cursor is not None:
                wanted.append(cursor)
                cursor = snaps[cursor].parent
                if cursor is not None and cursor not in snaps:
                    break
            include = set(wanted)
            include_active = False
            current = snapshot
        else:
            include = set(snaps)
            include_active = True

        entry: dict = {"active": None, "current": current, "snapshots": []}
        for name in sorted(include):
            snap = snaps[name]
            arc = f"snapshots/{disk}/{name}.qcow2"
            sources[arc] = snap.path
            parent_ref = self._backing_ref(snap.path, snaps, images, only=include)
            entry["snapshots"].append(
                {"name": name, "path": arc, "parent": parent_ref}
            )
        if include_active:
            arc = f"disks/{disk}.qcow2"
            sources[arc] = active
            entry["active"] = {
                "path": arc,
                "parent": self._backing_ref(active, snaps, images, only=include),
            }
        return entry

    def _backing_ref(
        self,
        path: Path,
        snaps: dict[str, Snapshot],
        images: dict[str, Path],
        *,
        only: set[str],
    ) -> Optional[dict]:
        """Describe a layer's logical parent: a snapshot in the tree, a shared
        base image (bundled), or nothing."""
        backing = qemu_img.info(path)[0].backing_file
        if backing is None:
            return None
        for name, snap in snaps.items():
            if snap.path == backing.resolve():
                if name not in only:
                    raise StorageError(
                        f"layer {path.name} is backed by snapshot {name!r} "
                        "which is outside the exported chain",
                        code="invalid_state",
                    )
                return {"snapshot": name}
        # external base image (linked clone): bundle it
        image_name = backing.name
        existing = images.get(image_name)
        if existing is not None and existing.resolve() != backing.resolve():
            raise StorageError(
                f"two distinct base images share the file name {image_name!r}",
                code="invalid_state",
            )
        images[image_name] = backing
        return {"image": f"images/{image_name}"}

    # ---- restore --------------------------------------------------------
    def restore(
        self,
        bundle_path: Path | str,
        *,
        as_vm: Optional[str] = None,
        force: bool = False,
    ) -> RestoreResult:
        store = self.store
        bundle_path = Path(bundle_path)
        if not bundle_path.exists():
            raise StorageError(f"bundle not found: {bundle_path}", code="not_found")
        try:
            tar = tarfile.open(bundle_path, _tar_mode(bundle_path, write=False))
        except (tarfile.TarError, OSError) as exc:
            raise _corrupt(f"not a readable tar archive ({exc})") from exc
        with tar:
            manifest = self._read_manifest(tar)
            vm = _validate_name("vm", as_vm or manifest["vm"])
            vm_dir = store.vm_dir(vm)
            if vm_dir.exists():
                if not force:
                    raise StorageError(
                        f"vm already exists: {vm_dir} (use --force to overwrite)",
                        code="already_exists",
                    )

            store.home.mkdir(parents=True, exist_ok=True)
            staging = Path(tempfile.mkdtemp(prefix=f".restore-{vm}-", dir=store.home))
            try:
                self._extract_verified(tar, manifest, staging)
                self._place_images(manifest, staging)
                if vm_dir.exists():
                    shutil.rmtree(vm_dir)
                result = self._assemble_vm(vm, manifest, staging)
            except BaseException:
                if vm_dir.exists():
                    shutil.rmtree(vm_dir, ignore_errors=True)
                raise
            finally:
                shutil.rmtree(staging, ignore_errors=True)
        return result

    def _read_manifest(self, tar: tarfile.TarFile) -> dict:
        try:
            member = tar.getmember(MANIFEST_NAME)
            fh = tar.extractfile(member)
            if fh is None:
                raise KeyError(MANIFEST_NAME)
            manifest = json.load(fh)
        except (KeyError, tarfile.TarError, json.JSONDecodeError) as exc:
            raise _corrupt(f"missing or unreadable {MANIFEST_NAME}") from exc
        version = manifest.get("schema_version")
        if version != MANIFEST_SCHEMA_VERSION:
            raise StorageError(
                f"unsupported bundle schema_version {version!r} "
                f"(this tool supports {MANIFEST_SCHEMA_VERSION})",
                code="invalid_config",
            )
        if not isinstance(manifest.get("vm"), str) or not isinstance(
            manifest.get("disks"), dict
        ):
            raise _corrupt("manifest is missing required fields")
        return manifest

    def _manifest_files(self, manifest: dict) -> list[dict]:
        files: list[dict] = []
        for section in manifest["disks"].values():
            if section.get("active") is not None:
                files.append(section["active"])
            files.extend(section.get("snapshots", []))
        files.extend(manifest.get("images", []))
        files.extend(manifest.get("config", []))
        return files

    def _extract_verified(
        self, tar: tarfile.TarFile, manifest: dict, staging: Path
    ) -> None:
        for record in self._manifest_files(manifest):
            arc = record.get("path")
            if (
                not isinstance(arc, str)
                or Path(arc).is_absolute()
                or ".." in Path(arc).parts
            ):
                raise _corrupt(f"unsafe archive path {arc!r}")
            try:
                member = tar.getmember(arc)
                fh = tar.extractfile(member)
                if fh is None:
                    raise KeyError(arc)
            except (KeyError, tarfile.TarError) as exc:
                raise _corrupt(f"missing archive member {arc!r}") from exc
            dst = staging / arc
            dst.parent.mkdir(parents=True, exist_ok=True)
            h = hashlib.sha256()
            size = 0
            with dst.open("wb") as out:
                for chunk in iter(lambda: fh.read(1024 * 1024), b""):
                    h.update(chunk)
                    size += len(chunk)
                    out.write(chunk)
            if size != record.get("size") or h.hexdigest() != record.get("sha256"):
                raise _corrupt(f"checksum mismatch for {arc!r}")

    def _place_images(self, manifest: dict, staging: Path) -> None:
        store = self.store
        for record in manifest.get("images", []):
            arc = record["path"]
            dst = store.images_dir / Path(arc).name
            if dst.exists():
                if _sha256(dst) != record["sha256"]:
                    raise StorageError(
                        f"existing base image {dst} differs from the bundled copy; "
                        "refusing to overwrite a shared image",
                        code="invalid_state",
                    )
                continue
            store.images_dir.mkdir(parents=True, exist_ok=True)
            shutil.move(staging / arc, dst)

    def _resolve_parent(
        self, vm: str, disk: str, parent: Optional[dict]
    ) -> Optional[Path]:
        if parent is None:
            return None
        if "snapshot" in parent:
            return self.store.snapshot_path(vm, disk, parent["snapshot"])
        if "image" in parent:
            return self.store.images_dir / Path(parent["image"]).name
        raise _corrupt(f"unrecognized parent record {parent!r}")

    def _assemble_vm(self, vm: str, manifest: dict, staging: Path) -> RestoreResult:
        store = self.store
        vm_dir = store.vm_dir(vm)
        snapshots = 0
        disks: list[str] = []
        for disk, section in sorted(manifest["disks"].items()):
            _validate_name("disk", disk)
            disks.append(disk)
            for snap in section.get("snapshots", []):
                name = _validate_name("snapshot", snap["name"])
                dst = store.snapshot_path(vm, disk, name)
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.move(staging / snap["path"], dst)
                snapshots += 1
            for snap in section.get("snapshots", []):
                dst = store.snapshot_path(vm, disk, snap["name"])
                parent_path = self._resolve_parent(vm, disk, snap.get("parent"))
                qemu_img.rebase(dst, parent_path, unsafe=True)
                os.chmod(dst, 0o444)
            active = store.disk_path(vm, disk)
            active.parent.mkdir(parents=True, exist_ok=True)
            if section.get("active") is not None:
                shutil.move(staging / section["active"]["path"], active)
                parent_path = self._resolve_parent(
                    vm, disk, section["active"].get("parent")
                )
                qemu_img.rebase(active, parent_path, unsafe=True)
            else:
                current = section.get("current")
                if current is None:
                    raise _corrupt(
                        f"disk {disk!r} has neither an active overlay nor a "
                        "current snapshot"
                    )
                qemu_img.create_qcow2(
                    active,
                    backing_file=store.snapshot_path(vm, disk, current),
                    backing_format="qcow2",
                )
        for record in manifest.get("config", []):
            name = Path(record["path"]).name
            shutil.move(staging / record["path"], vm_dir / name)

        checks: dict[str, bool] = {}
        for disk in disks:
            result = store.check_disk(vm, disk)
            checks[disk] = result.ok
            if not result.ok:
                raise StorageError(
                    f"restored disk {vm}/{disk} failed the qcow2 health check "
                    f"(corruptions={result.corruptions} leaks={result.leaks})",
                    code="invalid_state",
                )
        return RestoreResult(
            vm=vm, home=store.home, disks=disks, snapshots=snapshots, checks=checks
        )
