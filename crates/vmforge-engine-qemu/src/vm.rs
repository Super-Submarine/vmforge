//! A running QEMU child process plus its QMP control connection.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::json;
use vmforge_core::{HvError, SnapshotId};

use crate::invocation::Invocation;
use crate::qmp::QmpClient;

/// A live QEMU instance. Spawned paused (`-S`); the caller drives the
/// lifecycle over QMP: [`cont`](Self::cont), [`pause`](Self::pause),
/// [`snapshot`](Self::snapshot), [`quit`](Self::quit).
pub struct QemuVm {
    child: Child,
    qmp: QmpClient,
    qmp_socket: PathBuf,
}

impl QemuVm {
    /// Spawn QEMU per `invocation` and complete the QMP handshake.
    /// The guest is left paused; call [`cont`](Self::cont) to start vCPUs.
    pub fn spawn(invocation: &Invocation) -> Result<Self, HvError> {
        let mut child = Command::new(&invocation.binary)
            .args(&invocation.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| HvError::Engine(format!("failed to spawn {}: {e}", invocation.binary)))?;
        match QmpClient::connect(&invocation.qmp_socket, Duration::from_secs(10)) {
            Ok(qmp) => Ok(Self {
                child,
                qmp,
                qmp_socket: invocation.qmp_socket.clone(),
            }),
            Err(e) => {
                let _ = child.kill();
                let stderr = child
                    .stderr
                    .take()
                    .and_then(|mut s| {
                        let mut buf = String::new();
                        std::io::Read::read_to_string(&mut s, &mut buf).ok()?;
                        Some(buf)
                    })
                    .unwrap_or_default();
                let _ = child.wait();
                Err(HvError::Engine(format!(
                    "QEMU failed to come up: {e}; stderr: {}",
                    stderr.trim()
                )))
            }
        }
    }

    /// `query-status` run state string, e.g. "running", "paused", "prelaunch".
    pub fn status(&mut self) -> Result<String, HvError> {
        let ret = self.qmp.execute("query-status", None)?;
        Ok(ret["status"].as_str().unwrap_or_default().to_string())
    }

    /// Accelerator actually in use, from `query-kvm`-independent
    /// `query-accel` fallback: uses `query-kvm` (enabled == kvm) since
    /// QMP has no direct accelerator query; hvf/tcg report enabled=false.
    pub fn kvm_enabled(&mut self) -> Result<bool, HvError> {
        let ret = self.qmp.execute("query-kvm", None)?;
        Ok(ret["enabled"].as_bool().unwrap_or(false))
    }

    /// Start (or resume) vCPU execution.
    pub fn cont(&mut self) -> Result<(), HvError> {
        self.qmp.execute("cont", None).map(|_| ())
    }

    /// Halt vCPUs, retaining RAM and device state.
    pub fn pause(&mut self) -> Result<(), HvError> {
        self.qmp.execute("stop", None).map(|_| ())
    }

    /// Capture a snapshot: pause -> external disk overlay per block node
    /// (`blockdev-snapshot-sync`) -> RAM/device state via `migrate` to a
    /// file -> resume if the VM was running.
    ///
    /// This is the accelerator-portable path from the HVF port plan §2:
    /// hvf has no userfaultfd/`background-snapshot`, so snapshots take a
    /// pause window (precopy with vCPUs stopped) on macOS.
    ///
    /// Returns a content-addressed [`SnapshotId`] (hash of the saved RAM
    /// state and overlay paths).
    pub fn snapshot(
        &mut self,
        disk_nodes: &[String],
        state_dir: &std::path::Path,
        tag: &str,
    ) -> Result<SnapshotId, HvError> {
        let was_running = self.status()? == "running";
        if was_running {
            self.pause()?;
        }

        let mut overlays = Vec::new();
        for node in disk_nodes {
            let overlay = state_dir.join(format!("{tag}-{node}.qcow2"));
            self.qmp.execute(
                "blockdev-snapshot-sync",
                Some(json!({
                    "node-name": node,
                    "snapshot-file": overlay.to_string_lossy(),
                    "snapshot-node-name": format!("{tag}-{node}"),
                    "format": "qcow2",
                })),
            )?;
            overlays.push(overlay);
        }

        let state_file = state_dir.join(format!("{tag}-state.bin"));
        self.qmp.execute(
            "migrate",
            Some(json!({
                "uri": format!("exec:cat > {}", state_file.to_string_lossy()),
            })),
        )?;
        self.wait_migration(Duration::from_secs(120))?;

        if was_running {
            self.cont()?;
        }

        let mut hasher = DefaultHasher::new();
        std::fs::read(&state_file)
            .map_err(HvError::Io)?
            .hash(&mut hasher);
        for overlay in &overlays {
            overlay.hash(&mut hasher);
        }
        Ok(SnapshotId(format!("{:016x}", hasher.finish())))
    }

    /// Load saved RAM/device state on a VM spawned with `-incoming defer`
    /// (see [`Invocation::with_incoming_defer`](crate::Invocation::with_incoming_defer)):
    /// issues QMP `migrate-incoming` from `state_file` and waits until the
    /// run state leaves `inmigrate`. The guest is left paused (`-S`); call
    /// [`cont`](Self::cont) to start vCPUs — this is the instant-resume
    /// primitive behind `restore`.
    pub fn restore_incoming(&mut self, state_file: &std::path::Path) -> Result<(), HvError> {
        self.qmp.execute(
            "migrate-incoming",
            Some(json!({
                "uri": format!("exec:cat {}", state_file.to_string_lossy()),
            })),
        )?;
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            match self.status()?.as_str() {
                "inmigrate" if Instant::now() > deadline => {
                    return Err(HvError::Engine("incoming state load timed out".into()))
                }
                "inmigrate" => std::thread::sleep(Duration::from_millis(100)),
                "internal-error" | "io-error" => {
                    return Err(HvError::Engine("incoming state load failed".into()))
                }
                _ => return Ok(()),
            }
        }
    }

    fn wait_migration(&mut self, timeout: Duration) -> Result<(), HvError> {
        let deadline = Instant::now() + timeout;
        loop {
            let ret = self.qmp.execute("query-migrate", None)?;
            match ret["status"].as_str() {
                Some("completed") => return Ok(()),
                Some("failed") | Some("cancelled") => {
                    return Err(HvError::Engine(format!("state migration failed: {ret}")))
                }
                _ if Instant::now() > deadline => {
                    return Err(HvError::Engine("state migration timed out".into()))
                }
                _ => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    }

    /// Terminate QEMU gracefully via QMP `quit`.
    pub fn quit(mut self) -> Result<(), HvError> {
        let _ = self.qmp.execute("quit", None);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.child.try_wait().map_err(HvError::Io)? {
                Some(_) => break,
                None if Instant::now() > deadline => {
                    self.child.kill().map_err(HvError::Io)?;
                    self.child.wait().map_err(HvError::Io)?;
                    break;
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
        let _ = std::fs::remove_file(&self.qmp_socket);
        Ok(())
    }
}

impl Drop for QemuVm {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = std::fs::remove_file(&self.qmp_socket);
    }
}
