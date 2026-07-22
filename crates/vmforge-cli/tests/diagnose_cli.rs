//! End-to-end `vmforge diagnose` tests. No KVM, QEMU, or network required —
//! everything runs against a fixture $VMFORGE_HOME.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn fixture_home(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vmforge-diag-e2e-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for vm in ["alpha", "beta"] {
        let vm_dir = dir.join("vms").join(vm);
        fs::create_dir_all(vm_dir.join("disks")).unwrap();
        fs::create_dir_all(vm_dir.join("snapshots/disk0")).unwrap();
        fs::create_dir_all(vm_dir.join("logs")).unwrap();
        fs::write(vm_dir.join("disks/disk0.qcow2"), b"stub").unwrap();
        fs::write(vm_dir.join("snapshots/disk0/clean-install.qcow2"), b"stub").unwrap();
        fs::write(
            vm_dir.join("logs/qemu.log"),
            "qemu started\nAuthorization: Bearer abcdefghijklmnop\nguest shut down\n",
        )
        .unwrap();
    }
    fs::write(
        dir.join("config.toml"),
        "default_memory_mib = 4096\ngithub_token = \"ghp_1234567890abcdef\"\n",
    )
    .unwrap();
    dir
}

fn diagnose(home: &PathBuf, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vmforge"))
        .arg("diagnose")
        .arg("--home")
        .arg(home)
        .args(extra)
        .output()
        .expect("run vmforge diagnose")
}

#[test]
fn full_report_to_stdout_is_redacted() {
    let home = fixture_home("full");
    let out = diagnose(&home, &[]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = String::from_utf8_lossy(&out.stdout);

    // Field list promised by docs/tester-guide/reporting-bugs.md.
    for section in [
        "generated:",
        "-- version --",
        "-- host --",
        "-- kvm --",
        "-- qemu --",
        "-- backend --",
        "-- disk space --",
        "-- config --",
    ] {
        assert!(report.contains(section), "missing {section}");
    }
    // Per-VM state and logs.
    assert!(report.contains("-- vm: alpha --"));
    assert!(report.contains("-- vm: beta --"));
    assert!(report.contains("disk: disk0"));
    assert!(report.contains("clean-install"));
    assert!(report.contains("qemu started"));
    // Guardrails: secrets from config and logs never appear.
    assert!(
        !report.contains("ghp_1234567890abcdef"),
        "config token leaked"
    );
    assert!(!report.contains("abcdefghijklmnop"), "bearer token leaked");
    assert!(report.contains("[REDACTED]"));

    fs::remove_dir_all(&home).ok();
}

#[test]
fn vm_flag_scopes_report() {
    let home = fixture_home("scope");
    let out = diagnose(&home, &["--vm", "alpha"]);
    assert!(out.status.success());
    let report = String::from_utf8_lossy(&out.stdout);
    assert!(report.contains("-- vm: alpha --"));
    assert!(!report.contains("-- vm: beta --"));

    let bad = diagnose(&home, &["--vm", "missing"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("not found"));

    fs::remove_dir_all(&home).ok();
}

#[test]
fn tar_output_bundles_report_and_logs() {
    let home = fixture_home("tar");
    let bundle = home.join("diag.tar");
    let out = diagnose(&home, &["--output", bundle.to_str().unwrap()]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("summary"));
    assert!(stdout.contains("bundle written"));

    // Extract with system tar to prove the archive is standard.
    let extract = home.join("extract");
    fs::create_dir_all(&extract).unwrap();
    let status = Command::new("tar")
        .arg("-xf")
        .arg(&bundle)
        .arg("-C")
        .arg(&extract)
        .status()
        .expect("system tar");
    assert!(status.success(), "system tar could not extract the bundle");

    let report = fs::read_to_string(extract.join("report.txt")).unwrap();
    assert!(report.contains("== vmforge diagnose =="));
    assert!(!report.contains("ghp_1234567890abcdef"));
    let log = fs::read_to_string(extract.join("vms/alpha/logs/qemu.log")).unwrap();
    assert!(log.contains("qemu started"));
    assert!(!log.contains("abcdefghijklmnop"));

    fs::remove_dir_all(&home).ok();
}

#[test]
fn text_output_writes_plain_file() {
    let home = fixture_home("txt");
    let file = home.join("diag.txt");
    let out = diagnose(&home, &["--output", file.to_str().unwrap()]);
    assert!(out.status.success());
    let report = fs::read_to_string(&file).unwrap();
    assert!(report.contains("== vmforge diagnose =="));
    fs::remove_dir_all(&home).ok();
}
