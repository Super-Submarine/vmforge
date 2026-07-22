import json
import subprocess
import tarfile

import pytest

from vmforge_storage import DiskStore, StorageError
from vmforge_storage.bundle import BundleManager


def _write_marker(path, char: str, offset: int = 0):
    subprocess.run(
        ["qemu-io", "-f", "qcow2", "-c", f"write -P {ord(char)} {offset} 64k", str(path)],
        check=True, capture_output=True,
    )


def _read_marker(path, offset: int = 0) -> int:
    out = subprocess.run(
        ["qemu-io", "-r", "-f", "qcow2", "-c", f"read -v {offset} 1", str(path)],
        check=True, capture_output=True, text=True,
    ).stdout
    for line in out.splitlines():
        parts = line.split()
        if parts and parts[0].endswith(":") and len(parts) > 1:
            return int(parts[1], 16)
    raise AssertionError(f"could not parse qemu-io output:\n{out}")


@pytest.fixture()
def branched_vm(store: DiskStore):
    """VM with a multi-branch snapshot tree:

        base
        |-- feature-a
        `-- feature-b *      (current; active overlay has marker 'D')
    """
    store.create_disk("vm1", "root", "64M")
    disk = store.disk_path("vm1", "root")
    _write_marker(disk, "A")
    store.snapshot_create("vm1", "root", "base")
    _write_marker(disk, "B")
    store.snapshot_create("vm1", "root", "feature-a")
    store.snapshot_revert("vm1", "root", "base")
    _write_marker(disk, "C")
    store.snapshot_create("vm1", "root", "feature-b")
    _write_marker(disk, "D")
    (store.vm_dir("vm1") / "config.json").write_text(
        json.dumps({"name": "vm1", "cpus": 2, "memory_mib": 1024})
    )
    return store


def _tree(store: DiskStore, vm: str) -> dict:
    return {s.name: s for s in store.snapshot_list(vm, "root")}


def test_backup_bundle_contents(branched_vm: DiskStore, tmp_path):
    store = branched_vm
    bundle = tmp_path / "vm1.tar"
    result = BundleManager(store).backup("vm1", bundle)
    assert result.bundle == bundle.resolve()

    with tarfile.open(bundle) as tar:
        names = set(tar.getnames())
        manifest = json.load(tar.extractfile("manifest.json"))
    assert "disks/root.qcow2" in names
    assert "snapshots/root/base.qcow2" in names
    assert "snapshots/root/feature-a.qcow2" in names
    assert "snapshots/root/feature-b.qcow2" in names
    assert "config/config.json" in names
    assert manifest["schema_version"] == 1
    assert manifest["vm"] == "vm1"
    assert manifest["disks"]["root"]["current"] == "feature-b"
    for record in manifest["disks"]["root"]["snapshots"]:
        assert len(record["sha256"]) == 64
        assert record["size"] > 0


def test_round_trip_preserves_tree_and_data(branched_vm: DiskStore, tmp_path):
    store = branched_vm
    bundle = tmp_path / "vm1.tar.gz"
    BundleManager(store).backup("vm1", bundle)

    dest = DiskStore(home=tmp_path / "other-home")
    result = BundleManager(dest).restore(bundle)
    assert result.vm == "vm1"
    assert result.checks == {"root": True}

    tree = _tree(dest, "vm1")
    assert set(tree) == {"base", "feature-a", "feature-b"}
    assert tree["feature-a"].parent == "base"
    assert tree["feature-b"].parent == "base"
    assert tree["feature-b"].current

    # active overlay data survived
    assert _read_marker(dest.disk_path("vm1", "root")) == ord("D")
    # branch contents survived: revert to feature-a and read its marker
    dest.snapshot_revert("vm1", "root", "feature-a")
    assert _read_marker(dest.disk_path("vm1", "root")) == ord("B")

    config = json.loads((dest.vm_dir("vm1") / "config.json").read_text())
    assert config["cpus"] == 2


def test_restore_as_new_name(branched_vm: DiskStore, tmp_path):
    store = branched_vm
    bundle = tmp_path / "vm1.tar"
    BundleManager(store).backup("vm1", bundle)
    result = BundleManager(store).restore(bundle, as_vm="vm2")
    assert result.vm == "vm2"
    assert set(_tree(store, "vm2")) == {"base", "feature-a", "feature-b"}
    assert _read_marker(store.disk_path("vm2", "root")) == ord("D")


def test_restore_refuses_clobber_without_force(branched_vm: DiskStore, tmp_path):
    store = branched_vm
    bundle = tmp_path / "vm1.tar"
    BundleManager(store).backup("vm1", bundle)
    with pytest.raises(StorageError, match="already exists") as exc:
        BundleManager(store).restore(bundle)
    assert exc.value.code == "already_exists"
    # with --force it succeeds
    result = BundleManager(store).restore(bundle, force=True)
    assert result.vm == "vm1"
    assert _read_marker(store.disk_path("vm1", "root")) == ord("D")


def test_restore_rejects_corrupted_bundle(branched_vm: DiskStore, tmp_path):
    store = branched_vm
    bundle = tmp_path / "vm1.tar"
    BundleManager(store).backup("vm1", bundle)

    data = bytearray(bundle.read_bytes())
    # flip bytes inside the payload region, past the manifest header blocks
    for off in range(len(data) // 2, len(data) // 2 + 64):
        data[off] ^= 0xFF
    corrupted = tmp_path / "corrupted.tar"
    corrupted.write_bytes(bytes(data))

    dest = DiskStore(home=tmp_path / "corrupt-home")
    with pytest.raises(StorageError, match="corrupt bundle") as exc:
        BundleManager(dest).restore(corrupted)
    assert exc.value.code == "invalid_state"
    assert not dest.vm_dir("vm1").exists()


def test_restore_rejects_garbage_file(store: DiskStore, tmp_path):
    garbage = tmp_path / "garbage.tar"
    garbage.write_bytes(b"this is not a tar archive at all")
    with pytest.raises(StorageError, match="corrupt bundle"):
        BundleManager(store).restore(garbage)


def test_backup_missing_vm(store: DiskStore, tmp_path):
    with pytest.raises(StorageError, match="not found") as exc:
        BundleManager(store).backup("nope", tmp_path / "x.tar")
    assert exc.value.code == "not_found"


def test_backup_refuses_existing_bundle(branched_vm: DiskStore, tmp_path):
    bundle = tmp_path / "vm1.tar"
    bundle.write_bytes(b"")
    with pytest.raises(StorageError, match="already exists"):
        BundleManager(branched_vm).backup("vm1", bundle)


def test_backup_snapshot_subchain(branched_vm: DiskStore, tmp_path):
    store = branched_vm
    bundle = tmp_path / "sub.tar"
    BundleManager(store).backup("vm1", bundle, snapshot="feature-a")

    with tarfile.open(bundle) as tar:
        names = set(tar.getnames())
    assert "snapshots/root/base.qcow2" in names
    assert "snapshots/root/feature-a.qcow2" in names
    assert "snapshots/root/feature-b.qcow2" not in names
    assert "disks/root.qcow2" not in names

    dest = DiskStore(home=tmp_path / "sub-home")
    result = BundleManager(dest).restore(bundle)
    assert result.checks == {"root": True}
    tree = _tree(dest, "vm1")
    assert set(tree) == {"base", "feature-a"}
    assert tree["feature-a"].current
    assert _read_marker(dest.disk_path("vm1", "root")) == ord("B")


def test_backup_unknown_snapshot(branched_vm: DiskStore, tmp_path):
    with pytest.raises(StorageError, match="snapshot not found"):
        BundleManager(branched_vm).backup("vm1", tmp_path / "x.tar", snapshot="nope")


def test_round_trip_linked_clone(store: DiskStore, tmp_path):
    src = tmp_path / "base-src.qcow2"
    subprocess.run(
        ["qemu-img", "create", "-f", "qcow2", str(src), "64M"],
        check=True, capture_output=True,
    )
    _write_marker(src, "Z")
    store.import_image(src, "ubuntu-base")
    store.clone_disk("ubuntu-base", "clone-vm", "root")
    store.snapshot_create("clone-vm", "root", "s1")

    bundle = tmp_path / "clone.tar"
    BundleManager(store).backup("clone-vm", bundle)
    with tarfile.open(bundle) as tar:
        assert "images/ubuntu-base.qcow2" in tar.getnames()

    dest = DiskStore(home=tmp_path / "clone-home")
    result = BundleManager(dest).restore(bundle)
    assert result.checks == {"root": True}
    assert (dest.images_dir / "ubuntu-base.qcow2").exists()
    assert _read_marker(dest.disk_path("clone-vm", "root")) == ord("Z")


def test_cli_backup_restore_json(branched_vm: DiskStore, tmp_path):
    from vmforge_storage.cli import main

    bundle = tmp_path / "cli.tar"
    home = str(branched_vm.home)
    assert main(["--home", home, "--json", "backup", "vm1", str(bundle)]) == 0

    dest_home = str(tmp_path / "cli-home")
    assert main(["--home", dest_home, "--json", "restore", str(bundle)]) == 0
    # clobber refusal exits 1
    assert main(["--home", dest_home, "restore", str(bundle)]) == 1
    # --as + --force paths
    assert main(["--home", dest_home, "restore", str(bundle), "--as", "vm9"]) == 0
    assert main(["--home", dest_home, "restore", str(bundle), "--force"]) == 0
