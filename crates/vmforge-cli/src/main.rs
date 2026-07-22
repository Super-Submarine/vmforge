//! `vmforge` CLI: thin driver over the Hypervisor trait.
//!
//! Supports `vmforge info` (show selected backend + capabilities) and
//! `vmforge diagnose` (host/VM diagnostics bundle for bug reports).
//! Lifecycle subcommands (create/boot/snapshot/...) land with the Phase 1
//! QEMU engine.

mod diagnose;
mod redact;
mod tarball;

use std::path::PathBuf;

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

fn cmd_info() -> i32 {
    match select_backend() {
        Some(hv) => {
            let caps = hv.capabilities();
            println!("backend: {}", hv.name());
            println!("accelerator: {}", caps.accelerator);
            println!("accelerated guest archs: {:?}", caps.accelerated_archs);
            println!("live snapshot: {}", caps.live_snapshot);
            println!("virtio-gpu 3D: {}", caps.virtio_gpu_3d);
            0
        }
        None => {
            eprintln!("no hardware-accelerated backend available on this host");
            1
        }
    }
}

const DIAGNOSE_USAGE: &str =
    "usage: vmforge diagnose [--vm <name>] [--output <file[.tar]>] [--home <dir>]

Collects a redacted host/VM diagnostics report for bug reports
(see docs/diagnose.md for exactly what is collected).

  --vm <name>       only include the named VM
  --output <file>   write the bundle to <file> instead of stdout;
                    a .tar suffix produces a tarball with per-VM log excerpts
  --home <dir>      VMForge home (default: $VMFORGE_HOME or ~/.vmforge)";

fn cmd_diagnose(args: &[String]) -> i32 {
    let mut opts = diagnose::DiagnoseOptions {
        home: diagnose::default_home(),
        vm: None,
        output: None,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--vm" | "--output" | "--home" => {
                let Some(value) = it.next() else {
                    eprintln!("missing value for {arg}\n{DIAGNOSE_USAGE}");
                    return 2;
                };
                match arg.as_str() {
                    "--vm" => opts.vm = Some(value.clone()),
                    "--output" => opts.output = Some(PathBuf::from(value)),
                    _ => opts.home = PathBuf::from(value),
                }
            }
            "--help" | "-h" => {
                println!("{DIAGNOSE_USAGE}");
                return 0;
            }
            other => {
                eprintln!("unknown diagnose option: {other}\n{DIAGNOSE_USAGE}");
                return 2;
            }
        }
    }
    diagnose::run(&opts)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("info") | None => cmd_info(),
        Some("diagnose") => cmd_diagnose(&args[1..]),
        Some(other) => {
            eprintln!("unknown command: {other} (supported: info, diagnose)");
            2
        }
    };
    std::process::exit(code);
}
