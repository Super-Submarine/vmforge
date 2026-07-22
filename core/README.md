# VMForge Core Engine (v0)

Rust library (`vmforge_core`) + CLI (`vmforge`) that drives QEMU as a
separate out-of-process engine over **QMP** (QEMU Machine Protocol).

- Accelerator: **KVM** when `/dev/kvm` is available, automatic **TCG**
  fallback otherwise (e.g. CI runners).
- Devices: **virtio** disk + NIC (user-mode networking), qcow2 disks.
- Machine type: `q35`; `-cpu host` under KVM.
- Each VM has a state directory (default `~/.vmforge/vms/<name>`, override
  with `--root` or `VMFORGE_HOME`) containing `vm.json`, `qmp.sock`,
  `qemu.pid` and `serial.log`.
- QEMU runs daemonized; the CLI returns once the QMP handshake succeeds.

## Build

```sh
cd core
cargo build --release        # binary at target/release/vmforge
cargo test                   # unit tests (uses qemu-img if installed)
```

Requires `qemu-system-x86_64` and `qemu-img` on `PATH` at runtime
(`apt install qemu-system-x86 qemu-utils`).

## CLI

```sh
# Define a VM: 2 vCPU, 1 GiB RAM, new 8G qcow2 disk, Alpine ISO as CD-ROM
vmforge create alpine --cpus 2 --memory 1024 \
    --disk ~/.vmforge/disks/alpine.qcow2 --disk-size 8G \
    --iso ~/isos/alpine-virt-3.22.0-x86_64.iso

vmforge start alpine          # boot (prints accel + QMP socket path)
vmforge status alpine         # process state + QMP run state
vmforge list                  # all defined VMs

# Snapshots — live (full machine state: RAM+devices+disk, via QMP
# savevm/loadvm) when the VM is running; disk-only (qemu-img) when stopped.
vmforge snapshot create alpine after-boot
vmforge snapshot list alpine
vmforge snapshot restore alpine after-boot
vmforge snapshot delete alpine after-boot

vmforge stop alpine           # ACPI powerdown; hard quit after --grace (30s)
```

Notes:
- Live snapshots require all writable disks to be qcow2 (they are, by
  default) and store VM state inside the qcow2 — this is the basis for the
  git-like snapshot/branching UX planned for later milestones.
- Live-ISO guests (like Alpine's boot media) run from RAM; live snapshot +
  restore brings back the exact RAM/CPU state ("instant resume").
- The guest serial console is captured to `<state-dir>/serial.log` — useful
  for boot verification and debugging.

## Library API (for GUI / networking / storage / guest-tools teams)

```rust
use vmforge_core::{Vm, VmConfig, VmStatus};

// Define + create
let root = Vm::default_root();
let cfg = VmConfig {
    name: "alpine".into(),
    cpus: 2,
    memory_mib: 1024,
    disk: "/path/alpine.qcow2".into(),
    iso: Some("/path/alpine.iso".into()),
    extra_args: vec![],                  // raw QEMU args escape hatch
};
let vm = Vm::create(&root, cfg, Some("8G"))?;   // creates qcow2 if absent

// Lifecycle
let vm = Vm::open(&root, "alpine")?;
let accel = vm.start()?;                 // "kvm" | "tcg"; waits for QMP
vm.status()?;                            // VmStatus::Running | Stopped
vm.run_state()?;                         // Some("running"|"paused"|...)
vm.stop(std::time::Duration::from_secs(30))?;

// Snapshots (auto-routes live vs offline)
vm.snapshot_create("tag")?;
vm.snapshot_list()?;                     // Vec<SnapshotInfo>
vm.snapshot_restore("tag")?;
vm.snapshot_delete("tag")?;

// Direct QMP access for advanced integrations
let mut qmp = vm.qmp()?;                 // handshake + qmp_capabilities done
qmp.execute("query-block", None)?;       // any QMP command, serde_json::Value
qmp.hmp("info snapshots")?;              // legacy HMP passthrough
```

Modules:

| Module | Purpose |
|---|---|
| `config` | `VmConfig` (serde JSON, validation) |
| `vm` | `Vm` lifecycle manager + state-dir layout |
| `qmp` | `QmpClient`: unix-socket QMP client (handshake, execute, events skipped, HMP passthrough) |
| `qemu` | QEMU command-line construction, accel selection, launch |
| `snapshot` | live (savevm/loadvm) + offline (qemu-img) snapshots, table parsing |
| `error` | `Error`/`Result` types |

## Portability plan

The engine keeps all platform specifics in `qemu::choose_accel` /
`build_command`: macOS will use `-accel hvf`, Windows `-accel whpx`, with
the same QMP control plane (TCP or named pipes where unix sockets are
unavailable). The QMP client and snapshot logic are host-agnostic.
