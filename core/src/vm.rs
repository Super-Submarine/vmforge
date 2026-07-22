//! VM lifecycle management: create/start/stop/status/snapshot, keyed by a
//! per-VM state directory.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config::VmConfig;
use crate::error::{Error, Result};
use crate::qemu;
use crate::qmp::QmpClient;
use crate::snapshot::{self, SnapshotInfo};

const QMP_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmStatus {
    /// QEMU process alive; contains the QMP run state (e.g. "running").
    Running,
    Stopped,
}

/// Handle to a VM: its config plus state directory layout.
pub struct Vm {
    pub config: VmConfig,
    state_dir: PathBuf,
}

impl Vm {
    /// Default root for VM state: `$VMFORGE_HOME` or `~/.vmforge/vms`.
    pub fn default_root() -> PathBuf {
        if let Ok(home) = std::env::var("VMFORGE_HOME") {
            return PathBuf::from(home).join("vms");
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".vmforge").join("vms")
    }

    /// Create a new VM: writes vm.json and (optionally) creates the qcow2
    /// disk when `disk_size` is given and the disk does not already exist.
    pub fn create(root: &Path, config: VmConfig, disk_size: Option<&str>) -> Result<Vm> {
        config.validate()?;
        let state_dir = root.join(&config.name);
        if state_dir.join("vm.json").exists() {
            return Err(Error::VmExists(config.name.clone()));
        }
        std::fs::create_dir_all(&state_dir)?;
        if let Some(size) = disk_size {
            if !config.disk.exists() {
                if let Some(parent) = config.disk.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                snapshot::create_disk(&config.disk, size)?;
            }
        }
        config.save(&state_dir.join("vm.json"))?;
        Ok(Vm { config, state_dir })
    }

    /// Open an existing VM by name.
    pub fn open(root: &Path, name: &str) -> Result<Vm> {
        let state_dir = root.join(name);
        let cfg_path = state_dir.join("vm.json");
        if !cfg_path.exists() {
            return Err(Error::VmNotFound(name.to_string()));
        }
        let config = VmConfig::load(&cfg_path)?;
        Ok(Vm { config, state_dir })
    }

    /// List all VM names under the root.
    pub fn list(root: &Path) -> Result<Vec<String>> {
        let mut names = vec![];
        if root.exists() {
            for entry in std::fs::read_dir(root)? {
                let entry = entry?;
                if entry.path().join("vm.json").exists() {
                    names.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }
    pub fn qmp_socket(&self) -> PathBuf {
        self.state_dir.join("qmp.sock")
    }
    pub fn pidfile(&self) -> PathBuf {
        self.state_dir.join("qemu.pid")
    }
    pub fn serial_log(&self) -> PathBuf {
        self.state_dir.join("serial.log")
    }

    /// Boot the VM. Returns the accelerator used ("kvm" or "tcg").
    pub fn start(&self) -> Result<&'static str> {
        if self.status()? == VmStatus::Running {
            return Err(Error::AlreadyRunning(self.config.name.clone()));
        }
        let _ = std::fs::remove_file(self.qmp_socket());
        let accel = qemu::choose_accel();
        let cmd = qemu::build_command(
            &self.config,
            &self.qmp_socket(),
            &self.pidfile(),
            &self.serial_log(),
            accel,
        );
        qemu::launch(cmd)?;
        // Wait for the QMP socket to accept a handshake.
        let deadline = Instant::now() + QMP_CONNECT_TIMEOUT;
        loop {
            match QmpClient::connect(&self.qmp_socket()) {
                Ok(_) => return Ok(accel),
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(200))
                }
                Err(_) => return Err(Error::QmpTimeout(self.qmp_socket())),
            }
        }
    }

    /// Connect to the running VM's QMP socket.
    pub fn qmp(&self) -> Result<QmpClient> {
        if self.status()? != VmStatus::Running {
            return Err(Error::NotRunning(self.config.name.clone()));
        }
        QmpClient::connect(&self.qmp_socket())
    }

    /// Process-level status via the pidfile (kill -0).
    pub fn status(&self) -> Result<VmStatus> {
        match std::fs::read_to_string(self.pidfile()) {
            Ok(pid_str) => {
                let pid: i32 = pid_str.trim().parse().unwrap_or(0);
                if pid > 0 && unsafe { libc_kill(pid, 0) } == 0 {
                    Ok(VmStatus::Running)
                } else {
                    Ok(VmStatus::Stopped)
                }
            }
            Err(_) => Ok(VmStatus::Stopped),
        }
    }

    /// QMP run state string ("running", "paused", ...) when up.
    pub fn run_state(&self) -> Result<Option<String>> {
        if self.status()? != VmStatus::Running {
            return Ok(None);
        }
        Ok(Some(self.qmp()?.query_status()?))
    }

    /// Stop the VM: graceful ACPI powerdown, then hard quit after `grace`.
    pub fn stop(&self, grace: Duration) -> Result<()> {
        let mut qmp = self.qmp()?;
        qmp.system_powerdown()?;
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if self.status()? == VmStatus::Stopped {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        // Guest ignored ACPI (no acpid, e.g. minimal live ISO): hard quit.
        if let Ok(mut qmp) = self.qmp() {
            qmp.quit()?;
        }
        let _ = std::fs::remove_file(self.pidfile());
        Ok(())
    }

    // --- snapshots: route to live (QMP) or offline (qemu-img) ---

    pub fn snapshot_create(&self, tag: &str) -> Result<&'static str> {
        if self.status()? == VmStatus::Running {
            snapshot::save_live(&mut self.qmp()?, tag)?;
            Ok("live")
        } else {
            snapshot::save_offline(&self.config.disk, tag)?;
            Ok("offline")
        }
    }

    pub fn snapshot_restore(&self, tag: &str) -> Result<&'static str> {
        if self.status()? == VmStatus::Running {
            snapshot::restore_live(&mut self.qmp()?, tag)?;
            Ok("live")
        } else {
            snapshot::restore_offline(&self.config.disk, tag)?;
            Ok("offline")
        }
    }

    pub fn snapshot_delete(&self, tag: &str) -> Result<()> {
        if self.status()? == VmStatus::Running {
            snapshot::delete_live(&mut self.qmp()?, tag)
        } else {
            snapshot::delete_offline(&self.config.disk, tag)
        }
    }

    pub fn snapshot_list(&self) -> Result<Vec<SnapshotInfo>> {
        if self.status()? == VmStatus::Running {
            snapshot::list_live(&mut self.qmp()?)
        } else {
            snapshot::list_offline(&self.config.disk)
        }
    }
}

extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(name: &str, disk: PathBuf) -> VmConfig {
        VmConfig {
            name: name.into(),
            cpus: 1,
            memory_mib: 256,
            disk,
            iso: None,
            extra_args: vec![],
        }
    }

    #[test]
    fn create_open_list() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let disk = root.join("a.qcow2");
        std::fs::write(&disk, b"").unwrap(); // fake disk, no size given
        Vm::create(root, cfg("alpine", disk.clone()), None).unwrap();
        assert!(matches!(
            Vm::create(root, cfg("alpine", disk), None),
            Err(Error::VmExists(_))
        ));
        let vm = Vm::open(root, "alpine").unwrap();
        assert_eq!(vm.config.cpus, 1);
        assert_eq!(vm.status().unwrap(), VmStatus::Stopped);
        assert_eq!(Vm::list(root).unwrap(), vec!["alpine".to_string()]);
        assert!(matches!(
            Vm::open(root, "missing"),
            Err(Error::VmNotFound(_))
        ));
    }
}
