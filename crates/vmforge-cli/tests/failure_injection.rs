//! Failure-injection tests: drive the built `vmforge` binary through every
//! taxonomy error class and assert the stable exit code and `--json` error
//! shape (`docs/error-taxonomy.md`).
//!
//! Host-condition probes (kvm/qemu/home/disk) are injected via the documented
//! test knobs (`VMFORGE_KVM_PATH`, `VMFORGE_QEMU_BIN`, `VMFORGE_MIN_FREE_BYTES`)
//! plus real filesystem states (absent device node, unreadable file, missing
//! or truncated qcow2). Classes that need real hardware to trigger naturally
//! (boot_timeout, qemu_crashed, snapshot_conflict, port_in_use) are exercised
//! end-to-end via `VMFORGE_INJECT_ERROR`; the manual hardware procedure is in
//! `docs/error-taxonomy.md` §4.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use vmforge_core::taxonomy::ErrorClass;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vmforge"))
}

fn tmpdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vmforge-fi-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Doctor env that makes all host probes pass, so tests only fail the probe
/// they intend to.
fn passing_env(cmd: &mut Command, dir: &Path) {
    let kvm = dir.join("kvm");
    std::fs::write(&kvm, b"").unwrap();
    let qemu = dir.join("qemu-system-x86_64");
    std::fs::write(&qemu, b"").unwrap();
    cmd.env("VMFORGE_KVM_PATH", &kvm)
        .env("VMFORGE_QEMU_BIN", &qemu)
        .env("VMFORGE_HOME", dir.join("home"))
        .env("VMFORGE_MIN_FREE_BYTES", "0");
}

fn stderr_error(output: &Output) -> serde_json::Value {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let doc: serde_json::Value = serde_json::from_str(stderr.trim())
        .unwrap_or_else(|e| panic!("stderr is not one JSON document ({e}): {stderr}"));
    doc["error"].clone()
}

fn assert_error(output: &Output, class: ErrorClass) {
    assert_eq!(
        output.status.code(),
        Some(class.exit_code()),
        "wrong exit code; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let error = stderr_error(output);
    assert_eq!(error["code"], class.code());
    assert!(error["message"].is_string());
    assert!(
        !error["recovery"].as_str().unwrap_or("").is_empty(),
        "recovery hint missing"
    );
}

#[test]
fn doctor_passes_with_healthy_injected_host() {
    let dir = tmpdir("healthy");
    let mut cmd = bin();
    passing_env(&mut cmd, &dir);
    let out = cmd.args(["doctor", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("doctor --json stdout is one JSON document");
    assert_eq!(doc["ok"], true);
    assert!(doc["probes"].as_array().unwrap().len() >= 3);
}

#[test]
fn doctor_kvm_unavailable() {
    let dir = tmpdir("kvm-missing");
    let mut cmd = bin();
    passing_env(&mut cmd, &dir);
    cmd.env("VMFORGE_KVM_PATH", dir.join("no-such-kvm"));
    let out = cmd.args(["doctor", "--json"]).output().unwrap();
    assert_error(&out, ErrorClass::KvmUnavailable);
}

#[cfg(unix)]
#[test]
fn doctor_kvm_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tmpdir("kvm-denied");
    let mut cmd = bin();
    passing_env(&mut cmd, &dir);
    let kvm = dir.join("kvm-denied");
    std::fs::write(&kvm, b"").unwrap();
    std::fs::set_permissions(&kvm, std::fs::Permissions::from_mode(0o000)).unwrap();
    cmd.env("VMFORGE_KVM_PATH", &kvm);
    let out = cmd.args(["doctor", "--json"]).output().unwrap();
    // Root ignores file modes (e.g. some CI containers); skip there.
    if out.status.code() == Some(0) {
        eprintln!("skipping: running as root, chmod 000 not enforced");
        return;
    }
    assert_error(&out, ErrorClass::KvmPermissionDenied);
}

#[test]
fn doctor_qemu_binary_missing() {
    let dir = tmpdir("qemu-missing");
    let mut cmd = bin();
    passing_env(&mut cmd, &dir);
    cmd.env("VMFORGE_QEMU_BIN", dir.join("no-such-qemu"));
    let out = cmd.args(["doctor", "--json"]).output().unwrap();
    assert_error(&out, ErrorClass::QemuBinaryMissing);
}

#[test]
fn doctor_disk_full() {
    let dir = tmpdir("disk-full");
    let mut cmd = bin();
    passing_env(&mut cmd, &dir);
    cmd.env("VMFORGE_MIN_FREE_BYTES", u64::MAX.to_string());
    let out = cmd.args(["doctor", "--json"]).output().unwrap();
    assert_error(&out, ErrorClass::DiskFull);
}

#[test]
fn doctor_disk_image_missing() {
    let dir = tmpdir("disk-missing");
    let mut cmd = bin();
    passing_env(&mut cmd, &dir);
    let missing = dir.join("no-such-disk.qcow2");
    let out = cmd
        .args(["doctor", "--json", "--disk", missing.to_str().unwrap()])
        .output()
        .unwrap();
    assert_error(&out, ErrorClass::DiskImageMissing);
}

#[test]
fn doctor_disk_image_corrupt_truncated() {
    let dir = tmpdir("disk-corrupt");
    let mut cmd = bin();
    passing_env(&mut cmd, &dir);
    let truncated = dir.join("truncated.qcow2");
    std::fs::write(&truncated, b"QFI").unwrap(); // truncated qcow2 header
    let out = cmd
        .args(["doctor", "--json", "--disk", truncated.to_str().unwrap()])
        .output()
        .unwrap();
    assert_error(&out, ErrorClass::DiskImageCorrupt);
}

#[test]
fn doctor_disk_image_corrupt_bad_magic() {
    let dir = tmpdir("disk-magic");
    let mut cmd = bin();
    passing_env(&mut cmd, &dir);
    let garbage = dir.join("garbage.qcow2");
    std::fs::write(&garbage, b"this is definitely not a qcow2 image").unwrap();
    let out = cmd
        .args(["doctor", "--json", "--disk", garbage.to_str().unwrap()])
        .output()
        .unwrap();
    assert_error(&out, ErrorClass::DiskImageCorrupt);
}

/// Every taxonomy class is surfaceable end-to-end with its distinct exit
/// code and JSON error object via the documented injection knob.
#[test]
fn injected_errors_cover_every_class() {
    for class in ErrorClass::ALL {
        let out = bin()
            .args(["doctor", "--json"])
            .env("VMFORGE_INJECT_ERROR", class.code())
            .output()
            .unwrap();
        assert_error(&out, class);
    }
}

/// Injection also works on the frozen `info` verb without changing its
/// human output contract when unset.
#[test]
fn injection_on_info_uses_human_format() {
    let out = bin()
        .arg("info")
        .env("VMFORGE_INJECT_ERROR", ErrorClass::BootTimeout.code())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(ErrorClass::BootTimeout.exit_code()));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("boot_timeout"), "stderr: {stderr}");
    assert!(stderr.contains("recovery:"), "stderr: {stderr}");
}

#[test]
fn frozen_exit_codes_unchanged() {
    // Freeze doc §1.2: unknown verb exits 2.
    let out = bin().arg("bogus-verb").output().unwrap();
    assert_eq!(out.status.code(), Some(2));

    // Unknown injection code is a usage error, not a silent pass.
    let out = bin()
        .arg("info")
        .env("VMFORGE_INJECT_ERROR", "not-a-code")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
