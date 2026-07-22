//! `vmforge` CLI: thin driver over the Hypervisor trait.
//!
//! Supports `vmforge info` (show selected backend + capabilities),
//! `vmforge diagnose` (host/VM diagnostics bundle for bug reports), and
//! the EXPERIMENTAL `vmforge net` group (user-mode port forwarding).
//! Lifecycle subcommands (create/boot/snapshot/...) land with the Phase 1
//! QEMU engine; the `--forward` flag below is the same flag `create` will
//! accept per `docs/interface-contracts.md` §4.

mod diagnose;
mod redact;
mod tarball;

use std::path::PathBuf;

use vmforge_backend_hvf::HvfBackend;
use vmforge_backend_kvm::KvmBackend;
use vmforge_core::net::{NetworkBackend, NicConfig, PortForward, UserNetBackend};
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
        Some("info") | None => cmd_info(),
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
            eprintln!("unknown command: {other} (supported: info, diagnose, net [EXPERIMENTAL])");
            2
        }
    };
    std::process::exit(code);
}
