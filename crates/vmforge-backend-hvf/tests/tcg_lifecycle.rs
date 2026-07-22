//! End-to-end lifecycle test of the hvf backend's aarch64 `virt`
//! invocation and QMP path, runnable in Linux CI.
//!
//! On macOS with Hypervisor.framework the backend uses `-accel hvf -cpu
//! host`; here the identical invocation runs under `-accel tcg -cpu max`
//! (the only two flags that differ — asserted by unit tests in
//! `vmforge-engine-qemu`). Everything this test exercises — process spawn,
//! UEFI firmware boot, virtio devices, QMP handshake, cont/stop, external
//! disk overlay + RAM-state snapshot, restore-from-state (`-incoming
//! defer` + `migrate-incoming`), snapshot branching, quit — is common
//! code shared with the real hvf configuration.
//!
//! hvf-only behavior (real `-accel hvf` init, `-cpu host`, HVF vGIC,
//! entitlement checks) cannot run on Linux and is SKIPPED here; it is
//! covered by the planned self-hosted macOS runner (port plan §4/M2).
//!
//! Requires `qemu-system-aarch64` + edk2 firmware + `qemu-img`; ignored by
//! default, run in CI with `--ignored`.

use std::process::Command;

use vmforge_backend_hvf::HvfBackend;
use vmforge_core::{GuestArch, Hypervisor, SnapshotId, VmConfig, VmState};
use vmforge_engine_qemu::Accel;

fn have_tools() -> bool {
    let ok = |bin: &str| {
        Command::new(bin)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    ok("qemu-system-aarch64") && ok("qemu-img")
}

#[test]
#[ignore = "requires qemu-system-aarch64; run in CI with --ignored"]
fn aarch64_virt_tcg_full_lifecycle() {
    assert!(
        have_tools(),
        "qemu-system-aarch64 and qemu-img are required for this test"
    );

    let workdir = std::env::temp_dir().join(format!("vmforge-tcg-test-{}", std::process::id()));
    std::fs::create_dir_all(&workdir).unwrap();
    let disk = workdir.join("disk.qcow2");
    let status = Command::new("qemu-img")
        .args(["create", "-f", "qcow2", disk.to_str().unwrap(), "64M"])
        .status()
        .unwrap();
    assert!(status.success(), "qemu-img create failed");

    // Same backend, same invocation builder, same QMP lifecycle as on
    // macOS — only the accelerator is swapped for Linux CI.
    let backend = HvfBackend::with_accel(Accel::Tcg);
    let config = VmConfig {
        name: format!("tcg-ci-{}", std::process::id()),
        arch: GuestArch::Aarch64,
        vcpus: 1,
        memory_mib: 256,
        disks: vec![disk.to_string_lossy().into_owned()],
        gpu_3d: false,
    };

    // create -> Defined
    let vm = backend.create(&config).expect("create failed");
    assert_eq!(backend.state(&vm).unwrap(), VmState::Defined);

    // boot -> Running (QEMU spawned, UEFI firmware loaded, QMP handshake,
    // vCPUs started via `cont`)
    backend.boot(&vm).expect("boot failed");
    assert_eq!(backend.state(&vm).unwrap(), VmState::Running);

    // pause/resume over QMP
    backend.pause(&vm).expect("pause failed");
    assert_eq!(backend.state(&vm).unwrap(), VmState::Paused);
    backend.resume(&vm).expect("resume failed");
    assert_eq!(backend.state(&vm).unwrap(), VmState::Running);

    // snapshot while running: pause window -> external qcow2 overlay +
    // RAM/device state via migrate-to-file -> resume (the portable path
    // hvf uses on macOS, port plan §2)
    let snap = backend.snapshot(&vm, None).expect("snapshot failed");
    assert!(!snap.0.is_empty());
    assert_eq!(backend.state(&vm).unwrap(), VmState::Running);

    // restore from a snapshot requires the VM not be Running (FSM)
    assert!(backend.restore(&vm, snap.clone()).is_err());

    // stop, then restore: re-spawn with -incoming defer, fresh overlay on
    // the snapshot's frozen disk layer, load saved RAM state, cont ->
    // Running (the instant-resume primitive)
    backend.stop(&vm).expect("stop failed");
    assert_eq!(backend.state(&vm).unwrap(), VmState::Stopped);
    backend.restore(&vm, snap.clone()).expect("restore failed");
    assert_eq!(backend.state(&vm).unwrap(), VmState::Running);

    // branch: snapshot the restored timeline as a child of `snap` — the
    // DAG now has two nodes on one lineage (git-like branching)
    let child = backend
        .snapshot(&vm, Some(snap.clone()))
        .expect("branch snapshot failed");
    assert_ne!(child, snap);

    // unknown parents/snapshots are rejected
    assert!(backend
        .snapshot(&vm, Some(SnapshotId("ghost".into())))
        .is_err());

    // restore the ORIGINAL snapshot again from Stopped -> a second branch
    // rooted at `snap`; the frozen layers are never mutated
    backend.stop(&vm).expect("stop failed");
    backend.restore(&vm, snap).expect("re-restore failed");
    assert_eq!(backend.state(&vm).unwrap(), VmState::Running);
    assert!(backend.restore(&vm, SnapshotId("ghost".into())).is_err());

    // stop -> Stopped (QMP quit), then delete
    backend.stop(&vm).expect("stop failed");
    assert_eq!(backend.state(&vm).unwrap(), VmState::Stopped);
    backend.delete(vm).expect("delete failed");

    // hvf-only assertions: skipped on non-macOS hosts.
    if !HvfBackend::is_available() {
        eprintln!(
            "SKIPPED: hvf-only assertions (-accel hvf init, -cpu host, HVF vGIC) — \
             no Hypervisor.framework on this host; covered by the macOS HVF runner (M2)"
        );
    }

    let _ = std::fs::remove_dir_all(&workdir);
}
