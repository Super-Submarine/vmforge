"""VMForge disk store: layout conventions, disk lifecycle, and the
git-like offline snapshot tree.

Layout (see storage/README.md):

    $VMFORGE_HOME/
        images/                                 # shared imported base images
            <image>.qcow2
        vms/<vm>/
            disks/<disk>.qcow2                  # active writable overlay
            snapshots/<disk>/<snapshot>.qcow2   # frozen snapshot layers

A snapshot is a frozen qcow2 layer; its parent is its qcow2 backing file.
The active disk is always a writable overlay whose backing file is the
"current" snapshot (or nothing, before the first snapshot). Reverting
recreates the active overlay on top of any snapshot in the tree, which is
what makes the history a tree rather than a chain.
"""

from __future__ import annotations

import os
import re
import shutil
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

from . import qemu_img
from .qemu_img import CheckResult, ImageInfo

_NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")


class StorageError(RuntimeError):
    pass


def _validate_name(kind: str, name: str) -> str:
    if not _NAME_RE.match(name):
        raise StorageError(
            f"invalid {kind} name {name!r}: use letters, digits, '.', '_', '-'"
        )
    return name


@dataclass
class Snapshot:
    name: str
    path: Path
    parent: Optional[str]  # parent snapshot name, None for a root
    children: list[str] = field(default_factory=list)
    current: bool = False  # is this the backing of the active disk?
    virtual_size: int = 0
    actual_size: int = 0


class DiskStore:
    def __init__(self, home: Optional[Path | str] = None) -> None:
        self.home = Path(
            home
            or os.environ.get("VMFORGE_HOME")
            or Path.home() / ".vmforge"
        ).resolve()

    # ---- layout helpers -------------------------------------------------
    @property
    def images_dir(self) -> Path:
        return self.home / "images"

    def vm_dir(self, vm: str) -> Path:
        return self.home / "vms" / _validate_name("vm", vm)

    def disk_path(self, vm: str, disk: str) -> Path:
        return self.vm_dir(vm) / "disks" / f"{_validate_name('disk', disk)}.qcow2"

    def snapshot_dir(self, vm: str, disk: str) -> Path:
        return self.vm_dir(vm) / "snapshots" / _validate_name("disk", disk)

    def snapshot_path(self, vm: str, disk: str, name: str) -> Path:
        return self.snapshot_dir(vm, disk) / f"{_validate_name('snapshot', name)}.qcow2"

    def _require_disk(self, vm: str, disk: str) -> Path:
        path = self.disk_path(vm, disk)
        if not path.exists():
            raise StorageError(f"disk not found: {path}")
        return path

    # ---- disk lifecycle -------------------------------------------------
    def create_disk(
        self,
        vm: str,
        disk: str,
        size: int | str,
        *,
        preallocation: str = "off",
        cluster_size: Optional[str] = None,
    ) -> Path:
        path = self.disk_path(vm, disk)
        if path.exists():
            raise StorageError(f"disk already exists: {path}")
        path.parent.mkdir(parents=True, exist_ok=True)
        qemu_img.create_qcow2(
            path, size, preallocation=preallocation, cluster_size=cluster_size
        )
        return path

    def resize_disk(self, vm: str, disk: str, new_size: int | str, *, shrink: bool = False) -> None:
        qemu_img.resize(self._require_disk(vm, disk), new_size, shrink=shrink)

    def import_image(
        self,
        src: Path | str,
        name: str,
        *,
        src_format: Optional[str] = None,
        compress: bool = False,
    ) -> Path:
        """Convert any qemu-supported image (raw, vmdk, vdi, qcow2, ...) into
        a shared base image under images/."""
        src = Path(src)
        if not src.exists():
            raise StorageError(f"source image not found: {src}")
        dst = self.images_dir / f"{_validate_name('image', name)}.qcow2"
        if dst.exists():
            raise StorageError(f"image already exists: {dst}")
        self.images_dir.mkdir(parents=True, exist_ok=True)
        qemu_img.convert(src, dst, src_format=src_format, compress=compress)
        return dst

    def import_disk(
        self,
        src: Path | str,
        vm: str,
        disk: str,
        *,
        src_format: Optional[str] = None,
    ) -> Path:
        """Convert an existing image directly into a VM's active disk."""
        src = Path(src)
        if not src.exists():
            raise StorageError(f"source image not found: {src}")
        dst = self.disk_path(vm, disk)
        if dst.exists():
            raise StorageError(f"disk already exists: {dst}")
        dst.parent.mkdir(parents=True, exist_ok=True)
        qemu_img.convert(src, dst, src_format=src_format)
        return dst

    def clone_disk(
        self,
        base: Path | str,
        vm: str,
        disk: str,
        *,
        size: Optional[int | str] = None,
    ) -> Path:
        """Create a linked clone: a copy-on-write overlay backed by `base`.

        `base` may be a shared image name (under images/) or any path to a
        qcow2/raw image. The base must never be modified while clones exist.
        """
        base_path = Path(base)
        if not base_path.exists():
            candidate = self.images_dir / f"{base}.qcow2"
            if candidate.exists():
                base_path = candidate
            else:
                raise StorageError(f"base image not found: {base}")
        base_info = qemu_img.info(base_path)[0]
        dst = self.disk_path(vm, disk)
        if dst.exists():
            raise StorageError(f"disk already exists: {dst}")
        dst.parent.mkdir(parents=True, exist_ok=True)
        qemu_img.create_qcow2(
            dst,
            size,
            backing_file=base_path.resolve(),
            backing_format=base_info.format,
        )
        return dst

    def delete_disk(self, vm: str, disk: str, *, force: bool = False) -> None:
        path = self._require_disk(vm, disk)
        snap_dir = self.snapshot_dir(vm, disk)
        if snap_dir.exists() and any(snap_dir.iterdir()) and not force:
            raise StorageError(
                f"disk {disk!r} has snapshots; delete them first or use force"
            )
        path.unlink()
        if snap_dir.exists():
            shutil.rmtree(snap_dir)

    # ---- info / health ---------------------------------------------------
    def disk_info(self, vm: str, disk: str) -> list[ImageInfo]:
        return qemu_img.info(self._require_disk(vm, disk), backing_chain=True)

    def check_disk(self, vm: str, disk: str, *, repair: bool = False) -> CheckResult:
        return qemu_img.check(self._require_disk(vm, disk), repair=repair)

    # ---- snapshot tree ----------------------------------------------------
    def _snapshot_name_for(self, vm: str, disk: str, path: Path) -> Optional[str]:
        snap_dir = self.snapshot_dir(vm, disk).resolve()
        path = path.resolve()
        if path.parent == snap_dir and path.suffix == ".qcow2":
            return path.stem
        return None

    def snapshot_create(self, vm: str, disk: str, name: str) -> Snapshot:
        """Freeze the active overlay as snapshot `name` and start a fresh
        overlay on top of it. Offline operation: the VM must not be running."""
        active = self._require_disk(vm, disk)
        snap_path = self.snapshot_path(vm, disk, name)
        if snap_path.exists():
            raise StorageError(f"snapshot already exists: {snap_path}")
        snap_path.parent.mkdir(parents=True, exist_ok=True)

        active_info = qemu_img.info(active)[0]
        active.rename(snap_path)
        os.chmod(snap_path, 0o444)
        try:
            qemu_img.create_qcow2(
                active, backing_file=snap_path, backing_format="qcow2"
            )
        except Exception:
            os.chmod(snap_path, 0o644)
            snap_path.rename(active)
            raise
        parent = (
            self._snapshot_name_for(vm, disk, active_info.backing_file)
            if active_info.backing_file
            else None
        )
        return Snapshot(
            name=name, path=snap_path, parent=parent, current=True,
            virtual_size=active_info.virtual_size,
            actual_size=active_info.actual_size,
        )

    def snapshot_list(self, vm: str, disk: str) -> list[Snapshot]:
        """Build the snapshot tree from qcow2 metadata (backing files)."""
        active = self._require_disk(vm, disk)
        snap_dir = self.snapshot_dir(vm, disk)
        current_parent = None
        active_backing = qemu_img.info(active)[0].backing_file
        if active_backing is not None:
            current_parent = self._snapshot_name_for(vm, disk, active_backing)

        snapshots: dict[str, Snapshot] = {}
        if snap_dir.exists():
            for path in sorted(snap_dir.glob("*.qcow2")):
                inf = qemu_img.info(path)[0]
                parent = (
                    self._snapshot_name_for(vm, disk, inf.backing_file)
                    if inf.backing_file
                    else None
                )
                snapshots[path.stem] = Snapshot(
                    name=path.stem,
                    path=path.resolve(),
                    parent=parent,
                    current=(path.stem == current_parent),
                    virtual_size=inf.virtual_size,
                    actual_size=inf.actual_size,
                )
        for snap in snapshots.values():
            if snap.parent and snap.parent in snapshots:
                snapshots[snap.parent].children.append(snap.name)
        return list(snapshots.values())

    def _get_snapshot(self, vm: str, disk: str, name: str) -> Snapshot:
        for snap in self.snapshot_list(vm, disk):
            if snap.name == name:
                return snap
        raise StorageError(f"snapshot not found: {name!r} (disk {disk!r}, vm {vm!r})")

    def snapshot_revert(self, vm: str, disk: str, name: str) -> None:
        """Discard the active overlay and branch a fresh one from `name`.

        Because any snapshot in the tree can be reverted to, histories fork
        like git branches."""
        active = self._require_disk(vm, disk)
        snap = self._get_snapshot(vm, disk, name)
        tmp = active.with_suffix(".qcow2.reverting")
        qemu_img.create_qcow2(tmp, backing_file=snap.path, backing_format="qcow2")
        tmp.replace(active)

    def snapshot_delete(self, vm: str, disk: str, name: str) -> None:
        """Delete a snapshot. Leaf snapshots are simply removed; a snapshot
        with exactly one child is squashed into that child (qemu-img commit
        is not applicable upward, so the child is rebased onto the parent's
        parent after the parent's data is pulled in via qemu-img rebase)."""
        snap = self._get_snapshot(vm, disk, name)
        if snap.current:
            raise StorageError(
                f"snapshot {name!r} is the base of the active disk; "
                "revert to another snapshot first"
            )
        if snap.children:
            if len(snap.children) > 1:
                raise StorageError(
                    f"snapshot {name!r} has multiple children "
                    f"({', '.join(snap.children)}); delete or merge them first"
                )
            child = self._get_snapshot(vm, disk, snap.children[0])
            grandparent = (
                self._get_snapshot(vm, disk, snap.parent).path if snap.parent else None
            )
            os.chmod(child.path, 0o644)
            # safe rebase copies name's data into the child before repointing
            qemu_img.rebase(child.path, grandparent)
            os.chmod(child.path, 0o444)
        os.chmod(snap.path, 0o644)
        snap.path.unlink()

    def snapshot_tree(self, vm: str, disk: str) -> str:
        """Render the snapshot tree as text, git-log style."""
        snaps = {s.name: s for s in self.snapshot_list(vm, disk)}
        roots = sorted(s.name for s in snaps.values() if s.parent not in snaps)
        lines: list[str] = []

        def label(name: str) -> str:
            return f"{name} *" if snaps[name].current else name

        def walk(name: str, prefix: str) -> None:
            children = sorted(snaps[name].children)
            for i, child in enumerate(children):
                last = i == len(children) - 1
                lines.append(f"{prefix}{'`-- ' if last else '|-- '}{label(child)}")
                walk(child, prefix + ("    " if last else "|   "))

        for root in roots:
            lines.append(label(root))
            walk(root, "")
        if not lines:
            return "(no snapshots)"
        return "\n".join(lines)
