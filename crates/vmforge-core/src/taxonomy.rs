//! Structured error taxonomy for engine failure paths (boot/KVM/disk).
//!
//! Every user-visible engine failure maps to an [`ErrorClass`] with a stable
//! machine-readable code, a human message, a suggested recovery action and a
//! distinct CLI exit code, so the CLI and GUI can present actionable guidance
//! instead of raw QEMU/ioctl output. Wire form follows the interface contract
//! (`docs/interface-contracts.md` §0):
//! `{"error": {"code", "message", "recovery", "details"}}` on stderr.
//!
//! Full table: `docs/error-taxonomy.md`.

use std::io::ErrorKind as IoKind;
use std::path::Path;

use crate::error::HvError;

/// Stable engine failure classes. Codes and exit codes are frozen once
/// released (additive-only per the wave-1 CLI freeze).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// `/dev/kvm` does not exist (no KVM module / nested virt disabled).
    KvmUnavailable,
    /// `/dev/kvm` exists but the user lacks read/write permission.
    KvmPermissionDenied,
    /// No usable `qemu-system-*` binary on the host.
    QemuBinaryMissing,
    /// Guest did not become ready within the boot timeout.
    BootTimeout,
    /// QEMU exited unexpectedly (crash or early exit).
    QemuCrashed,
    /// Host volume holding VM state is out of space.
    DiskFull,
    /// A referenced disk image does not exist.
    DiskImageMissing,
    /// A disk image exists but is not a valid qcow2 file.
    DiskImageCorrupt,
    /// Snapshot operation conflicts with existing state
    /// (duplicate tag, missing tag on restore/delete, or wrong VM state).
    SnapshotConflict,
    /// A host port the VM needs (VNC, port-forward, QMP TCP) is taken.
    PortInUse,
    /// Anything not classified above; details carry the raw error.
    Internal,
}

impl ErrorClass {
    /// Every class, in exit-code order.
    pub const ALL: [ErrorClass; 11] = [
        ErrorClass::KvmUnavailable,
        ErrorClass::KvmPermissionDenied,
        ErrorClass::QemuBinaryMissing,
        ErrorClass::BootTimeout,
        ErrorClass::QemuCrashed,
        ErrorClass::DiskFull,
        ErrorClass::DiskImageMissing,
        ErrorClass::DiskImageCorrupt,
        ErrorClass::SnapshotConflict,
        ErrorClass::PortInUse,
        ErrorClass::Internal,
    ];

    /// Stable machine-readable code (snake_case per contract §0).
    pub fn code(self) -> &'static str {
        match self {
            ErrorClass::KvmUnavailable => "kvm_unavailable",
            ErrorClass::KvmPermissionDenied => "kvm_permission_denied",
            ErrorClass::QemuBinaryMissing => "qemu_binary_missing",
            ErrorClass::BootTimeout => "boot_timeout",
            ErrorClass::QemuCrashed => "qemu_crashed",
            ErrorClass::DiskFull => "disk_full",
            ErrorClass::DiskImageMissing => "disk_image_missing",
            ErrorClass::DiskImageCorrupt => "disk_image_corrupt",
            ErrorClass::SnapshotConflict => "snapshot_conflict",
            ErrorClass::PortInUse => "port_in_use",
            ErrorClass::Internal => "internal",
        }
    }

    /// Distinct, stable CLI exit code for this class.
    ///
    /// 0-2 stay as frozen (0 success, 1 generic error, 2 usage); taxonomy
    /// classes occupy 10-20 so new generic failures never collide.
    pub fn exit_code(self) -> i32 {
        match self {
            ErrorClass::KvmUnavailable => 10,
            ErrorClass::KvmPermissionDenied => 11,
            ErrorClass::QemuBinaryMissing => 12,
            ErrorClass::BootTimeout => 13,
            ErrorClass::QemuCrashed => 14,
            ErrorClass::DiskFull => 15,
            ErrorClass::DiskImageMissing => 16,
            ErrorClass::DiskImageCorrupt => 17,
            ErrorClass::SnapshotConflict => 18,
            ErrorClass::PortInUse => 19,
            ErrorClass::Internal => 20,
        }
    }

    /// Default user-facing message.
    pub fn message(self) -> &'static str {
        match self {
            ErrorClass::KvmUnavailable => "KVM is not available on this host (/dev/kvm missing)",
            ErrorClass::KvmPermissionDenied => "permission denied opening /dev/kvm",
            ErrorClass::QemuBinaryMissing => "QEMU binary not found on this host",
            ErrorClass::BootTimeout => "the VM did not become ready before the boot timeout",
            ErrorClass::QemuCrashed => "QEMU exited unexpectedly",
            ErrorClass::DiskFull => "not enough free disk space for VM state",
            ErrorClass::DiskImageMissing => "disk image not found",
            ErrorClass::DiskImageCorrupt => "disk image is not a valid qcow2 file",
            ErrorClass::SnapshotConflict => "snapshot operation conflicts with existing state",
            ErrorClass::PortInUse => "a required host port is already in use",
            ErrorClass::Internal => "internal engine error",
        }
    }

    /// Suggested recovery action, phrased for direct display in CLI/GUI.
    pub fn recovery(self) -> &'static str {
        match self {
            ErrorClass::KvmUnavailable => {
                "Enable virtualization (VT-x/AMD-V) in firmware and load the kvm module \
                 (`modprobe kvm_intel` or `kvm_amd`); inside a VM, enable nested \
                 virtualization. VMForge can fall back to slow TCG emulation."
            }
            ErrorClass::KvmPermissionDenied => {
                "Add your user to the kvm group (`sudo usermod -aG kvm $USER`) and re-login, \
                 or fix permissions on /dev/kvm."
            }
            ErrorClass::QemuBinaryMissing => {
                "Install QEMU (e.g. `sudo apt install qemu-system-x86`) or set VMFORGE_QEMU_BIN \
                 to the qemu-system binary."
            }
            ErrorClass::BootTimeout => {
                "Check the serial log under the VM state dir for guest boot errors, verify the \
                 disk image is bootable, and retry with a larger timeout."
            }
            ErrorClass::QemuCrashed => {
                "Inspect the QEMU stderr in the error details and the VM serial log; verify \
                 host QEMU version and VM configuration, then retry."
            }
            ErrorClass::DiskFull => {
                "Free space on the volume holding $VMFORGE_HOME (delete unused VMs/snapshots \
                 or move VMFORGE_HOME to a larger disk), then retry."
            }
            ErrorClass::DiskImageMissing => {
                "Check the disk path in the VM config; re-import or re-create the disk \
                 (`vmforge-storage create`/`import`)."
            }
            ErrorClass::DiskImageCorrupt => {
                "Run `vmforge-storage check <vm> <disk> --repair`, or restore the disk from a \
                 snapshot or backup."
            }
            ErrorClass::SnapshotConflict => {
                "List snapshots (`vmforge snapshot list <vm>`) and pick an unused tag for \
                 create or an existing tag for restore/delete; stop the VM first for offline \
                 snapshot operations."
            }
            ErrorClass::PortInUse => {
                "Find the process holding the port (`ss -ltnp`) and stop it, or configure a \
                 different port."
            }
            ErrorClass::Internal => {
                "Retry the operation; if it persists, report a bug with the error details \
                 (`docs/tester-guide/reporting-bugs.md`)."
            }
        }
    }

    /// Parse a stable code back into its class.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.code() == code)
    }
}

/// A classified engine failure: stable class + concrete message + details.
#[derive(Debug)]
pub struct EngineError {
    pub class: ErrorClass,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl EngineError {
    pub fn new(class: ErrorClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
            details: None,
        }
    }

    /// Use the class's default user-facing message.
    pub fn of(class: ErrorClass) -> Self {
        Self::new(class, class.message())
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Contract §0 wire form: `{"error": {"code", "message", "recovery", "details"}}`.
    pub fn to_json(&self) -> serde_json::Value {
        let mut error = serde_json::json!({
            "code": self.class.code(),
            "message": self.message,
            "recovery": self.class.recovery(),
        });
        if let Some(details) = &self.details {
            error["details"] = details.clone();
        }
        serde_json::json!({ "error": error })
    }

    pub fn exit_code(&self) -> i32 {
        self.class.exit_code()
    }
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.class.code())
    }
}

impl std::error::Error for EngineError {}

impl From<HvError> for EngineError {
    fn from(e: HvError) -> Self {
        match &e {
            HvError::Unsupported { backend, reason } if *backend == "kvm" => {
                let class = if reason.contains("permission") || reason.contains("denied") {
                    ErrorClass::KvmPermissionDenied
                } else {
                    ErrorClass::KvmUnavailable
                };
                EngineError::new(class, e.to_string())
            }
            HvError::Io(io) => classify_io(io, &e.to_string()),
            HvError::InvalidTransition { .. } => {
                EngineError::new(ErrorClass::SnapshotConflict, e.to_string())
            }
            _ => EngineError::new(ErrorClass::Internal, e.to_string()),
        }
    }
}

/// Classify a raw I/O error into the taxonomy.
pub fn classify_io(err: &std::io::Error, context: &str) -> EngineError {
    let class = match err.kind() {
        IoKind::AddrInUse => ErrorClass::PortInUse,
        IoKind::NotFound => ErrorClass::DiskImageMissing,
        IoKind::PermissionDenied if context.contains("/dev/kvm") => ErrorClass::KvmPermissionDenied,
        _ if err.raw_os_error() == Some(libc_enospc()) => ErrorClass::DiskFull,
        _ => ErrorClass::Internal,
    };
    EngineError::new(class, format!("{context}: {err}"))
}

const fn libc_enospc() -> i32 {
    28 // ENOSPC on Linux
}

/// Classify QEMU stderr output (from a crashed or refusing-to-start QEMU).
pub fn classify_qemu_stderr(stderr: &str) -> EngineError {
    let s = stderr.to_ascii_lowercase();
    let class = if s.contains("could not access kvm kernel module") || s.contains("/dev/kvm") {
        if s.contains("permission denied") {
            ErrorClass::KvmPermissionDenied
        } else {
            ErrorClass::KvmUnavailable
        }
    } else if s.contains("no space left on device") {
        ErrorClass::DiskFull
    } else if s.contains("no such file or directory") && s.contains("could not open") {
        ErrorClass::DiskImageMissing
    } else if s.contains("image is corrupt")
        || s.contains("not in qcow2 format")
        || s.contains("could not read qcow2 header")
        || s.contains("invalid or corrupt")
    {
        ErrorClass::DiskImageCorrupt
    } else if s.contains("address already in use") || s.contains("failed to find an available port")
    {
        ErrorClass::PortInUse
    } else if s.contains("snapshot")
        && (s.contains("already exists") || s.contains("does not exist") || s.contains("not found"))
    {
        ErrorClass::SnapshotConflict
    } else {
        ErrorClass::QemuCrashed
    };
    EngineError::new(class, class.message())
        .with_details(serde_json::json!({ "qemu_stderr": stderr.trim() }))
}

/// Construct a boot-timeout error.
pub fn boot_timeout(vm: &str, timeout_secs: u64) -> EngineError {
    EngineError::new(
        ErrorClass::BootTimeout,
        format!("VM '{vm}' did not become ready within {timeout_secs}s"),
    )
}

/// Construct a snapshot-conflict error.
pub fn snapshot_conflict(vm: &str, tag: &str, reason: &str) -> EngineError {
    EngineError::new(
        ErrorClass::SnapshotConflict,
        format!("snapshot '{tag}' on VM '{vm}': {reason}"),
    )
}

/// Validate that `path` exists and looks like a qcow2 image
/// (magic `QFI\xfb`, version 2 or 3, plausible header).
pub fn check_qcow2(path: &Path) -> Result<(), EngineError> {
    use std::io::Read;

    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == IoKind::NotFound => {
            return Err(EngineError::new(
                ErrorClass::DiskImageMissing,
                format!("disk image not found: {}", path.display()),
            ));
        }
        Err(e) => return Err(classify_io(&e, &format!("opening {}", path.display()))),
    };
    let mut header = [0u8; 8];
    let corrupt = |why: &str| {
        EngineError::new(
            ErrorClass::DiskImageCorrupt,
            format!("{}: {}", path.display(), why),
        )
    };
    match f.read_exact(&mut header) {
        Ok(()) => {}
        Err(_) => return Err(corrupt("file too short for a qcow2 header")),
    }
    if &header[0..4] != b"QFI\xfb" {
        return Err(corrupt("bad qcow2 magic"));
    }
    let version = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
    if !(2..=3).contains(&version) {
        return Err(corrupt("unsupported qcow2 version"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_exit_codes_are_distinct_and_stable() {
        let mut codes: Vec<&str> = ErrorClass::ALL.iter().map(|c| c.code()).collect();
        let mut exits: Vec<i32> = ErrorClass::ALL.iter().map(|c| c.exit_code()).collect();
        codes.sort_unstable();
        codes.dedup();
        exits.sort_unstable();
        exits.dedup();
        assert_eq!(codes.len(), ErrorClass::ALL.len());
        assert_eq!(exits.len(), ErrorClass::ALL.len());
        // Frozen values: changing any of these breaks scripts and the GUI.
        assert_eq!(ErrorClass::KvmUnavailable.exit_code(), 10);
        assert_eq!(ErrorClass::KvmPermissionDenied.exit_code(), 11);
        assert_eq!(ErrorClass::QemuBinaryMissing.exit_code(), 12);
        assert_eq!(ErrorClass::BootTimeout.exit_code(), 13);
        assert_eq!(ErrorClass::QemuCrashed.exit_code(), 14);
        assert_eq!(ErrorClass::DiskFull.exit_code(), 15);
        assert_eq!(ErrorClass::DiskImageMissing.exit_code(), 16);
        assert_eq!(ErrorClass::DiskImageCorrupt.exit_code(), 17);
        assert_eq!(ErrorClass::SnapshotConflict.exit_code(), 18);
        assert_eq!(ErrorClass::PortInUse.exit_code(), 19);
        assert_eq!(ErrorClass::Internal.exit_code(), 20);
    }

    #[test]
    fn from_code_round_trips() {
        for class in ErrorClass::ALL {
            assert_eq!(ErrorClass::from_code(class.code()), Some(class));
        }
        assert_eq!(ErrorClass::from_code("nope"), None);
    }

    #[test]
    fn json_shape_matches_contract() {
        let e =
            EngineError::of(ErrorClass::DiskFull).with_details(serde_json::json!({"path": "/tmp"}));
        let v = e.to_json();
        assert_eq!(v["error"]["code"], "disk_full");
        assert!(v["error"]["message"].is_string());
        assert!(v["error"]["recovery"].is_string());
        assert_eq!(v["error"]["details"]["path"], "/tmp");
    }

    #[test]
    fn classify_io_maps_kinds() {
        let e = classify_io(
            &std::io::Error::new(IoKind::AddrInUse, "in use"),
            "binding vnc port",
        );
        assert_eq!(e.class, ErrorClass::PortInUse);

        let e = classify_io(
            &std::io::Error::new(IoKind::PermissionDenied, "denied"),
            "opening /dev/kvm",
        );
        assert_eq!(e.class, ErrorClass::KvmPermissionDenied);

        let e = classify_io(&std::io::Error::from_raw_os_error(28), "writing snapshot");
        assert_eq!(e.class, ErrorClass::DiskFull);

        let e = classify_io(
            &std::io::Error::new(IoKind::NotFound, "gone"),
            "opening disk",
        );
        assert_eq!(e.class, ErrorClass::DiskImageMissing);
    }

    #[test]
    fn classify_qemu_stderr_patterns() {
        let cases = [
            (
                "Could not access KVM kernel module: No such file or directory",
                ErrorClass::KvmUnavailable,
            ),
            (
                "Could not access KVM kernel module: Permission denied",
                ErrorClass::KvmPermissionDenied,
            ),
            (
                "qemu-system-x86_64: disk.qcow2: No space left on device",
                ErrorClass::DiskFull,
            ),
            (
                "qemu-system-x86_64: Could not open 'disk.qcow2': No such file or directory",
                ErrorClass::DiskImageMissing,
            ),
            (
                "qemu-system-x86_64: 'disk.qcow2': Image is not in qcow2 format",
                ErrorClass::DiskImageCorrupt,
            ),
            (
                "qemu-system-x86_64: -vnc :1: Failed to listen on socket: Address already in use",
                ErrorClass::PortInUse,
            ),
            ("Segmentation fault (core dumped)", ErrorClass::QemuCrashed),
        ];
        for (stderr, class) in cases {
            assert_eq!(classify_qemu_stderr(stderr).class, class, "{stderr}");
        }
    }

    #[test]
    fn qcow2_check_detects_missing_and_corrupt() {
        let dir = std::env::temp_dir().join(format!("vmforge-taxo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let missing = dir.join("missing.qcow2");
        assert_eq!(
            check_qcow2(&missing).unwrap_err().class,
            ErrorClass::DiskImageMissing
        );

        let truncated = dir.join("truncated.qcow2");
        std::fs::write(&truncated, b"QFI").unwrap();
        assert_eq!(
            check_qcow2(&truncated).unwrap_err().class,
            ErrorClass::DiskImageCorrupt
        );

        let garbage = dir.join("garbage.qcow2");
        std::fs::write(&garbage, b"not a qcow2 image at all").unwrap();
        assert_eq!(
            check_qcow2(&garbage).unwrap_err().class,
            ErrorClass::DiskImageCorrupt
        );

        let valid = dir.join("valid.qcow2");
        let mut header = Vec::from(&b"QFI\xfb"[..]);
        header.extend_from_slice(&3u32.to_be_bytes());
        std::fs::write(&valid, header).unwrap();
        assert!(check_qcow2(&valid).is_ok());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hv_error_maps_to_taxonomy() {
        let e: EngineError = HvError::Unsupported {
            backend: "kvm",
            reason: "/dev/kvm not present".into(),
        }
        .into();
        assert_eq!(e.class, ErrorClass::KvmUnavailable);

        let e: EngineError = HvError::Unsupported {
            backend: "kvm",
            reason: "permission denied on /dev/kvm".into(),
        }
        .into();
        assert_eq!(e.class, ErrorClass::KvmPermissionDenied);
    }
}
