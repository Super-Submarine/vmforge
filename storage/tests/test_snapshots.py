import subprocess

import pytest

from vmforge_storage import DiskStore, StorageError, qemu_img


def _write_marker(path, text: str, offset: int = 0):
    """Write data into a qcow2 image via qemu-io-free 'qemu-img dd' trick:
    convert marker bytes in with qemu-img is heavy, so use qemu-io if present,
    else fall back to writing through a raw convert cycle."""
    subprocess.run(
        ["qemu-io", "-f", "qcow2", "-c", f"write -P {ord(text[0])} {offset} 64k", str(path)],
        check=True, capture_output=True,
    )


def _read_marker(path, offset: int = 0) -> int:
    out = subprocess.run(
        ["qemu-io", "-f", "qcow2", "-c", f"read -v {offset} 1", str(path)],
        check=True, capture_output=True, text=True,
    ).stdout
    for line in out.splitlines():
        parts = line.split()
        if parts and parts[0].endswith(":") and len(parts) > 1:
            return int(parts[1], 16)
    raise AssertionError(f"could not parse qemu-io output:\n{out}")


@pytest.fixture()
def disk(store: DiskStore):
    store.create_disk("vm1", "root", "64M")
    return store.disk_path("vm1", "root")


def test_snapshot_create_freezes_state(store: DiskStore, disk):
    snap = store.snapshot_create("vm1", "root", "s1")
    assert snap.path.exists()
    assert snap.parent is None
    assert snap.current
    # active disk is now an overlay backed by the snapshot
    inf = qemu_img.info(disk)[0]
    assert inf.backing_file == snap.path.resolve()


def test_snapshot_duplicate_name_fails(store: DiskStore, disk):
    store.snapshot_create("vm1", "root", "s1")
    with pytest.raises(StorageError, match="already exists"):
        store.snapshot_create("vm1", "root", "s1")


def test_snapshot_chain_parents(store: DiskStore, disk):
    store.snapshot_create("vm1", "root", "s1")
    store.snapshot_create("vm1", "root", "s2")
    snaps = {s.name: s for s in store.snapshot_list("vm1", "root")}
    assert snaps["s1"].parent is None
    assert snaps["s2"].parent == "s1"
    assert snaps["s1"].children == ["s2"]
    assert snaps["s2"].current


def test_snapshot_tree_branches(store: DiskStore, disk):
    store.snapshot_create("vm1", "root", "base")
    store.snapshot_create("vm1", "root", "feature-a")
    store.snapshot_revert("vm1", "root", "base")
    store.snapshot_create("vm1", "root", "feature-b")
    snaps = {s.name: s for s in store.snapshot_list("vm1", "root")}
    assert snaps["feature-a"].parent == "base"
    assert snaps["feature-b"].parent == "base"
    assert sorted(snaps["base"].children) == ["feature-a", "feature-b"]
    assert snaps["feature-b"].current
    tree = store.snapshot_tree("vm1", "root")
    assert "base" in tree
    assert "|-- feature-a" in tree
    assert "`-- feature-b *" in tree


def test_snapshot_revert_discards_writes(store: DiskStore, disk):
    _write_marker(disk, "A")
    store.snapshot_create("vm1", "root", "with-a")
    _write_marker(disk, "B")
    assert _read_marker(disk) == ord("B")
    store.snapshot_revert("vm1", "root", "with-a")
    assert _read_marker(disk) == ord("A")


def test_snapshot_revert_unknown(store: DiskStore, disk):
    with pytest.raises(StorageError, match="not found"):
        store.snapshot_revert("vm1", "root", "ghost")


def test_snapshot_delete_leaf(store: DiskStore, disk):
    store.snapshot_create("vm1", "root", "s1")
    store.snapshot_create("vm1", "root", "s2")
    store.snapshot_revert("vm1", "root", "s1")
    store.snapshot_delete("vm1", "root", "s2")
    names = [s.name for s in store.snapshot_list("vm1", "root")]
    assert names == ["s1"]


def test_snapshot_delete_current_fails(store: DiskStore, disk):
    store.snapshot_create("vm1", "root", "s1")
    with pytest.raises(StorageError, match="active"):
        store.snapshot_delete("vm1", "root", "s1")


def test_snapshot_delete_middle_squashes(store: DiskStore, disk):
    _write_marker(disk, "A", 0)
    store.snapshot_create("vm1", "root", "s1")
    _write_marker(disk, "B", 65536)
    store.snapshot_create("vm1", "root", "s2")
    store.snapshot_create("vm1", "root", "s3")
    # delete middle snapshot s2: its data must be preserved in child s3
    store.snapshot_delete("vm1", "root", "s2")
    snaps = {s.name: s for s in store.snapshot_list("vm1", "root")}
    assert set(snaps) == {"s1", "s3"}
    assert snaps["s3"].parent == "s1"
    assert _read_marker(disk, 0) == ord("A")
    assert _read_marker(disk, 65536) == ord("B")


def test_snapshot_delete_with_two_children_fails(store: DiskStore, disk):
    store.snapshot_create("vm1", "root", "base")
    store.snapshot_create("vm1", "root", "a")
    store.snapshot_revert("vm1", "root", "base")
    store.snapshot_create("vm1", "root", "b")
    store.snapshot_revert("vm1", "root", "a")
    with pytest.raises(StorageError, match="multiple children"):
        store.snapshot_delete("vm1", "root", "base")


def test_check_disk_with_snapshots(store: DiskStore, disk):
    store.snapshot_create("vm1", "root", "s1")
    assert store.check_disk("vm1", "root").ok
