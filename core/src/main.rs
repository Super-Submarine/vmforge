use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use vmforge_core::{Vm, VmConfig, VmStatus};

#[derive(Parser)]
#[command(
    name = "vmforge",
    version,
    about = "VMForge core engine CLI: create, boot and snapshot VMs via QEMU/QMP"
)]
struct Cli {
    /// Root directory for VM state (default: $VMFORGE_HOME/vms or ~/.vmforge/vms)
    #[arg(long, global = true)]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Define a new VM (and create its qcow2 disk if --disk-size is given)
    Create {
        name: String,
        #[arg(long, default_value_t = 2)]
        cpus: u32,
        /// Memory in MiB
        #[arg(long, default_value_t = 1024)]
        memory: u32,
        /// Path to the qcow2 boot disk
        #[arg(long)]
        disk: PathBuf,
        /// Create the disk with this size (e.g. 8G) if it does not exist
        #[arg(long)]
        disk_size: Option<String>,
        /// ISO to attach as CD-ROM
        #[arg(long)]
        iso: Option<PathBuf>,
    },
    /// Boot a VM
    Start { name: String },
    /// Stop a VM (ACPI powerdown, hard quit after --grace seconds)
    Stop {
        name: String,
        #[arg(long, default_value_t = 30)]
        grace: u64,
    },
    /// Show VM status
    Status { name: String },
    /// List all VMs
    List,
    /// Snapshot operations (live via QMP savevm/loadvm when running,
    /// offline via qemu-img when stopped)
    Snapshot {
        #[command(subcommand)]
        op: SnapCmd,
    },
}

#[derive(Subcommand)]
enum SnapCmd {
    Create { name: String, tag: String },
    Restore { name: String, tag: String },
    Delete { name: String, tag: String },
    List { name: String },
}

fn main() {
    let cli = Cli::parse();
    let root = cli.root.unwrap_or_else(Vm::default_root);
    if let Err(e) = run(&root, cli.command) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(root: &std::path::Path, cmd: Cmd) -> vmforge_core::Result<()> {
    match cmd {
        Cmd::Create {
            name,
            cpus,
            memory,
            disk,
            disk_size,
            iso,
        } => {
            let config = VmConfig {
                name: name.clone(),
                cpus,
                memory_mib: memory,
                disk,
                iso,
                extra_args: vec![],
            };
            let vm = Vm::create(root, config, disk_size.as_deref())?;
            println!("created VM '{name}' ({})", vm.state_dir().display());
        }
        Cmd::Start { name } => {
            let vm = Vm::open(root, &name)?;
            let accel = vm.start()?;
            println!(
                "started VM '{name}' (accel={accel}, qmp={})",
                vm.qmp_socket().display()
            );
        }
        Cmd::Stop { name, grace } => {
            let vm = Vm::open(root, &name)?;
            vm.stop(Duration::from_secs(grace))?;
            println!("stopped VM '{name}'");
        }
        Cmd::Status { name } => {
            let vm = Vm::open(root, &name)?;
            match vm.status()? {
                VmStatus::Running => {
                    let state = vm.run_state()?.unwrap_or_else(|| "unknown".into());
                    println!("{name}: running (qmp state: {state})");
                }
                VmStatus::Stopped => println!("{name}: stopped"),
            }
        }
        Cmd::List => {
            for name in Vm::list(root)? {
                println!("{name}");
            }
        }
        Cmd::Snapshot { op } => match op {
            SnapCmd::Create { name, tag } => {
                let mode = Vm::open(root, &name)?.snapshot_create(&tag)?;
                println!("snapshot '{tag}' created ({mode})");
            }
            SnapCmd::Restore { name, tag } => {
                let mode = Vm::open(root, &name)?.snapshot_restore(&tag)?;
                println!("snapshot '{tag}' restored ({mode})");
            }
            SnapCmd::Delete { name, tag } => {
                Vm::open(root, &name)?.snapshot_delete(&tag)?;
                println!("snapshot '{tag}' deleted");
            }
            SnapCmd::List { name } => {
                let snaps = Vm::open(root, &name)?.snapshot_list()?;
                if snaps.is_empty() {
                    println!("no snapshots");
                } else {
                    let (id, tag, size, date, clock) = ("ID", "TAG", "VM SIZE", "DATE", "VM CLOCK");
                    println!("{id:<4} {tag:<20} {size:>10} {date:<20} {clock}");
                    for s in snaps {
                        println!(
                            "{:<4} {:<20} {:>10} {:<20} {}",
                            s.id, s.tag, s.vm_size, s.date, s.vm_clock
                        );
                    }
                }
            }
        },
    }
    Ok(())
}
