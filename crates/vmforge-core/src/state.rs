use crate::HvError;

/// VM lifecycle states. See `docs/architecture.md` §4 for the diagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmState {
    /// Configuration exists; no runtime resources allocated.
    Defined,
    /// vCPUs executing guest code.
    Running,
    /// vCPUs halted; RAM and device state retained.
    Paused,
    /// Guest shut down or was stopped; disk state retained.
    Stopped,
    /// Transient state while a snapshot is being captured.
    Snapshotting,
}

/// Lifecycle operations that drive state transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmOp {
    Boot,
    Pause,
    Resume,
    Stop,
    SnapshotBegin,
    /// End of snapshot capture; returns to the given prior state.
    SnapshotEnd(VmState),
    Restore,
    Delete,
}

impl VmState {
    /// Validate and apply a lifecycle transition.
    ///
    /// Returns the next state, or `HvError::InvalidTransition` if `op` is not
    /// permitted from `self`. `Delete` has no successor state and is modeled
    /// as an error-free `None`.
    pub fn transition(self, op: VmOp) -> Result<Option<VmState>, HvError> {
        use VmOp::*;
        use VmState::*;
        let next = match (self, op) {
            (Defined | Stopped, Boot) => Some(Running),
            (Running, Pause) => Some(Paused),
            (Paused, Resume) => Some(Running),
            (Running | Paused, Stop) => Some(Stopped),
            (Running | Paused, SnapshotBegin) => Some(Snapshotting),
            (Snapshotting, SnapshotEnd(prior @ (Running | Paused))) => Some(prior),
            // Restore boots straight into Running from saved RAM state.
            (Defined | Stopped | Paused, Restore) => Some(Running),
            (Defined | Stopped, Delete) => None,
            (from, op) => {
                return Err(HvError::InvalidTransition {
                    from,
                    op: format!("{op:?}"),
                })
            }
        };
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_lifecycle() {
        let s = VmState::Defined.transition(VmOp::Boot).unwrap().unwrap();
        assert_eq!(s, VmState::Running);
        let s = s.transition(VmOp::Pause).unwrap().unwrap();
        assert_eq!(s, VmState::Paused);
        let s = s.transition(VmOp::Resume).unwrap().unwrap();
        assert_eq!(s, VmState::Running);
        let s = s.transition(VmOp::Stop).unwrap().unwrap();
        assert_eq!(s, VmState::Stopped);
        assert!(s.transition(VmOp::Delete).unwrap().is_none());
    }

    #[test]
    fn live_snapshot_returns_to_prior_state() {
        let s = VmState::Running
            .transition(VmOp::SnapshotBegin)
            .unwrap()
            .unwrap();
        assert_eq!(s, VmState::Snapshotting);
        let s = s
            .transition(VmOp::SnapshotEnd(VmState::Running))
            .unwrap()
            .unwrap();
        assert_eq!(s, VmState::Running);
    }

    #[test]
    fn restore_is_instant_resume() {
        let s = VmState::Stopped.transition(VmOp::Restore).unwrap().unwrap();
        assert_eq!(s, VmState::Running);
    }

    #[test]
    fn invalid_transitions_rejected() {
        assert!(VmState::Running.transition(VmOp::Boot).is_err());
        assert!(VmState::Running.transition(VmOp::Delete).is_err());
        assert!(VmState::Defined.transition(VmOp::Pause).is_err());
    }
}
