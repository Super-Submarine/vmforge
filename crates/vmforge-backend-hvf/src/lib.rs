//! macOS Hypervisor.framework (hvf) backend v0.
//!
//! Phase 1: drives QEMU (`qemu-system-aarch64 -machine virt -accel hvf
//! -cpu host` + edk2/AAVMF UEFI firmware + virtio devices) as a child
//! process over QMP, via the shared `vmforge-engine-qemu` engine. Only the
//! `-accel`/`-cpu` pair differs from the KVM backend; everything else —
//! machine type, firmware, virtio device set, QMP lifecycle — is common
//! code, so the invocation/QMP path is testable on Linux CI with
//! [`Accel::Tcg`].
//!
//! MVP scope (per the HVF port plan doc): Apple Silicon hosts, aarch64
//! Linux guests. Snapshots take a pause window (no userfaultfd /
//! `background-snapshot` on macOS). `restore` re-spawns QEMU with
//! `-incoming defer`, fresh qcow2 overlays on the snapshot's frozen disk
//! layers, and loads the saved RAM state — restoring the same snapshot
//! repeatedly (or snapshotting after a restore) branches the DAG, git
//! style. macOS packaging/signing/entitlement follow-ups are tracked in
//! `docs/macos-packaging-todo.md`, not implemented here.
//!
//! Phase 2: direct Hypervisor.framework / Virtualization.framework backend
//! behind the same trait. See `docs/architecture.md` §2.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use vmforge_core::{
    Capabilities, GuestArch, HvError, Hypervisor, SnapshotId, VmConfig, VmHandle, VmOp, VmState,
};
use vmforge_engine_qemu::{invocation, Accel, QemuVm};

const BACKEND: &str = "hvf";

/// On-disk artifacts backing one snapshot: the frozen qcow2 layers that
/// were active when it was taken (per disk, restore overlays stack on
/// these) and the saved RAM/device state file.
struct SnapshotArtifacts {
    disk_bases: Vec<PathBuf>,
    state_file: PathBuf,
    /// Lineage edge in the snapshot DAG; not consulted on restore yet.
    #[allow(dead_code)]
    parent: Option<SnapshotId>,
}

struct VmEntry {
    config: VmConfig,
    state: VmState,
    runtime: Option<QemuVm>,
    /// Per-VM directory for the QMP socket and snapshot artifacts.
    dir: PathBuf,
    snapshot_seq: u64,
    /// Currently-active writable qcow2 layer per disk (starts as the
    /// configured disk images; moves to fresh overlays on snapshot/restore).
    active_disks: Vec<PathBuf>,
    /// Snapshot DAG node -> artifacts. Branching = restoring any node and
    /// snapshotting again (multiple children per parent).
    snapshots: HashMap<SnapshotId, SnapshotArtifacts>,
}

/// Hypervisor.framework-accelerated backend for macOS hosts.
pub struct HvfBackend {
    accel: Accel,
    vms: Mutex<HashMap<String, VmEntry>>,
}

impl Default for HvfBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl HvfBackend {
    /// Backend using `-accel hvf` (real macOS hosts).
    pub fn new() -> Self {
        Self::with_accel(Accel::Hvf)
    }

    /// Backend using an explicit accelerator. [`Accel::Tcg`] runs the
    /// identical aarch64 `virt` invocation and QMP lifecycle under
    /// software emulation, so Linux CI can exercise this backend
    /// end-to-end without Hypervisor.framework.
    pub fn with_accel(accel: Accel) -> Self {
        Self {
            accel,
            vms: Mutex::new(HashMap::new()),
        }
    }

    /// Whether Hypervisor.framework is usable on this host.
    pub fn is_available() -> bool {
        #[cfg(target_os = "macos")]
        {
            // kern.hv_support reports Hypervisor.framework availability.
            std::process::Command::new("sysctl")
                .args(["-n", "kern.hv_support"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "1")
                .unwrap_or(false)
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    fn base_dir() -> PathBuf {
        std::env::var("VMFORGE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir().join("vmforge"))
    }

    fn transition(entry: &mut VmEntry, op: VmOp) -> Result<Option<VmState>, HvError> {
        let next = entry.state.transition(op)?;
        if let Some(state) = next {
            entry.state = state;
        }
        Ok(next)
    }

    /// Invocation for `entry` using its currently-active disk layers
    /// (which move to fresh overlays as snapshots/restores happen).
    fn invocation_for(&self, entry: &VmEntry) -> Result<invocation::Invocation, HvError> {
        let mut config = entry.config.clone();
        config.disks = entry
            .active_disks
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        invocation::build_aarch64_virt(&config, self.accel, &entry.dir.join("qmp.sock"))
    }

    /// Create a fresh writable qcow2 overlay at `overlay` backed by `base`.
    fn create_overlay(base: &PathBuf, overlay: &PathBuf) -> Result<(), HvError> {
        let out = std::process::Command::new("qemu-img")
            .args(["create", "-f", "qcow2", "-F", "qcow2", "-b"])
            .arg(base)
            .arg(overlay)
            .output()
            .map_err(|e| HvError::Engine(format!("failed to run qemu-img: {e}")))?;
        if !out.status.success() {
            return Err(HvError::Engine(format!(
                "qemu-img create overlay failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(())
    }
}

impl Hypervisor for HvfBackend {
    fn name(&self) -> &'static str {
        BACKEND
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            accelerator: self.accel.as_str(),
            // Apple Silicon accelerates aarch64 guests only (MVP); other
            // ISAs fall back to TCG "slow mode".
            accelerated_archs: match self.accel {
                Accel::Tcg => vec![],
                _ => vec![GuestArch::Aarch64],
            },
            // Snapshots work while running but take a pause window on hvf
            // (no background-snapshot / userfaultfd on macOS).
            live_snapshot: true,
            virtio_gpu_3d: false, // Venus on macOS pending MoltenVK validation
        }
    }

    fn create(&self, config: &VmConfig) -> Result<VmHandle, HvError> {
        let mut vms = self.vms.lock().unwrap();
        if vms.contains_key(&config.name) {
            return Err(HvError::Engine(format!(
                "VM '{}' already exists",
                config.name
            )));
        }
        let dir = Self::base_dir().join(&config.name);
        std::fs::create_dir_all(&dir).map_err(HvError::Io)?;
        // Validate the invocation up front so misconfiguration (e.g. a
        // non-aarch64 guest) fails at create, not boot.
        invocation::build_aarch64_virt(config, self.accel, &dir.join("qmp.sock"))?;
        let active_disks = config.disks.iter().map(PathBuf::from).collect();
        vms.insert(
            config.name.clone(),
            VmEntry {
                config: config.clone(),
                state: VmState::Defined,
                runtime: None,
                dir,
                snapshot_seq: 0,
                active_disks,
                snapshots: HashMap::new(),
            },
        );
        Ok(VmHandle::new(config.name.clone()))
    }

    fn boot(&self, vm: &VmHandle) -> Result<(), HvError> {
        let mut vms = self.vms.lock().unwrap();
        let entry = vms
            .get_mut(&vm.id)
            .ok_or_else(|| HvError::VmNotFound(vm.id.clone()))?;
        entry.state.transition(VmOp::Boot)?; // validate before side effects
        let inv = self.invocation_for(entry)?;
        let mut qemu = QemuVm::spawn(&inv)?;
        qemu.cont()?;
        entry.runtime = Some(qemu);
        Self::transition(entry, VmOp::Boot)?;
        Ok(())
    }

    fn pause(&self, vm: &VmHandle) -> Result<(), HvError> {
        let mut vms = self.vms.lock().unwrap();
        let entry = vms
            .get_mut(&vm.id)
            .ok_or_else(|| HvError::VmNotFound(vm.id.clone()))?;
        entry.state.transition(VmOp::Pause)?;
        entry
            .runtime
            .as_mut()
            .ok_or_else(|| HvError::Engine("VM has no running QEMU process".into()))?
            .pause()?;
        Self::transition(entry, VmOp::Pause)?;
        Ok(())
    }

    fn resume(&self, vm: &VmHandle) -> Result<(), HvError> {
        let mut vms = self.vms.lock().unwrap();
        let entry = vms
            .get_mut(&vm.id)
            .ok_or_else(|| HvError::VmNotFound(vm.id.clone()))?;
        entry.state.transition(VmOp::Resume)?;
        entry
            .runtime
            .as_mut()
            .ok_or_else(|| HvError::Engine("VM has no running QEMU process".into()))?
            .cont()?;
        Self::transition(entry, VmOp::Resume)?;
        Ok(())
    }

    fn stop(&self, vm: &VmHandle) -> Result<(), HvError> {
        let mut vms = self.vms.lock().unwrap();
        let entry = vms
            .get_mut(&vm.id)
            .ok_or_else(|| HvError::VmNotFound(vm.id.clone()))?;
        entry.state.transition(VmOp::Stop)?;
        if let Some(qemu) = entry.runtime.take() {
            qemu.quit()?;
        }
        Self::transition(entry, VmOp::Stop)?;
        Ok(())
    }

    fn snapshot(&self, vm: &VmHandle, parent: Option<SnapshotId>) -> Result<SnapshotId, HvError> {
        let mut vms = self.vms.lock().unwrap();
        let entry = vms
            .get_mut(&vm.id)
            .ok_or_else(|| HvError::VmNotFound(vm.id.clone()))?;
        if let Some(p) = &parent {
            if !entry.snapshots.contains_key(p) {
                return Err(HvError::SnapshotNotFound(p.0.clone()));
            }
        }
        let prior = entry.state;
        Self::transition(entry, VmOp::SnapshotBegin)?;
        entry.snapshot_seq += 1;
        let tag = format!("snap{}", entry.snapshot_seq);
        let disk_nodes: Vec<String> = (0..entry.config.disks.len())
            .map(|i| format!("disk{i}"))
            .collect();
        let dir = entry.dir.clone();
        // The layers active right now freeze as this snapshot's disk
        // state; QEMU switches writes onto the new `{tag}-disk{i}` overlays.
        let disk_bases = entry.active_disks.clone();
        let result = entry
            .runtime
            .as_mut()
            .ok_or_else(|| HvError::Engine("VM has no running QEMU process".into()))
            .and_then(|qemu| qemu.snapshot(&disk_nodes, &dir, &tag));
        Self::transition(entry, VmOp::SnapshotEnd(prior))?;
        let id = result?;
        entry.active_disks = disk_nodes
            .iter()
            .map(|node| dir.join(format!("{tag}-{node}.qcow2")))
            .collect();
        entry.snapshots.insert(
            id.clone(),
            SnapshotArtifacts {
                disk_bases,
                state_file: dir.join(format!("{tag}-state.bin")),
                parent,
            },
        );
        Ok(id)
    }

    fn restore(&self, vm: &VmHandle, snapshot: SnapshotId) -> Result<(), HvError> {
        let mut vms = self.vms.lock().unwrap();
        let entry = vms
            .get_mut(&vm.id)
            .ok_or_else(|| HvError::VmNotFound(vm.id.clone()))?;
        entry.state.transition(VmOp::Restore)?; // validate before side effects
        if !entry.snapshots.contains_key(&snapshot) {
            return Err(HvError::SnapshotNotFound(snapshot.0.clone()));
        }

        // Tear down any current QEMU (Paused case), then branch: fresh
        // writable overlays on the snapshot's frozen layers, so restoring
        // the same node repeatedly never mutates it (git-like branching).
        if let Some(qemu) = entry.runtime.take() {
            qemu.quit()?;
        }
        entry.snapshot_seq += 1;
        let branch = format!("branch{}", entry.snapshot_seq);
        let artifacts = &entry.snapshots[&snapshot];
        let state_file = artifacts.state_file.clone();
        let mut overlays = Vec::with_capacity(artifacts.disk_bases.len());
        for (i, base) in artifacts.disk_bases.iter().enumerate() {
            let overlay = entry.dir.join(format!("{branch}-disk{i}.qcow2"));
            Self::create_overlay(base, &overlay)?;
            overlays.push(overlay);
        }
        entry.active_disks = overlays;

        // Boot directly into the saved RAM/device state (instant resume).
        let inv = self.invocation_for(entry)?.with_incoming_defer();
        let mut qemu = QemuVm::spawn(&inv)?;
        qemu.restore_incoming(&state_file)?;
        qemu.cont()?;
        entry.runtime = Some(qemu);
        Self::transition(entry, VmOp::Restore)?;
        Ok(())
    }

    fn delete(&self, vm: VmHandle) -> Result<(), HvError> {
        let mut vms = self.vms.lock().unwrap();
        let entry = vms
            .get_mut(&vm.id)
            .ok_or_else(|| HvError::VmNotFound(vm.id.clone()))?;
        entry.state.transition(VmOp::Delete)?;
        vms.remove(&vm.id);
        Ok(())
    }

    fn state(&self, vm: &VmHandle) -> Result<VmState, HvError> {
        let vms = self.vms.lock().unwrap();
        vms.get(&vm.id)
            .map(|e| e.state)
            .ok_or_else(|| HvError::VmNotFound(vm.id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_state_validation_without_qemu() {
        let backend = HvfBackend::with_accel(Accel::Tcg);
        let config = VmConfig {
            name: format!("unit-{}", std::process::id()),
            arch: GuestArch::Aarch64,
            vcpus: 1,
            memory_mib: 128,
            disks: vec![],
            gpu_3d: false,
        };
        let vm = backend.create(&config).unwrap();
        assert_eq!(backend.state(&vm).unwrap(), VmState::Defined);
        // Pause from Defined is an invalid transition, caught before any
        // QEMU interaction.
        assert!(backend.pause(&vm).is_err());
        backend.delete(vm).unwrap();
    }

    #[test]
    fn x86_guest_rejected_at_create() {
        let backend = HvfBackend::with_accel(Accel::Tcg);
        let config = VmConfig {
            name: format!("unit-x86-{}", std::process::id()),
            arch: GuestArch::X86_64,
            vcpus: 1,
            memory_mib: 128,
            disks: vec![],
            gpu_3d: false,
        };
        assert!(backend.create(&config).is_err());
    }
}
