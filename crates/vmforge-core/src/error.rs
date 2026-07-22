use crate::VmState;

/// Errors surfaced by hypervisor backends and the lifecycle state machine.
#[derive(Debug, thiserror::Error)]
pub enum HvError {
    #[error("invalid lifecycle transition: {op} from {from:?}")]
    InvalidTransition { from: VmState, op: String },

    #[error("VM not found: {0}")]
    VmNotFound(String),

    #[error("snapshot not found: {0}")]
    SnapshotNotFound(String),

    #[error("backend '{backend}' not supported on this host: {reason}")]
    Unsupported {
        backend: &'static str,
        reason: String,
    },

    #[error("backend '{backend}' operation '{op}' is not implemented yet")]
    NotImplemented {
        backend: &'static str,
        op: &'static str,
    },

    #[error("engine error: {0}")]
    Engine(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
