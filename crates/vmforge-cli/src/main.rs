//! `vmforge` CLI: thin driver over the Hypervisor trait.
//!
//! Supports `vmforge info` (show selected backend + capabilities),
//! `vmforge diagnose` (host/VM diagnostics bundle for bug reports),
//! `vmforge doctor [--json] [--disk PATH]...` (host preflight probes
//! mapped onto the structured error taxonomy — see `docs/error-taxonomy.md`),
//! and the EXPERIMENTAL `vmforge net` group (user-mode port forwarding).
//! Lifecycle subcommands (create/boot/snapshot/...) land with the Phase 1
//! QEMU engine; the `--forward` flag below is the same flag `create` will
//! accept per `docs/interface-contracts.md` §4.
//!
//! Exit codes: 0 success, 1 generic error, 2 usage error, 10-20 taxonomy
//! classes (`ErrorClass::exit_code`). With `--json`, errors are emitted as
//! one `{"error": {"code", "message", "recovery", ...}}` document on stderr
//! (interface contract §0/§4).

mod diagnose;
mod redact;
mod tarball;

use std::path::PathBuf;

use vmforge_backend_hvf::HvfBackend;
use vmforge_backend_kvm::KvmBackend;
use vmforge_core::net::{NetworkBackend, NicConfig, PortForward, UserNetBackend};
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

const NET_USAGE: &str = "usage: vmforge net <args|ssh-command> [options]  (EXPERIMENTAL)

  vmforge net args [--forward SPEC]... [--id ID] [--model MODEL] [--mac MAC] [--json]
      Print the QEMU argv fragment for one user-mode (SLIRP) NIC.
      SPEC: [tcp|udp:][HOSTIP:]HOSTPORT:GUESTPORT   e.g. --forward 2222:22

  vmforge net ssh-command [--forward SPEC]... [--host-port PORT] [--user USER]
      Print the ssh invocation that reaches a guest's port 22 through a
      forward (convenience helper for UAT-6).";

fn die(msg: &str, code: i32) -> ! {
    eprintln!("{msg}");
    std::process::exit(code);
}

struct NetOpts {
    forwards: Vec<PortForward>,
    id: String,
    model: Option<String>,
    mac: Option<String>,
    json: bool,
    host_port: Option<u16>,
    user: Option<String>,
}

fn parse_net_opts(args: &[String]) -> NetOpts {
    let mut opts = NetOpts {
        forwards: Vec::new(),
        id: "net0".to_string(),
        model: None,
        mac: None,
        json: false,
        host_port: None,
        user: None,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut value = |name: &str| -> String {
            it.next()
                .unwrap_or_else(|| die(&format!("{name} requires a value\n\n{NET_USAGE}"), 2))
                .clone()
        };
        match flag.as_str() {
            "--forward" => {
                let spec = value("--forward");
                match PortForward::parse(&spec) {
                    Ok(f) => opts.forwards.push(f),
                    Err(e) => die(&format!("{e}"), 2),
                }
            }
            "--id" => opts.id = value("--id"),
            "--model" => opts.model = Some(value("--model")),
            "--mac" => opts.mac = Some(value("--mac")),
            "--json" => opts.json = true,
            "--host-port" => {
                let v = value("--host-port");
                match v.parse::<u16>() {
                    Ok(p) if p != 0 => opts.host_port = Some(p),
                    _ => die(&format!("invalid --host-port '{v}'"), 2),
                }
            }
            "--user" => opts.user = Some(value("--user")),
            other => die(&format!("unknown option: {other}\n\n{NET_USAGE}"), 2),
        }
    }
    opts
}

fn nic_from_opts(opts: &NetOpts) -> NicConfig {
    let mut nic = NicConfig::nat(opts.id.clone());
    if let Some(model) = &opts.model {
        nic.model = model.clone();
    }
    nic.mac = opts.mac.clone();
    nic.port_forwards = opts.forwards.clone();
    nic
}

fn net_args(opts: &NetOpts) {
    let nic = nic_from_opts(opts);
    match UserNetBackend::new().qemu_args(&nic) {
        Ok(args) => {
            if opts.json {
                let doc = serde_json::json!({
                    "nic": nic,
                    "qemu_args": args,
                });
                println!("{doc}");
            } else {
                println!("{}", args.join(" "));
            }
        }
        Err(e) => die(&format!("{e}"), 1),
    }
}

fn net_ssh_command(opts: &NetOpts) {
    // Prefer an explicit --host-port; otherwise use the first tcp forward
    // whose guest port is 22.
    let port = opts.host_port.or_else(|| {
        opts.forwards
            .iter()
            .find(|f| f.guest_port == 22)
            .map(|f| f.host_port)
    });
    let Some(port) = port else {
        die(
            "no SSH forward found: pass --host-port PORT or a --forward mapping guest port 22",
            2,
        );
    };
    let user = opts.user.as_deref().unwrap_or("root");
    println!("ssh -p {port} {user}@127.0.0.1");
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
        Some("info") | None => {
            maybe_inject(false);
            cmd_info()
        }
        Some("doctor") => cmd_doctor(&args[1..]),
        Some("diagnose") => cmd_diagnose(&args[1..]),
        Some("net") => {
            eprintln!("note: 'vmforge net' is EXPERIMENTAL and may change before CLI freeze");
            let sub = args.get(1).map(String::as_str);
            let opts = parse_net_opts(args.get(2..).unwrap_or(&[]));
            match sub {
                Some("args") => net_args(&opts),
                Some("ssh-command") => net_ssh_command(&opts),
                _ => die(NET_USAGE, 2),
            }
            0
        }
        Some(other) => {
            eprintln!(
                "unknown command: {other} (supported: info, doctor, diagnose, net [EXPERIMENTAL])"
            );
            2
        }
    };
    std::process::exit(code);
}
