//! vmforge-core: hypervisor abstraction layer.
//!
//! Defines the [`Hypervisor`] trait that every backend (KVM on Linux,
//! Hypervisor.framework on macOS) implements, plus the VM lifecycle state
//! machine and snapshot model shared by all backends.
//!
//! See `docs/architecture.md` for the full design.

pub mod error;
pub mod snapshot;
pub mod state;
pub mod vm;

pub use error::HvError;
pub use snapshot::{SnapshotId, SnapshotStore};
pub use state::VmState;
pub use vm::{VmConfig, VmHandle};

/// Guest instruction-set architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestArch {
    X86_64,
    Aarch64,
}

/// Capabilities advertised by a backend so callers can adapt
/// (e.g. warn when a guest ISA will fall back to slow emulation).
#[derive(Debug, Clone)]
pub struct Capabilities {
    /// Human-readable accelerator name, e.g. "kvm" or "hvf".
    pub accelerator: &'static str,
    /// Guest ISAs that run with hardware acceleration on this host.
    pub accelerated_archs: Vec<GuestArch>,
    /// Whether snapshots can be taken while the VM is running.
    pub live_snapshot: bool,
    /// Whether paravirtual 3D (virtio-gpu + virgl/Venus) is available.
    pub virtio_gpu_3d: bool,
}

/// The hypervisor abstraction implemented by each platform backend.
///
/// Lifecycle transitions are validated by [`VmState::transition`]; backends
/// must go through it rather than mutating state directly.
pub trait Hypervisor: Send + Sync {
    /// Backend identifier, e.g. "kvm" or "hvf".
    fn name(&self) -> &'static str;

    /// What this backend can do on the current host.
    fn capabilities(&self) -> Capabilities;

    /// Define a new VM from `config`. Transitions: (none) -> Defined.
    fn create(&self, config: &VmConfig) -> Result<VmHandle, HvError>;

    /// Boot a defined or stopped VM. Transitions: Defined|Stopped -> Running.
    fn boot(&self, vm: &VmHandle) -> Result<(), HvError>;

    /// Pause a running VM. Transitions: Running -> Paused.
    fn pause(&self, vm: &VmHandle) -> Result<(), HvError>;

    /// Resume a paused VM. Transitions: Paused -> Running.
    fn resume(&self, vm: &VmHandle) -> Result<(), HvError>;

    /// Stop a running or paused VM. Transitions: Running|Paused -> Stopped.
    fn stop(&self, vm: &VmHandle) -> Result<(), HvError>;

    /// Capture disk + RAM + device state as a snapshot. Valid while Running
    /// (live) or Paused (quiesced); the VM returns to its prior state.
    /// `parent` places the snapshot in the DAG (None = new root).
    fn snapshot(&self, vm: &VmHandle, parent: Option<SnapshotId>) -> Result<SnapshotId, HvError>;

    /// Restore a snapshot, booting directly into Running from saved RAM
    /// state (the instant-resume primitive).
    fn restore(&self, vm: &VmHandle, snapshot: SnapshotId) -> Result<(), HvError>;

    /// Delete a VM definition. Transitions: Defined|Stopped -> (none).
    fn delete(&self, vm: VmHandle) -> Result<(), HvError>;

    /// Current lifecycle state of the VM.
    fn state(&self, vm: &VmHandle) -> Result<VmState, HvError>;
}
