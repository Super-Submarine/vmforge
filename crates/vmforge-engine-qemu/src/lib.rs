//! vmforge-engine-qemu: Phase 1 QEMU process engine shared by backends.
//!
//! Drives QEMU as a child process over QMP. Accelerator-specific backends
//! (`vmforge-backend-kvm`, `vmforge-backend-hvf`) build an [`Invocation`]
//! for their accelerator and reuse the same [`QemuVm`] lifecycle:
//! spawn (paused) -> QMP handshake -> cont / stop / snapshot / quit.
//!
//! See `docs/architecture.md` §2 and the HVF port plan doc.

pub mod invocation;
pub mod qmp;
pub mod vm;

pub use invocation::{Accel, Invocation};
pub use qmp::QmpClient;
pub use vm::QemuVm;
