//! macOS Hypervisor.framework (hvf) backend stub.
//!
//! Phase 1: drives QEMU as a child process (`-accel hvf`) over QMP.
//! Phase 2: direct Hypervisor.framework / Virtualization.framework backend
//! (requires the com.apple.security.hypervisor entitlement).
//! See `docs/architecture.md` §2.

use vmforge_core::{
    Capabilities, GuestArch, HvError, Hypervisor, SnapshotId, VmConfig, VmHandle, VmState,
};

const BACKEND: &str = "hvf";

/// Hypervisor.framework-accelerated backend for macOS hosts.
#[derive(Debug, Default)]
pub struct HvfBackend;

impl HvfBackend {
    pub fn new() -> Self {
        Self
    }

    /// Whether Hypervisor.framework is usable on this host
    /// (macOS only; checked via `kern.hv_support` at runtime in Phase 1).
    pub fn is_available() -> bool {
        cfg!(target_os = "macos")
    }

    fn todo(op: &'static str) -> HvError {
        HvError::NotImplemented {
            backend: BACKEND,
            op,
        }
    }
}

impl Hypervisor for HvfBackend {
    fn name(&self) -> &'static str {
        BACKEND
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            accelerator: "hvf",
            // Apple silicon accelerates ARM64 guests only; Intel Macs x86-64.
            accelerated_archs: vec![if cfg!(target_arch = "aarch64") {
                GuestArch::Aarch64
            } else {
                GuestArch::X86_64
            }],
            live_snapshot: true,
            virtio_gpu_3d: false, // Venus on macOS pending MoltenVK validation
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
