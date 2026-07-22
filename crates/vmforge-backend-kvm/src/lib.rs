//! Linux/KVM backend stub.
//!
//! Phase 1: drives QEMU as a child process (`-accel kvm`) over QMP.
//! Phase 2: direct `/dev/kvm` ioctls via rust-vmm (kvm-ioctls/kvm-bindings).
//! See `docs/architecture.md` §2.

use vmforge_core::{
    Capabilities, GuestArch, HvError, Hypervisor, SnapshotId, VmConfig, VmHandle, VmState,
};

const BACKEND: &str = "kvm";

/// KVM-accelerated backend for Linux hosts.
#[derive(Debug, Default)]
pub struct KvmBackend;

impl KvmBackend {
    pub fn new() -> Self {
        Self
    }

    /// Whether `/dev/kvm` is present and usable on this host.
    pub fn is_available() -> bool {
        cfg!(target_os = "linux") && std::path::Path::new("/dev/kvm").exists()
    }

    fn todo(op: &'static str) -> HvError {
        HvError::NotImplemented {
            backend: BACKEND,
            op,
        }
    }
}

impl Hypervisor for KvmBackend {
    fn name(&self) -> &'static str {
        BACKEND
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            accelerator: "kvm",
            // KVM accelerates the host ISA; cross-ISA guests fall back to TCG.
            accelerated_archs: vec![if cfg!(target_arch = "aarch64") {
                GuestArch::Aarch64
            } else {
                GuestArch::X86_64
            }],
            live_snapshot: true,
            virtio_gpu_3d: true,
        }
    }

    fn create(&self, config: &VmConfig) -> Result<VmHandle, HvError> {
        let _ = config;
        Err(Self::todo("create"))
    }

    fn boot(&self, _vm: &VmHandle) -> Result<(), HvError> {
        Err(Self::todo("boot"))
    }

    fn pause(&self, _vm: &VmHandle) -> Result<(), HvError> {
        Err(Self::todo("pause"))
    }

    fn resume(&self, _vm: &VmHandle) -> Result<(), HvError> {
        Err(Self::todo("resume"))
    }

    fn stop(&self, _vm: &VmHandle) -> Result<(), HvError> {
        Err(Self::todo("stop"))
    }

    fn snapshot(&self, _vm: &VmHandle, _parent: Option<SnapshotId>) -> Result<SnapshotId, HvError> {
        Err(Self::todo("snapshot"))
    }

    fn restore(&self, _vm: &VmHandle, _snapshot: SnapshotId) -> Result<(), HvError> {
        Err(Self::todo("restore"))
    }

    fn delete(&self, _vm: VmHandle) -> Result<(), HvError> {
        Err(Self::todo("delete"))
    }

    fn state(&self, _vm: &VmHandle) -> Result<VmState, HvError> {
        Err(Self::todo("state"))
    }
}
