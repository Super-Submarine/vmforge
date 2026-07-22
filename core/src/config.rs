use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Declarative definition of a VM. Serialized as `vm.json` in the VM's
/// state directory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VmConfig {
    /// Unique VM name ([a-zA-Z0-9._-]).
    pub name: String,
    /// Number of virtual CPUs.
    pub cpus: u32,
    /// Memory size in MiB.
    pub memory_mib: u32,
    /// Path to the qcow2 boot disk.
    pub disk: PathBuf,
    /// Optional ISO attached as a CD-ROM (boot media / installer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iso: Option<PathBuf>,
    /// Extra raw QEMU arguments appended verbatim (escape hatch).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_args: Vec<String>,
}

impl VmConfig {
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty()
            || !self
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return Err(Error::InvalidName(self.name.clone()));
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let data = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&data)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(name: &str) -> VmConfig {
        VmConfig {
            name: name.into(),
            cpus: 2,
            memory_mib: 1024,
            disk: PathBuf::from("/tmp/disk.qcow2"),
            iso: None,
            extra_args: vec![],
        }
    }

    #[test]
    fn valid_names() {
        for n in ["alpine", "vm-1", "test.vm_2"] {
            assert!(cfg(n).validate().is_ok(), "{n}");
        }
    }

    #[test]
    fn invalid_names() {
        for n in ["", "bad name", "a/b", "x;y"] {
            assert!(cfg(n).validate().is_err(), "{n:?}");
        }
    }

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("vm.json");
        let c = cfg("alpine");
        c.save(&p).unwrap();
        assert_eq!(VmConfig::load(&p).unwrap(), c);
    }
}
