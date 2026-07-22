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
//! `background-snapshot` on macOS); `restore` is deferred to the M3
//! snapshot-fidelity spike. macOS packaging/signing/entitlement follow-ups
//! are tracked in `docs/macos-packaging-todo.md`, not implemented here.
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

struct VmEntry {
    config: VmConfig,
    state: VmState,
    runtime: Option<QemuVm>,
    /// Per-VM directory for the QMP socket and snapshot artifacts.
    dir: PathBuf,
    snapshot_seq: u64,
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
        vms.insert(
            config.name.clone(),
            VmEntry {
                config: config.clone(),
                state: VmState::Defined,
                runtime: None,
                dir,
                snapshot_seq: 0,
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
        let inv =
            invocation::build_aarch64_virt(&entry.config, self.accel, &entry.dir.join("qmp.sock"))?;
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
        // v0: the DAG placement (`parent`) is recorded by the caller's
        // SnapshotStore; the engine captures disk overlays + RAM state.
        let _ = parent;
        let mut vms = self.vms.lock().unwrap();
        let entry = vms
            .get_mut(&vm.id)
            .ok_or_else(|| HvError::VmNotFound(vm.id.clone()))?;
        let prior = entry.state;
        Self::transition(entry, VmOp::SnapshotBegin)?;
        entry.snapshot_seq += 1;
        let tag = format!("snap{}", entry.snapshot_seq);
        let disk_nodes: Vec<String> = (0..entry.config.disks.len())
            .map(|i| format!("disk{i}"))
            .collect();
        let dir = entry.dir.clone();
        let result = entry
            .runtime
            .as_mut()
            .ok_or_else(|| HvError::Engine("VM has no running QEMU process".into()))
            .and_then(|qemu| qemu.snapshot(&disk_nodes, &dir, &tag));
        Self::transition(entry, VmOp::SnapshotEnd(prior))?;
        result
    }

    fn restore(&self, _vm: &VmHandle, _snapshot: SnapshotId) -> Result<(), HvError> {
        // Deferred to the M3 HVF snapshot-fidelity spike (port plan §5):
        // restore-from-RAM-state needs `-incoming` plumbing plus vtimer
        // validation on hvf before we expose it.
        Err(HvError::NotImplemented {
            backend: BACKEND,
            op: "restore",
        })
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
