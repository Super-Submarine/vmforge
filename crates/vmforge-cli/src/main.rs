//! `vmforge` CLI: thin driver over the Hypervisor trait.
//!
//! Scaffold supports `vmforge info` (show selected backend + capabilities)
//! and `vmforge doctor [--json] [--disk PATH]...` (host preflight probes
//! mapped onto the structured error taxonomy — see `docs/error-taxonomy.md`).
//! Lifecycle subcommands (create/boot/snapshot/...) land with the Phase 1
//! QEMU engine.
//!
//! Exit codes: 0 success, 1 generic error, 2 usage error, 10-20 taxonomy
//! classes (`ErrorClass::exit_code`). With `--json`, errors are emitted as
//! one `{"error": {"code", "message", "recovery", ...}}` document on stderr
//! (interface contract §0/§4).

use std::path::PathBuf;

use vmforge_backend_hvf::HvfBackend;
use vmforge_backend_kvm::KvmBackend;
use vmforge_core::taxonomy::{EngineError, ErrorClass};
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

/// Report an error and exit with its taxonomy code.
fn fail(err: EngineError, json: bool) -> ! {
    if json {
        eprintln!("{}", err.to_json());
    } else {
        eprintln!("error: {} ({})", err.message, err.class.code());
        eprintln!("recovery: {}", err.class.recovery());
    }
    std::process::exit(err.exit_code());
}

/// Test-only failure injection: `VMFORGE_INJECT_ERROR=<code>` makes any verb
/// fail with that taxonomy class, so CI and GUI development can exercise
/// every error path end-to-end without special hardware.
fn maybe_inject(json: bool) {
    let Ok(code) = std::env::var("VMFORGE_INJECT_ERROR") else {
        return;
    };
    match ErrorClass::from_code(&code) {
        Some(class) => fail(EngineError::of(class), json),
        None => {
            eprintln!("unknown VMFORGE_INJECT_ERROR code: {code}");
            std::process::exit(2);
        }
    }
}

fn vmforge_home() -> PathBuf {
    std::env::var_os("VMFORGE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            home.join(".vmforge")
        })
}

fn cmd_info() -> ! {
    match select_backend() {
        Some(hv) => {
            let caps = hv.capabilities();
            println!("backend: {}", hv.name());
            println!("accelerator: {}", caps.accelerator);
            println!("accelerated guest archs: {:?}", caps.accelerated_archs);
            println!("live snapshot: {}", caps.live_snapshot);
            println!("virtio-gpu 3D: {}", caps.virtio_gpu_3d);
            std::process::exit(0);
        }
        None => {
            eprintln!("no hardware-accelerated backend available on this host");
            std::process::exit(1);
        }
    }
}

fn cmd_doctor(args: &[String]) -> ! {
    let mut json = false;
    let mut disks: Vec<PathBuf> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--disk" => match it.next() {
                Some(path) => disks.push(PathBuf::from(path)),
                None => {
                    eprintln!("doctor: --disk requires a path");
                    std::process::exit(2);
                }
            },
            other => {
                eprintln!("doctor: unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }
    maybe_inject(json);

    let home = vmforge_home();
    let results = vmforge_core::doctor::run_all(&home, &disks);
    let first_failure = results.iter().find(|r| r.result.is_err());

    if json {
        let probes: Vec<serde_json::Value> = results
            .iter()
            .map(|r| match &r.result {
                Ok(summary) => serde_json::json!({
                    "probe": r.name, "ok": true, "summary": summary,
                }),
                Err(e) => serde_json::json!({
                    "probe": r.name, "ok": false, "error": e.to_json()["error"],
                }),
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({ "ok": first_failure.is_none(), "probes": probes })
        );
        match first_failure {
            Some(probe) => {
                let err = probe.result.as_ref().unwrap_err();
                eprintln!("{}", err.to_json());
                std::process::exit(err.exit_code());
            }
            None => std::process::exit(0),
        }
    }

    for r in &results {
        match &r.result {
            Ok(summary) => println!("ok   {:<5} {summary}", r.name),
            Err(e) => {
                println!("FAIL {:<5} {} ({})", r.name, e.message, e.class.code());
                println!("     recovery: {}", e.class.recovery());
            }
        }
    }
    match first_failure {
        Some(probe) => std::process::exit(probe.result.as_ref().unwrap_err().exit_code()),
        None => std::process::exit(0),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("info") | None => {
            maybe_inject(false);
            cmd_info()
        }
        Some("doctor") => cmd_doctor(&args[1..]),
        Some(other) => {
            eprintln!("unknown command: {other} (scaffold supports: info, doctor)");
            std::process::exit(2);
        }
    }
}
