use std::collections::HashMap;

use crate::HvError;

/// Content-addressed snapshot identifier (hash of disk+RAM+device state).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotId(pub String);

/// Metadata for one node in the snapshot DAG.
#[derive(Debug, Clone)]
pub struct SnapshotMeta {
    pub id: SnapshotId,
    /// Parent snapshot; `None` for a root. Branching = multiple children.
    pub parent: Option<SnapshotId>,
    pub label: Option<String>,
    pub created_unix: u64,
}

/// Git-like snapshot DAG: snapshots are immutable nodes with parent
/// pointers; branching a VM is creating a new child of any node.
///
/// This in-memory implementation backs the scaffold and tests; the
/// production store persists metadata alongside qcow2 external snapshots.
#[derive(Debug, Default)]
pub struct SnapshotStore {
    nodes: HashMap<SnapshotId, SnapshotMeta>,
}

impl SnapshotStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a snapshot node, validating that the parent exists.
    pub fn insert(&mut self, meta: SnapshotMeta) -> Result<(), HvError> {
        if let Some(parent) = &meta.parent {
            if !self.nodes.contains_key(parent) {
                return Err(HvError::SnapshotNotFound(parent.0.clone()));
            }
        }
        self.nodes.insert(meta.id.clone(), meta);
        Ok(())
    }

    pub fn get(&self, id: &SnapshotId) -> Option<&SnapshotMeta> {
        self.nodes.get(id)
    }

    /// Children of `id` — the branches rooted at that snapshot.
    pub fn children(&self, id: &SnapshotId) -> Vec<&SnapshotMeta> {
        self.nodes
            .values()
            .filter(|m| m.parent.as_ref() == Some(id))
            .collect()
    }

    /// Ancestry from `id` back to its root (inclusive), newest first.
    pub fn lineage(&self, id: &SnapshotId) -> Result<Vec<&SnapshotMeta>, HvError> {
        let mut out = Vec::new();
        let mut cur = Some(id.clone());
        while let Some(cid) = cur {
            let node = self
                .nodes
                .get(&cid)
                .ok_or_else(|| HvError::SnapshotNotFound(cid.0.clone()))?;
            cur = node.parent.clone();
            out.push(node);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(id: &str, parent: Option<&str>) -> SnapshotMeta {
        SnapshotMeta {
            id: SnapshotId(id.into()),
            parent: parent.map(|p| SnapshotId(p.into())),
            label: None,
            created_unix: 0,
        }
    }

    #[test]
    fn branching_dag() {
        let mut store = SnapshotStore::new();
        store.insert(meta("root", None)).unwrap();
        store.insert(meta("a", Some("root"))).unwrap();
        store.insert(meta("b", Some("root"))).unwrap();
        assert_eq!(store.children(&SnapshotId("root".into())).len(), 2);
        let lineage = store.lineage(&SnapshotId("a".into())).unwrap();
        assert_eq!(lineage.len(), 2);
    }

    #[test]
    fn missing_parent_rejected() {
        let mut store = SnapshotStore::new();
        assert!(store.insert(meta("x", Some("ghost"))).is_err());
    }
}
