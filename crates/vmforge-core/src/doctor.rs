//! Host preflight probes ("doctor") mapping failures onto the error taxonomy.
//!
//! Each probe returns `Ok(summary)` or an [`EngineError`] with a stable code,
//! message and recovery hint. The CLI (`vmforge doctor`) and GUI use these to
//! diagnose kvm-unavailable / kvm-permission-denied / qemu-binary-missing /
//! disk-full / disk-image-missing / disk-image-corrupt before boot.
//!
//! Test-only injection knobs (documented in `docs/error-taxonomy.md`):
//! `VMFORGE_KVM_PATH`, `VMFORGE_QEMU_BIN`, `VMFORGE_MIN_FREE_BYTES` — they let
//! CI exercise every probe failure without special hardware.

use std::path::{Path, PathBuf};

use crate::taxonomy::{check_qcow2, EngineError, ErrorClass};

/// Default minimum free space required under `$VMFORGE_HOME` (512 MiB).
pub const DEFAULT_MIN_FREE_BYTES: u64 = 512 * 1024 * 1024;

/// One probe outcome: name plus pass summary or classified failure.
pub struct ProbeResult {
    pub name: &'static str,
    pub result: Result<String, EngineError>,
}

/// KVM device node to probe (`VMFORGE_KVM_PATH` overrides, tests only).
pub fn kvm_path() -> PathBuf {
    std::env::var_os("VMFORGE_KVM_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/dev/kvm"))
}

/// Probe KVM availability: device exists and is read/write accessible.
pub fn probe_kvm() -> Result<String, EngineError> {
    let path = kvm_path();
    if !path.exists() {
        return Err(EngineError::new(
            ErrorClass::KvmUnavailable,
            format!("{} does not exist", path.display()),
        ));
    }
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
    {
        Ok(_) => Ok(format!("{} is accessible", path.display())),
        Err(e) => Err(EngineError::new(
            ErrorClass::KvmPermissionDenied,
            format!("cannot open {} read/write: {}", path.display(), e),
        )),
    }
}

/// Probe for a usable QEMU system emulator.
///
/// `VMFORGE_QEMU_BIN` (path or name) overrides; otherwise searches `$PATH`
/// for `qemu-system-x86_64` / `qemu-system-aarch64`.
pub fn probe_qemu() -> Result<String, EngineError> {
    if let Some(bin) = std::env::var_os("VMFORGE_QEMU_BIN") {
        let bin = PathBuf::from(bin);
        if bin.is_absolute() || bin.components().count() > 1 {
            if bin.is_file() {
                return Ok(format!("QEMU binary: {}", bin.display()));
            }
            return Err(EngineError::new(
                ErrorClass::QemuBinaryMissing,
                format!("VMFORGE_QEMU_BIN={} does not exist", bin.display()),
            ));
        }
        return find_in_path(&bin)
            .map(|p| format!("QEMU binary: {}", p.display()))
            .ok_or_else(|| {
                EngineError::new(
                    ErrorClass::QemuBinaryMissing,
                    format!("VMFORGE_QEMU_BIN={} not found in PATH", bin.display()),
                )
            });
    }
    for name in ["qemu-system-x86_64", "qemu-system-aarch64"] {
        if let Some(p) = find_in_path(Path::new(name)) {
            return Ok(format!("QEMU binary: {}", p.display()));
        }
    }
    Err(EngineError::of(ErrorClass::QemuBinaryMissing))
}

fn find_in_path(name: &Path) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Probe that `home` is writable and has at least the required free space
/// (`VMFORGE_MIN_FREE_BYTES` overrides the threshold, tests only).
pub fn probe_home_space(home: &Path) -> Result<String, EngineError> {
    std::fs::create_dir_all(home).map_err(|e| {
        EngineError::new(
            ErrorClass::Internal,
            format!("cannot create {}: {}", home.display(), e),
        )
    })?;

    let probe = home.join(".vmforge-write-probe");
    if let Err(e) = std::fs::write(&probe, b"probe") {
        return Err(crate::taxonomy::classify_io(
            &e,
            &format!("writing to {}", home.display()),
        ));
    }
    std::fs::remove_file(&probe).ok();

    let min_free = std::env::var("VMFORGE_MIN_FREE_BYTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_MIN_FREE_BYTES);
    let free = free_bytes(home)?;
    if free < min_free {
        return Err(EngineError::new(
            ErrorClass::DiskFull,
            format!(
                "only {free} bytes free on the volume holding {} (need at least {min_free})",
                home.display()
            ),
        )
        .with_details(serde_json::json!({ "free_bytes": free, "min_free_bytes": min_free })));
    }
    Ok(format!("{} writable, {free} bytes free", home.display()))
}

#[cfg(unix)]
fn free_bytes(path: &Path) -> Result<u64, EngineError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        EngineError::new(ErrorClass::Internal, "path contains NUL byte".to_string())
    })?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut stat) };
    if rc != 0 {
        return Err(crate::taxonomy::classify_io(
            &std::io::Error::last_os_error(),
            &format!("statvfs {}", path.display()),
        ));
    }
    // Casts are needed on targets where these fields are not u64.
    #[allow(clippy::unnecessary_cast)]
    Ok(stat.f_bavail as u64 * stat.f_frsize as u64)
}

#[cfg(not(unix))]
fn free_bytes(_path: &Path) -> Result<u64, EngineError> {
    Ok(u64::MAX) // free-space probe is Unix-only for wave 1 (Linux beta)
}

/// Probe a disk image: exists and is a structurally valid qcow2 file.
pub fn probe_disk(path: &Path) -> Result<String, EngineError> {
    check_qcow2(path)?;
    Ok(format!("{} is a valid qcow2 image", path.display()))
}

/// Run all host probes (plus optional disk checks), in a stable order.
pub fn run_all(home: &Path, disks: &[PathBuf]) -> Vec<ProbeResult> {
    let mut results = vec![
        ProbeResult {
            name: "kvm",
            result: probe_kvm(),
        },
        ProbeResult {
            name: "qemu",
            result: probe_qemu(),
        },
        ProbeResult {
            name: "home",
            result: probe_home_space(home),
        },
    ];
    for disk in disks {
        results.push(ProbeResult {
            name: "disk",
            result: probe_disk(disk),
        });
    }
    results
}
