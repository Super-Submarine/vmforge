# VMForge subsystem interface contracts (v1, M1-binding)

**Status:** Binding for M1 · **Owner:** Arjun (VP Engineering) · **Date:** 2026-07-22
**Inputs:** HAL architecture spike (`docs/architecture.md` on `devin/1784731275-hypervisor-scaffold`, Doc 019f8a48-b4bb-74d3-8a8a-1671ce28677e), MVP PRD v1 (Doc 019f8a41-69a4-76fe-af3d-71589745cfa1), and the five open v0 PRs (#1–#5).

These contracts are the seams between the six parallel workstreams. Once this
doc merges to `main`, **changing a contract requires a PR that edits this file
and tags every affected subsystem owner.** Code may lag the contract; it may
not contradict it.

## 0. Ground rules (process & language model)

Current reality across the v0 branches:

- **Engine & HAL:** Rust (`core/` on PR #3, `crates/` workspace on PR #1 — to be unified, see the M1 plan).
- **Storage, networking, guest-tools host client:** Python libraries + CLIs.
- **GUI alpha:** not yet pushed; consumes the engine only.

**Process model for M1:** one Rust engine process per host user session
(`vmforged` embedded in the CLI for M1 — no daemon yet). Each VM is a QEMU
child process owned by the engine. Python subsystems are consumed two ways:

1. **Conventions/contracts** (file layouts, QEMU argv fragments, wire
   protocols) that the Rust engine implements natively — this is the M1 path
   for networking (argv generation) and guest-tools (socket protocol).
2. **Subprocess CLIs with `--json` output** for offline operations — this is
   the M1 path for storage (`vmforge-storage`), until it is ported to Rust
   post-M1. Exit code 0 + JSON on stdout = success; nonzero + JSON error
   object `{"error": {"code": <string>, "message": <string>}}` on stderr =
   failure.

**Threading model:** the engine is synchronous per-VM. All QMP traffic for a
VM goes through a single owned connection guarded by the `Vm` handle; the
engine never shares a QMP socket across threads. Long operations (boot wait,
snapshot) block the calling thread and must respect the caller-supplied
timeout. GUI concurrency is handled at the GUI/engine boundary (§4), not
inside the engine.

**Canonical shared types** (Rust, in `vmforge-core`; the JSON forms below are
the wire/CLI representations — field names are `snake_case`, enums are
lowercase strings):

```rust
pub struct VmName(String);        // [A-Za-z0-9][A-Za-z0-9._-]* (same regex everywhere)
pub struct DiskName(String);      // same regex
pub struct SnapshotName(String);  // same regex
pub struct ImageName(String);     // same regex

pub enum VmState { Defined, Running, Paused, Stopped }   // per HAL FSM

pub struct VmConfig {
    pub name: VmName,
    pub cpus: u32,                 // >= 1
    pub memory_mib: u64,           // >= 128
    pub disks: Vec<DiskName>,      // resolved via storage layout, §1
    pub nics: Vec<NicConfig>,      // §2
    pub guest_agent: bool,         // §3; default true
    pub display: DisplayConfig,    // §4 (none | vnc { port } for M1)
}
// Persisted as vm.json at $VMFORGE_HOME/vms/<name>/vm.json (schema_version: 1).
```

**Error semantics (all subsystems):** errors carry a stable machine-readable
`code` (string), a human `message`, and optional `details`. Rust:

```rust
pub enum ErrorKind {
    NotFound,        // VM/disk/snapshot/image does not exist
    AlreadyExists,
    InvalidConfig,   // validation failure, incl. bad names
    InvalidState,    // FSM violation, e.g. boot() while RUNNING
    Backend,         // QEMU/qemu-img/QMP failure; details carry stderr/QMP error class
    Timeout,
    Io,
}
pub struct VmError { pub kind: ErrorKind, pub message: String, pub details: Option<serde_json::Value> }
```

JSON/CLI form: `{"error": {"code": "invalid_state", "message": "...", "details": {...}}}`.
Subsystems MUST NOT panic/uncaught-except across the boundary.

**Home directory:** every subsystem resolves `$VMFORGE_HOME`, defaulting to
`~/.vmforge`. Nobody hardcodes paths outside it.

---

## 1. Engine ⟷ Storage

Storage owns everything under `$VMFORGE_HOME/images` and
`$VMFORGE_HOME/vms/<vm>/{disks,snapshots}`. The engine owns
`$VMFORGE_HOME/vms/<vm>/vm.json` and runtime files
(`qmp.sock`, `qemu.pid`, `serial.log`, `guest-agent.sock`).

**Layout contract (from storage v0 README, now binding):**

```
$VMFORGE_HOME/
├── images/<image>.qcow2                     # imported read-only bases
└── vms/<vm>/
    ├── vm.json                              # engine-owned
    ├── disks/<disk>.qcow2                   # ACTIVE writable overlay — the ONLY file QEMU opens
    └── snapshots/<disk>/<snapshot>.qcow2    # frozen layers (0444); parent = qcow2 backing file
```

**Rust trait the engine programs against** (`vmforge-core::storage`; M1 impl
`SubprocessStore` shells out to `vmforge-storage --json`, post-M1 impl is
native Rust — callers cannot tell):

```rust
pub trait StorageProvider: Send + Sync {
    fn import_image(&self, src: &Path, name: &ImageName) -> Result<ImageInfo, VmError>;
    fn create_disk(&self, vm: &VmName, disk: &DiskName, size_bytes: u64,
                   base: Option<&ImageName>) -> Result<DiskInfo, VmError>;
    fn attach_path(&self, vm: &VmName, disk: &DiskName) -> PathBuf; // vms/<vm>/disks/<disk>.qcow2
    fn snapshot(&self, vm: &VmName, disk: &DiskName, name: &SnapshotName)
        -> Result<SnapshotInfo, VmError>;                 // OFFLINE only: VM must not be RUNNING/PAUSED
    fn revert(&self, vm: &VmName, disk: &DiskName, snapshot: &SnapshotName)
        -> Result<(), VmError>;                           // recreates active overlay on top of <snapshot>
    fn tree(&self, vm: &VmName, disk: &DiskName) -> Result<Vec<SnapshotInfo>, VmError>;
    fn delete_disk(&self, vm: &VmName, disk: &DiskName) -> Result<(), VmError>;
}

pub struct SnapshotInfo { pub name: SnapshotName, pub parent: Option<SnapshotName>,
                          pub children: Vec<SnapshotName>, pub current: bool,
                          pub virtual_size: u64, pub actual_size: u64 }
```

**Invariants (both sides enforce):**

- S1: QEMU only ever opens the active overlay; snapshot files are immutable.
- S2: Disk-level (offline) snapshot/revert require the VM **not** RUNNING or
  PAUSED — the engine checks the FSM before calling; storage may not verify
  process state and trusts the engine. Live whole-VM snapshots
  (disk+RAM via QMP `snapshot-save`) are engine-owned and out of storage's
  scope for M1; see M1 plan §2 for which one M1 uses (answer: offline).
- S3: The snapshot tree is derivable purely from qcow2 backing-file metadata —
  no sidecar DB. `current` = backing file of the active overlay.
- S4: `vmforge-storage` CLI is versioned: `vmforge-storage --contract-version`
  prints `1`. The engine refuses to run against an unknown major version.

**Error mapping:** storage `not_found` → `NotFound`, name-regex violations →
`InvalidConfig`, `qemu-img` nonzero → `Backend` with `details.stderr`.

---

## 2. Engine ⟷ Networking

For M1, networking is **argv generation + QMP port-forward management** for
user-mode NAT (SLIRP). No host privileges, no TAP (v1 design, post-M1).

**Rust trait** (`vmforge-core::net`; M1 impl is native Rust following the
`vmforge_net` v0 semantics — the Python package remains the reference
implementation and test oracle):

```rust
pub struct NicConfig {
    pub id: String,                    // netdev id, unique per VM: "net<n>"
    pub mode: NetMode,                 // M1: Nat only
    pub model: String,                 // default "virtio-net-pci"
    pub mac: Option<String>,           // engine assigns 52:54:00:xx:xx:xx if None
    pub port_forwards: Vec<PortForward>,
}
pub enum NetMode { Nat }               // v1 adds Bridged { bridge: String }, HostOnly
pub struct PortForward { pub proto: Proto, pub host_ip: Option<IpAddr>,
                         pub host_port: u16, pub guest_port: u16 }  // proto: Tcp | Udp

pub trait NetworkBackend: Send + Sync {
    /// Pure function: QEMU argv fragment for one NIC.
    /// NAT: ["-netdev", "user,id=<id>[,hostfwd=tcp:...]...", "-device", "<model>,netdev=<id>,mac=<mac>"]
    fn qemu_args(&self, nic: &NicConfig) -> Result<Vec<String>, VmError>;
    /// Runtime port-forward add/remove on a live VM, via the engine's QMP conn:
    /// QMP `human-monitor-command` wrapping HMP `hostfwd_add`/`hostfwd_remove`.
    fn hostfwd_add(&self, qmp: &mut dyn QmpConn, nic_id: &str, fwd: &PortForward) -> Result<(), VmError>;
    fn hostfwd_remove(&self, qmp: &mut dyn QmpConn, nic_id: &str, fwd: &PortForward) -> Result<(), VmError>;
}
```

**Invariants:**

- N1: `qemu_args` is deterministic and side-effect free (unit-testable without
  QEMU; the Python `natgen` test vectors in `networking/tests/test_natgen.py`
  are the conformance suite — the Rust impl must produce identical argv).
- N2: All runtime networking mutations go through the engine's QMP connection;
  networking code never opens its own socket to a VM the engine owns.
- N3: Host-port conflicts surface as `Backend` errors with
  `details.hmp_output`; the engine does not pre-check port availability.

---

## 3. Engine ⟷ Guest tools

Transport: **virtio-serial** channel, wired by the engine at QEMU launch.
Guest-tools owns the in-guest agent and the wire protocol; the engine owns the
QEMU flags and the host socket.

**QEMU argv contract (engine MUST emit exactly, when `guest_agent: true`):**

```
-device virtio-serial-pci,id=vmforge-vs0
-chardev socket,id=vmforge-ga0,path=$VMFORGE_HOME/vms/<vm>/guest-agent.sock,server=on,wait=off
-device virtserialport,bus=vmforge-vs0.0,chardev=vmforge-ga0,name=org.vmforge.agent.0
```

(Guest side: `/dev/virtio-ports/org.vmforge.agent.0`.) Note: the v0 host
client used `/tmp/vmforge-ga.sock`; the per-VM path above is the binding form.

**Wire protocol (v0, line-delimited JSON, QMP-style):**

```
request:  {"execute": "<command>", "id": <int>, "arguments": {...}?}\n
response: {"id": <int>, "return": {...}} | {"id": <int>, "error": {"code": "...", "message": "..."}}
```

Commands for M1: `ping` → `{}`; `info` → `{os, kernel, hostname, agent_version}`;
`interfaces` → `[{name, mac, ips: [..]}]`; `shutdown {mode: powerdown|reboot|halt}` → `{}`.

**Rust trait** (`vmforge-core::guest`; M1 impl is a native Rust client
speaking the protocol above; `guest-tools/host/vmforgectl.py` stays as the
reference client + debugging tool):

```rust
pub trait GuestAgent: Send + Sync {
    fn ping(&mut self, timeout: Duration) -> Result<(), VmError>;
    fn info(&mut self) -> Result<GuestInfo, VmError>;
    fn interfaces(&mut self) -> Result<Vec<GuestNic>, VmError>;
    fn shutdown(&mut self, mode: ShutdownMode) -> Result<(), VmError>; // powerdown|reboot|halt
}
```

**Invariants:**

- G1: Requests are strictly serialized per channel; `id` matches responses to
  requests; unknown commands return `error.code = "unknown_command"` (never a
  disconnect).
- G2: Agent absence is not an engine error: `ping` timeout → `Timeout`; the
  engine's `vm status` reports `guest_agent: "connected" | "unavailable"`.
- G3: Protocol is versioned via `info.agent_version` (semver); additive
  changes only within major version 0/1.

---

## 4. Engine ⟷ GUI (and any automation client)

The GUI never touches QEMU, QMP, storage files, or sockets directly — it
consumes the engine exclusively. For M1 the surface is the **`vmforge` CLI
with `--json`** on every command (machine-readable, stable); post-M1 this
same schema moves onto a local JSON-RPC socket without changing shapes.

**Command surface (M1):**

```
vmforge create <name> --cpus N --memory MIB --disk-size BYTES [--image NAME] [--forward tcp:HOST:GUEST]...
vmforge boot <name>            # DEFINED|STOPPED → RUNNING; waits for QMP handshake
vmforge stop <name> [--force]  # graceful via guest agent/ACPI, --force = SIGKILL QEMU
vmforge pause <name> / resume <name>
vmforge snapshot <name> <snapshot> [--disk DISK]     # M1: offline; refuses if RUNNING/PAUSED
vmforge restore <name> <snapshot>                    # offline revert, then boot
vmforge list / status <name> / tree <name> / delete <name>
```

Every command with `--json` prints exactly one JSON document on stdout.
`status` shape:

```json
{"name": "vm1", "state": "running", "cpus": 2, "memory_mib": 2048,
 "disks": [{"name": "disk0", "current_snapshot": "base"}],
 "nics": [{"id": "net0", "mode": "nat", "port_forwards": [{"proto": "tcp", "host_port": 2222, "guest_port": 22}]}],
 "guest_agent": "connected", "display": {"type": "vnc", "port": 5901}}
```

**Console:** M1 exposes the guest display via VNC (`-display none -vnc :N`),
port reported in `status.display`. The GUI console viewer embeds a VNC client.
Serial log is at `vms/<vm>/serial.log` for headless debugging.

**Events:** M1 = polling `vmforge status --json` (1s). Post-M1: the JSON-RPC
socket adds a `subscribe` stream of `{"event": "state_changed", ...}`.

**Threading/process contract:** engine commands are safe to invoke
concurrently for *different* VMs; concurrent mutations of the *same* VM are
serialized by an advisory lock on `vms/<vm>/` (`flock` on `vm.json`), second
caller gets `InvalidState` with `details.reason = "busy"` rather than
blocking > 2s.

**Invariants:**

- U1: exit code 0 ⇔ success; nonzero + `{"error": ...}` JSON on stderr.
- U2: JSON schemas are additive-only within contract version 1;
  `vmforge --contract-version` prints `1`.
- U3: state strings match the FSM (`defined|running|paused|stopped`).
