use crate::GuestArch;

/// Static configuration of a VM (the "definition").
#[derive(Debug, Clone)]
pub struct VmConfig {
    /// Human-readable unique name.
    pub name: String,
    /// Guest instruction-set architecture.
    pub arch: GuestArch,
    pub vcpus: u32,
    pub memory_mib: u64,
    /// Paths to disk images (qcow2 in the QEMU-engine phase).
    pub disks: Vec<String>,
    /// Enable paravirtual 3D (virtio-gpu + virgl/Venus) when available.
    pub gpu_3d: bool,
}

/// Opaque handle to a defined VM, issued by a backend's `create`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VmHandle {
    pub id: String,
}

impl VmHandle {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}
