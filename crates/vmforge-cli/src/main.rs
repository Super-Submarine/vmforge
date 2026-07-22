//! `vmforge` CLI: thin driver over the Hypervisor trait.
//!
//! Scaffold supports `vmforge info` (show selected backend + capabilities).
//! Lifecycle subcommands (create/boot/snapshot/...) land with the Phase 1
//! QEMU engine.

use vmforge_backend_hvf::HvfBackend;
use vmforge_backend_kvm::KvmBackend;
use vmforge_core::Hypervisor;

/// Pick the native backend for the current host, if any.
fn select_backend() -> Option<Box<dyn Hypervisor>> {
    if KvmBackend::is_available() {
        Some(Box::new(KvmBackend::new()))
    } else if HvfBackend::is_available() {
        Some(Box::new(HvfBackend::new()))
    } else {
        None
    }
}

fn main() {
    let cmd = std::env::args().nth(1);
    match cmd.as_deref() {
        Some("info") | None => match select_backend() {
            Some(hv) => {
                let caps = hv.capabilities();
                println!("backend: {}", hv.name());
                println!("accelerator: {}", caps.accelerator);
                println!("accelerated guest archs: {:?}", caps.accelerated_archs);
                println!("live snapshot: {}", caps.live_snapshot);
                println!("virtio-gpu 3D: {}", caps.virtio_gpu_3d);
            }
            None => {
                eprintln!("no hardware-accelerated backend available on this host");
                std::process::exit(1);
            }
        },
        Some(other) => {
            eprintln!("unknown command: {other} (scaffold supports: info)");
            std::process::exit(2);
        }
    }
}
