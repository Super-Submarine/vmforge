//! Integration tests for `SubprocessStore` against the real `vmforge-storage`
//! CLI and real qcow2 files created by `qemu-img`.
//!
//! Requires `vmforge-storage` (pip install storage/) and `qemu-img` on PATH;
//! skips gracefully when either is missing so plain `cargo test` still works
//! on machines without the Python storage package.

use std::path::PathBuf;
use std::process::Command;

use vmforge_core::storage::{
    DiskName, ErrorKind, ImageName, SnapshotName, StorageProvider, SubprocessStore, VmName,
};

fn have(bin: &str) -> bool {
    Command::new(bin)
        .arg("--help")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

macro_rules! require_tools {
    () => {
        if !have("vmforge-storage") || !have("qemu-img") {
            eprintln!("skipping: vmforge-storage and/or qemu-img not on PATH");
            return;
        }
    };
}

struct TestHome(PathBuf);

impl TestHome {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("vmforge-storage-it-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn names(vm: &str, disk: &str) -> (VmName, DiskName) {
    (VmName::new(vm).unwrap(), DiskName::new(disk).unwrap())
}

fn snap(name: &str) -> SnapshotName {
    SnapshotName::new(name).unwrap()
}

#[test]
fn contract_handshake() {
    require_tools!();
    let home = TestHome::new("handshake");
    let store = SubprocessStore::new(&home.0).expect("contract version 1 accepted");
    assert_eq!(store.contract_version().unwrap(), "1");
}

#[test]
fn disk_create_resize_delete() {
    require_tools!();
    let home = TestHome::new("disk");
    let store = SubprocessStore::new(&home.0).unwrap();
    let (vm, disk) = names("vm1", "root");

    let info = store.create_disk(&vm, &disk, 16 << 20, None).unwrap();
    assert_eq!(info.path, store.attach_path(&vm, &disk));
    assert!(info.path.exists(), "active overlay exists at attach_path");

    store.resize_disk(&vm, &disk, 32 << 20).unwrap();

    let err = store.create_disk(&vm, &disk, 16 << 20, None).unwrap_err();
    assert_eq!(err.kind, ErrorKind::AlreadyExists);

    store.delete_disk(&vm, &disk).unwrap();
    assert!(!store.attach_path(&vm, &disk).exists());

    let err = store.resize_disk(&vm, &disk, 64 << 20).unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}

#[test]
fn import_image_and_linked_clone() {
    require_tools!();
    let home = TestHome::new("import");
    let store = SubprocessStore::new(&home.0).unwrap();

    // Make a source image with qemu-img, import it, clone a disk off it.
    let src = home.0.join("src.qcow2");
    let status = Command::new("qemu-img")
        .args(["create", "-f", "qcow2"])
        .arg(&src)
        .arg("16M")
        .status()
        .unwrap();
    assert!(status.success());

    let image = ImageName::new("alpine-base").unwrap();
    let imported = store.import_image(&src, &image).unwrap();
    assert!(imported.path.exists());
    assert!(imported.path.starts_with(home.0.join("images")));

    let (vm, disk) = names("vm1", "root");
    let info = store.create_disk(&vm, &disk, 0, Some(&image)).unwrap();
    assert!(info.path.exists(), "linked clone created");

    let err = store
        .create_disk(
            &vm,
            &DiskName::new("d2").unwrap(),
            0,
            Some(&ImageName::new("ghost").unwrap()),
        )
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}

#[test]
fn snapshot_tree_branching_restore_nonleaf_delete_with_children() {
    require_tools!();
    let home = TestHome::new("tree");
    let store = SubprocessStore::new(&home.0).unwrap();
    let (vm, disk) = names("vm1", "root");
    store.create_disk(&vm, &disk, 16 << 20, None).unwrap();

    // Linear chain: base -> mid -> leaf
    store.snapshot(&vm, &disk, &snap("base")).unwrap();
    store.snapshot(&vm, &disk, &snap("mid")).unwrap();
    store.snapshot(&vm, &disk, &snap("leaf")).unwrap();

    // Restore to non-leaf "base", then snapshot again => branch.
    store.revert(&vm, &disk, &snap("base")).unwrap();
    store.snapshot(&vm, &disk, &snap("branch")).unwrap();

    let tree = store.tree(&vm, &disk).unwrap();
    let get = |n: &str| tree.iter().find(|s| s.name == n).unwrap();
    assert_eq!(tree.len(), 4);
    assert_eq!(get("base").parent, None);
    assert_eq!(get("mid").parent.as_deref(), Some("base"));
    assert_eq!(get("leaf").parent.as_deref(), Some("mid"));
    assert_eq!(get("branch").parent.as_deref(), Some("base"));
    let mut base_children = get("base").children.clone();
    base_children.sort();
    assert_eq!(base_children, ["branch", "mid"]);
    assert!(
        get("branch").current,
        "active overlay is backed by 'branch'"
    );

    // Deleting a snapshot with multiple children must fail cleanly.
    let err = store
        .delete_snapshot(&vm, &disk, &snap("base"))
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidState);

    // Deleting a single-child middle snapshot squashes it into its child.
    store.delete_snapshot(&vm, &disk, &snap("mid")).unwrap();
    let tree = store.tree(&vm, &disk).unwrap();
    let get = |n: &str| tree.iter().find(|s| s.name == n).unwrap();
    assert_eq!(tree.len(), 3);
    assert_eq!(get("leaf").parent.as_deref(), Some("base"));

    // Deleting the current snapshot must be refused; a leaf is fine.
    let err = store
        .delete_snapshot(&vm, &disk, &snap("branch"))
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidState);
    store.delete_snapshot(&vm, &disk, &snap("leaf")).unwrap();
    assert_eq!(store.tree(&vm, &disk).unwrap().len(), 2);

    // Unknown snapshot revert maps to NotFound.
    let err = store.revert(&vm, &disk, &snap("ghost")).unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}
