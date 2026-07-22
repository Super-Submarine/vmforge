import pytest

from vmforge_storage import DiskStore, StorageError, qemu_img


def test_create_disk(store: DiskStore):
    path = store.create_disk("vm1", "root", "100M")
    assert path.exists()
    inf = qemu_img.info(path)[0]
    assert inf.format == "qcow2"
    assert inf.virtual_size == 100 * 1024 * 1024
    assert inf.backing_file is None


def test_create_disk_preallocation(store: DiskStore):
    path = store.create_disk("vm1", "root", "10M", preallocation="metadata")
    assert path.exists()
    lazy = store.create_disk("vm1", "lazy", "10M", preallocation="full")
    assert qemu_img.info(lazy)[0].actual_size >= 10 * 1024 * 1024


def test_create_disk_bad_preallocation(store: DiskStore):
    with pytest.raises(ValueError):
        store.create_disk("vm1", "root", "10M", preallocation="bogus")


def test_create_duplicate_disk_fails(store: DiskStore):
    store.create_disk("vm1", "root", "10M")
    with pytest.raises(StorageError, match="already exists"):
        store.create_disk("vm1", "root", "10M")


def test_invalid_names_rejected(store: DiskStore):
    for bad in ("../evil", "a/b", "", ".hidden", "-flag"):
        with pytest.raises(StorageError, match="invalid"):
            store.create_disk(bad, "root", "10M")
        with pytest.raises(StorageError, match="invalid"):
            store.create_disk("vm1", bad, "10M")


def test_resize_grow(store: DiskStore):
    path = store.create_disk("vm1", "root", "10M")
    store.resize_disk("vm1", "root", "20M")
    assert qemu_img.info(path)[0].virtual_size == 20 * 1024 * 1024


def test_resize_shrink_requires_flag(store: DiskStore):
    store.create_disk("vm1", "root", "20M")
    with pytest.raises(qemu_img.QemuImgError):
        store.resize_disk("vm1", "root", "10M")
    store.resize_disk("vm1", "root", "10M", shrink=True)
    assert qemu_img.info(store.disk_path("vm1", "root"))[0].virtual_size == 10 * 1024 * 1024


def test_import_raw_image(store: DiskStore, tmp_path):
    raw = tmp_path / "src.raw"
    raw.write_bytes(b"\x42" * (1024 * 1024))
    dst = store.import_image(raw, "base", src_format="raw")
    assert dst == store.images_dir / "base.qcow2"
    inf = qemu_img.info(dst)[0]
    assert inf.format == "qcow2"
    assert inf.virtual_size == 1024 * 1024


def test_import_as_vm_disk(store: DiskStore, tmp_path):
    raw = tmp_path / "src.raw"
    raw.write_bytes(b"\x00" * (1024 * 1024))
    dst = store.import_disk(raw, "vm1", "root", src_format="raw")
    assert dst == store.disk_path("vm1", "root")
    assert qemu_img.info(dst)[0].format == "qcow2"


def test_import_missing_source(store: DiskStore, tmp_path):
    with pytest.raises(StorageError, match="not found"):
        store.import_image(tmp_path / "nope.raw", "base")


def test_linked_clone(store: DiskStore, tmp_path):
    raw = tmp_path / "src.raw"
    raw.write_bytes(b"\x07" * (1024 * 1024))
    store.import_image(raw, "base", src_format="raw")
    clone = store.clone_disk("base", "vm2", "root")
    inf = qemu_img.info(clone)[0]
    assert inf.backing_file == (store.images_dir / "base.qcow2").resolve()
    # clone starts nearly empty: copy-on-write
    assert inf.actual_size < 1024 * 1024


def test_linked_clone_with_grow(store: DiskStore, tmp_path):
    raw = tmp_path / "src.raw"
    raw.write_bytes(b"\x00" * (1024 * 1024))
    store.import_image(raw, "base", src_format="raw")
    clone = store.clone_disk("base", "vm2", "root", size="4M")
    assert qemu_img.info(clone)[0].virtual_size == 4 * 1024 * 1024


def test_clone_missing_base(store: DiskStore):
    with pytest.raises(StorageError, match="not found"):
        store.clone_disk("ghost", "vm2", "root")


def test_delete_disk(store: DiskStore):
    path = store.create_disk("vm1", "root", "10M")
    store.delete_disk("vm1", "root")
    assert not path.exists()


def test_delete_disk_with_snapshots_needs_force(store: DiskStore):
    store.create_disk("vm1", "root", "10M")
    store.snapshot_create("vm1", "root", "s1")
    with pytest.raises(StorageError, match="has snapshots"):
        store.delete_disk("vm1", "root")
    store.delete_disk("vm1", "root", force=True)
    assert not store.disk_path("vm1", "root").exists()
    assert not store.snapshot_dir("vm1", "root").exists()


def test_info_backing_chain(store: DiskStore, tmp_path):
    raw = tmp_path / "src.raw"
    raw.write_bytes(b"\x00" * (1024 * 1024))
    store.import_image(raw, "base", src_format="raw")
    store.clone_disk("base", "vm2", "root")
    chain = store.disk_info("vm2", "root")
    assert len(chain) == 2
    assert chain[0].backing_file == chain[1].path


def test_check_clean_disk(store: DiskStore):
    store.create_disk("vm1", "root", "10M")
    result = store.check_disk("vm1", "root")
    assert result.ok
    assert result.corruptions == 0
    assert result.leaks == 0
