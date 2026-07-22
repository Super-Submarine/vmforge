//! Engine ⟷ Storage boundary (docs/interface-contracts.md §1).
//!
//! [`StorageProvider`] is the trait the engine programs against. The M1
//! implementation is [`SubprocessStore`], which shells out to the
//! `vmforge-storage` CLI with `--json`; a native Rust implementation can
//! replace it post-M1 without callers noticing.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Storage interface contract major version (contract §1, invariant S4).
pub const STORAGE_CONTRACT_VERSION: &str = "1";

/// Shared name regex: `[A-Za-z0-9][A-Za-z0-9._-]*`.
fn valid_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

macro_rules! name_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Result<Self, VmError> {
                let s = s.into();
                if valid_name(&s) {
                    Ok(Self(s))
                } else {
                    Err(VmError {
                        kind: ErrorKind::InvalidConfig,
                        message: format!(
                            concat!("invalid ", stringify!($name), ": {:?}"),
                            s
                        ),
                        details: None,
                    })
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

name_type!(
    /// VM name (`[A-Za-z0-9][A-Za-z0-9._-]*`).
    VmName
);
name_type!(
    /// Disk name (same regex as [`VmName`]).
    DiskName
);
name_type!(
    /// Snapshot name (same regex as [`VmName`]).
    SnapshotName
);
name_type!(
    /// Base image name (same regex as [`VmName`]).
    ImageName
);

/// Stable machine-readable error kinds shared across subsystems (contract §0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    NotFound,
    AlreadyExists,
    InvalidConfig,
    InvalidState,
    Backend,
    Timeout,
    Io,
}

impl ErrorKind {
    fn from_code(code: &str) -> Self {
        match code {
            "not_found" => Self::NotFound,
            "already_exists" => Self::AlreadyExists,
            "invalid_config" => Self::InvalidConfig,
            "invalid_state" => Self::InvalidState,
            "timeout" => Self::Timeout,
            "io" => Self::Io,
            _ => Self::Backend,
        }
    }
}

/// Error crossing the engine ⟷ storage boundary (contract §0).
#[derive(Debug)]
pub struct VmError {
    pub kind: ErrorKind,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for VmError {}

/// One node of the snapshot tree, derived purely from qcow2 backing-file
/// metadata (invariant S3).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SnapshotInfo {
    pub name: String,
    pub parent: Option<String>,
    #[serde(default)]
    pub children: Vec<String>,
    #[serde(default)]
    pub current: bool,
    #[serde(default)]
    pub virtual_size: u64,
    #[serde(default)]
    pub actual_size: u64,
}

/// Result of importing a base image.
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub name: ImageName,
    pub path: PathBuf,
}

/// Result of creating a disk.
#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub vm: VmName,
    pub disk: DiskName,
    pub path: PathBuf,
}

/// Storage operations the engine consumes (contract §1).
///
/// Disk-level snapshot/revert are OFFLINE-only: the engine must verify the
/// VM is not RUNNING/PAUSED before calling (invariant S2) — storage trusts
/// the engine.
pub trait StorageProvider: Send + Sync {
    fn import_image(&self, src: &Path, name: &ImageName) -> Result<ImageInfo, VmError>;
    fn create_disk(
        &self,
        vm: &VmName,
        disk: &DiskName,
        size_bytes: u64,
        base: Option<&ImageName>,
    ) -> Result<DiskInfo, VmError>;
    /// Path QEMU opens: `vms/<vm>/disks/<disk>.qcow2` — the active overlay is
    /// the ONLY file QEMU ever opens (invariant S1).
    fn attach_path(&self, vm: &VmName, disk: &DiskName) -> PathBuf;
    fn resize_disk(&self, vm: &VmName, disk: &DiskName, size_bytes: u64) -> Result<(), VmError>;
    fn snapshot(
        &self,
        vm: &VmName,
        disk: &DiskName,
        name: &SnapshotName,
    ) -> Result<SnapshotInfo, VmError>;
    fn revert(&self, vm: &VmName, disk: &DiskName, snapshot: &SnapshotName) -> Result<(), VmError>;
    fn delete_snapshot(
        &self,
        vm: &VmName,
        disk: &DiskName,
        snapshot: &SnapshotName,
    ) -> Result<(), VmError>;
    fn tree(&self, vm: &VmName, disk: &DiskName) -> Result<Vec<SnapshotInfo>, VmError>;
    fn delete_disk(&self, vm: &VmName, disk: &DiskName) -> Result<(), VmError>;
}

/// M1 [`StorageProvider`]: shells out to `vmforge-storage --json`.
pub struct SubprocessStore {
    binary: String,
    home: PathBuf,
}

impl SubprocessStore {
    /// Create a store rooted at `home` (`$VMFORGE_HOME`), verifying the CLI
    /// speaks a known contract major version (invariant S4).
    pub fn new(home: impl Into<PathBuf>) -> Result<Self, VmError> {
        let store = Self::new_unchecked(home);
        let version = store.contract_version()?;
        if version != STORAGE_CONTRACT_VERSION {
            return Err(VmError {
                kind: ErrorKind::Backend,
                message: format!(
                    "vmforge-storage contract version {version:?} is not supported \
                     (engine speaks {STORAGE_CONTRACT_VERSION:?})"
                ),
                details: None,
            });
        }
        Ok(store)
    }

    /// Like [`SubprocessStore::new`] but without the contract handshake.
    pub fn new_unchecked(home: impl Into<PathBuf>) -> Self {
        Self {
            binary: std::env::var("VMFORGE_STORAGE_BIN")
                .unwrap_or_else(|_| "vmforge-storage".to_string()),
            home: home.into(),
        }
    }

    /// `vmforge-storage --contract-version`.
    pub fn contract_version(&self) -> Result<String, VmError> {
        let out = Command::new(&self.binary)
            .arg("--contract-version")
            .output()
            .map_err(|e| VmError {
                kind: ErrorKind::Io,
                message: format!("failed to run {}: {e}", self.binary),
                details: None,
            })?;
        if !out.status.success() {
            return Err(VmError {
                kind: ErrorKind::Backend,
                message: "vmforge-storage --contract-version failed".to_string(),
                details: Some(serde_json::json!({
                    "stderr": String::from_utf8_lossy(&out.stderr),
                })),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn run(&self, args: &[&str]) -> Result<serde_json::Value, VmError> {
        let out = Command::new(&self.binary)
            .arg("--home")
            .arg(&self.home)
            .arg("--json")
            .args(args)
            .output()
            .map_err(|e| VmError {
                kind: ErrorKind::Io,
                message: format!("failed to run {}: {e}", self.binary),
                details: None,
            })?;
        if out.status.success() {
            return serde_json::from_slice(&out.stdout).map_err(|e| VmError {
                kind: ErrorKind::Backend,
                message: format!("vmforge-storage produced invalid JSON: {e}"),
                details: Some(serde_json::json!({
                    "stdout": String::from_utf8_lossy(&out.stdout),
                })),
            });
        }
        // Failure: nonzero exit + {"error": {code, message, details?}} on stderr.
        let stderr = String::from_utf8_lossy(&out.stderr);
        if let Ok(serde_json::Value::Object(obj)) =
            serde_json::from_str::<serde_json::Value>(stderr.trim())
        {
            if let Some(err) = obj.get("error") {
                return Err(VmError {
                    kind: ErrorKind::from_code(
                        err.get("code").and_then(|c| c.as_str()).unwrap_or(""),
                    ),
                    message: err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("storage operation failed")
                        .to_string(),
                    details: err.get("details").cloned(),
                });
            }
        }
        Err(VmError {
            kind: ErrorKind::Backend,
            message: format!("vmforge-storage exited with {}", out.status),
            details: Some(serde_json::json!({ "stderr": stderr })),
        })
    }
}

impl StorageProvider for SubprocessStore {
    fn import_image(&self, src: &Path, name: &ImageName) -> Result<ImageInfo, VmError> {
        let src = src.to_string_lossy().into_owned();
        let value = self.run(&["import", &src, "--name", name.as_str()])?;
        let path = value
            .get("path")
            .and_then(|p| p.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.join("images").join(format!("{name}.qcow2")));
        Ok(ImageInfo {
            name: name.clone(),
            path,
        })
    }

    fn create_disk(
        &self,
        vm: &VmName,
        disk: &DiskName,
        size_bytes: u64,
        base: Option<&ImageName>,
    ) -> Result<DiskInfo, VmError> {
        let size = size_bytes.to_string();
        match base {
            None => self.run(&["create", vm.as_str(), disk.as_str(), &size])?,
            Some(image) => {
                let mut args = vec!["clone", image.as_str(), vm.as_str(), disk.as_str()];
                if size_bytes > 0 {
                    args.extend(["--size", &size]);
                }
                self.run(&args)?
            }
        };
        Ok(DiskInfo {
            vm: vm.clone(),
            disk: disk.clone(),
            path: self.attach_path(vm, disk),
        })
    }

    fn attach_path(&self, vm: &VmName, disk: &DiskName) -> PathBuf {
        self.home
            .join("vms")
            .join(vm.as_str())
            .join("disks")
            .join(format!("{disk}.qcow2"))
    }

    fn resize_disk(&self, vm: &VmName, disk: &DiskName, size_bytes: u64) -> Result<(), VmError> {
        self.run(&[
            "resize",
            vm.as_str(),
            disk.as_str(),
            &size_bytes.to_string(),
        ])?;
        Ok(())
    }

    fn snapshot(
        &self,
        vm: &VmName,
        disk: &DiskName,
        name: &SnapshotName,
    ) -> Result<SnapshotInfo, VmError> {
        let value = self.run(&[
            "snapshot",
            "create",
            vm.as_str(),
            disk.as_str(),
            name.as_str(),
        ])?;
        serde_json::from_value(value).map_err(|e| VmError {
            kind: ErrorKind::Backend,
            message: format!("unexpected snapshot-create output: {e}"),
            details: None,
        })
    }

    fn revert(&self, vm: &VmName, disk: &DiskName, snapshot: &SnapshotName) -> Result<(), VmError> {
        self.run(&[
            "snapshot",
            "revert",
            vm.as_str(),
            disk.as_str(),
            snapshot.as_str(),
        ])?;
        Ok(())
    }

    fn delete_snapshot(
        &self,
        vm: &VmName,
        disk: &DiskName,
        snapshot: &SnapshotName,
    ) -> Result<(), VmError> {
        self.run(&[
            "snapshot",
            "delete",
            vm.as_str(),
            disk.as_str(),
            snapshot.as_str(),
        ])?;
        Ok(())
    }

    fn tree(&self, vm: &VmName, disk: &DiskName) -> Result<Vec<SnapshotInfo>, VmError> {
        let value = self.run(&["tree", vm.as_str(), disk.as_str()])?;
        serde_json::from_value(value).map_err(|e| VmError {
            kind: ErrorKind::Backend,
            message: format!("unexpected tree output: {e}"),
            details: None,
        })
    }

    fn delete_disk(&self, vm: &VmName, disk: &DiskName) -> Result<(), VmError> {
        self.run(&["delete", vm.as_str(), disk.as_str(), "--force"])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_regex() {
        assert!(VmName::new("vm-1.test_2").is_ok());
        for bad in ["", ".hidden", "-x", "a/b", "a b", "a:b"] {
            let err = VmName::new(bad).unwrap_err();
            assert_eq!(err.kind, ErrorKind::InvalidConfig, "{bad:?}");
        }
    }

    #[test]
    fn error_kind_mapping() {
        assert_eq!(ErrorKind::from_code("not_found"), ErrorKind::NotFound);
        assert_eq!(
            ErrorKind::from_code("already_exists"),
            ErrorKind::AlreadyExists
        );
        assert_eq!(
            ErrorKind::from_code("invalid_config"),
            ErrorKind::InvalidConfig
        );
        assert_eq!(
            ErrorKind::from_code("invalid_state"),
            ErrorKind::InvalidState
        );
        assert_eq!(ErrorKind::from_code("anything_else"), ErrorKind::Backend);
    }

    #[test]
    fn attach_path_layout() {
        let store = SubprocessStore::new_unchecked("/tmp/vmforge-home");
        let path = store.attach_path(
            &VmName::new("vm1").unwrap(),
            &DiskName::new("root").unwrap(),
        );
        assert_eq!(
            path,
            PathBuf::from("/tmp/vmforge-home/vms/vm1/disks/root.qcow2")
        );
    }
}
