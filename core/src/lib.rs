//! VMForge core engine: define, boot, control and snapshot VMs by driving
//! QEMU as an out-of-process engine over QMP.
//!
//! Architecture: each VM lives in a state directory (`~/.vmforge/vms/<name>`
//! by default) containing its config (`vm.json`), QMP unix socket, pidfile
//! and serial log. QEMU runs with `-accel kvm` when `/dev/kvm` is available,
//! falling back to TCG otherwise (e.g. in CI).

pub mod config;
pub mod error;
pub mod qemu;
pub mod qmp;
pub mod snapshot;
pub mod vm;

pub use config::VmConfig;
pub use error::{Error, Result};
pub use vm::{Vm, VmStatus};
