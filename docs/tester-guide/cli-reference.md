# CLI Reference — frozen wave-1 surface (v1.0-beta)

This is the complete command-line surface for the wave-1 beta, as frozen in
`docs/cli-freeze-v1.0-beta.md`
([PR #16](https://github.com/Super-Submarine/vmforge/pull/16)) and
verified against the code by building and running the binaries. Anything
marked **stable** will not change verb name, flag name, positional order, or
exit-code meaning until the wave-1 beta ends. Anything marked
**experimental** may change without notice — do not script against it.

The frozen surface has three parts:

1. [`vmforge`](#1-vmforge-rust-cli) — the Rust CLI (`crates/vmforge-cli`)
2. [`vmforge-storage`](#2-vmforge-storage-qcow2-disks--snapshot-trees) — qcow2 disk & snapshot-tree CLI (`storage/`)
3. [`qa/smoke/smoke_test.sh`](#3-qa-smoke-suite) — the QA smoke suite (the tester golden path until M1)

---

## 1. `vmforge` (Rust CLI)

### `vmforge info` — **stable**

Shows the selected hypervisor backend and its capabilities. Running `vmforge`
with **no arguments** is equivalent to `vmforge info`.

```sh
$ vmforge info          # or: cargo run -p vmforge-cli -- info
backend: kvm
accelerator: kvm
accelerated guest archs: [X86_64]
live snapshot: true
virtio-gpu 3D: true
```

Backend selection: KVM if `/dev/kvm` exists (Linux), else HVF (macOS), else no
backend.

`info` is the **only** verb and there are **no flags** — no `--help`, no
`--version`, no `--json`. Unknown verbs (including `--help`/`--version`)
exit 2.

### Exit codes — **stable** (verified)

| Exit | Meaning |
|---|---|
| 0 | Backend found; capabilities printed on stdout |
| 1 | `no hardware-accelerated backend available on this host` (stderr) |
| 2 | `unknown command: <cmd> (scaffold supports: info)` (stderr) |

### M1 lifecycle verbs — **experimental** (PR #3, not merged)

Enumerated in the freeze doc so you recognize them when they land, and so
UAT/AT step IDs stay reconcilable. **Do not script against these** until they
are promoted to stable:

```
vmforge [--root PATH] create <name> [--cpus N] [--memory MiB] --disk PATH [--disk-size SIZE] [--iso PATH]
vmforge [--root PATH] start <name>
vmforge [--root PATH] stop <name> [--grace SECS]
vmforge [--root PATH] status <name>
vmforge [--root PATH] list
vmforge [--root PATH] snapshot create  <name> <tag>
vmforge [--root PATH] snapshot restore <name> <tag>
vmforge [--root PATH] snapshot delete  <name> <tag>
vmforge [--root PATH] snapshot list    <name>
```

There is no CLI `pause`/`resume`, no `--json`, and no `--forward` in wave 1.

## 2. `vmforge-storage` (qcow2 disks & snapshot trees)

Python CLI in `storage/` (install: `cd storage && pip install -e .`; requires
`qemu-utils`). The **entire surface below is stable**. All snapshot
operations are **offline** — the VM must be powered off.

### Global flags — **stable**

| Flag | Meaning |
|---|---|
| `--home PATH` | VMForge home (default: `$VMFORGE_HOME` or `~/.vmforge`) |
| `--json` | Machine-readable output: exactly one JSON document on stdout |
| `--contract-version` | Print the interface-contract major version (`1`) and exit 0 |

### Disk commands — **stable**

| Command | Flags | Behavior |
|---|---|---|
| `create <vm> <disk> <size>` | `--preallocation {off,metadata,falloc,full}`, `--cluster-size SIZE` (e.g. `64k`) | Create a new qcow2 disk (size e.g. `10G`) |
| `resize <vm> <disk> <size>` | `--shrink` | Resize a disk (`--shrink` required to shrink) |
| `import <src>` | `--name NAME` (→ shared image in `images/`), `--vm VM --disk DISK` (→ VM disk), `--format FORMAT` (raw, vmdk, vdi, ...), `--compress` | Import a raw/ISO/vmdk/... image |
| `clone <base> <vm> <disk>` | `--size SIZE` | Linked clone backed by a base image (name under `images/` or a path) |
| `delete <vm> <disk>` | `--force` (also delete its snapshots) | Delete a disk |
| `info <vm> <disk>` | — | Show disk info incl. full backing chain |
| `check <vm> <disk>` | `--repair` | `qemu-img check` disk health |
| `tree <vm> <disk>` | — | Show the snapshot tree (alias of `snapshot list`) |

### Snapshot-tree commands — **stable**

| Command | Behavior |
|---|---|
| `snapshot create <vm> <disk> <name>` | Freeze current state as a read-only snapshot; start a fresh overlay on top |
| `snapshot list <vm> <disk>` | Render the snapshot tree (`*` marks the current base) |
| `snapshot revert <vm> <disk> <name>` | Discard active state; branch from any snapshot |
| `snapshot delete <vm> <disk> <name>` | Delete a leaf, or squash a single-child middle snapshot into its child |

See [Working with snapshot trees](snapshot-trees.md) for the model and a
worked branching example.

### Exit codes — **stable** (verified)

| Exit | Meaning |
|---|---|
| 0 | Success (with `--json`: one JSON document on stdout) |
| 1 | Storage/backend error — JSON error object `{"error": {"code", "message", ...}}` on stderr |
| 2 | Usage error (argparse) |
| 3 | `check` completed and found corruptions/leaks |

## 3. QA smoke suite

`qa/smoke/smoke_test.sh` is frozen as the tester golden path
(create → boot → live snapshot → restore → shutdown) until the M1 lifecycle
verbs merge — see the
[quickstart](quickstart-linux.md#4-create--boot--snapshot--restore-your-first-vm).

| Invocation / variable | Effect | Default |
|---|---|---|
| `qa/smoke/smoke_test.sh` | Happy path: create → boot → live snapshot → restore → shutdown | — |
| `qa/smoke/smoke_test.sh --negative` | Failure cases: corrupt disk, missing image, kill -9, restore of nonexistent tag | — |
| `FORCE_TCG=1` | Force TCG even when `/dev/kvm` is writable | `0` |
| `GUEST_IMAGE_URL` | Any NoCloud-compatible qcow2 cloud image | Alpine 3.20 nocloud |
| `GUEST_LOGIN_REGEX` | Serial-console readiness pattern | `login:` |
| `BOOT_TIMEOUT` | Seconds to wait for boot-to-ready | `300` |
| `WORKDIR` | Scratch + image cache dir | `qa/smoke/.work` |
| `VM_MEM` | Guest RAM (MB) | `512` |
| `DRIVER` | Backend driver in `qa/smoke/drivers/` | `qemu` |

Exit 0 = all steps passed; nonzero = failure. Accelerator selection is
automatic: KVM when `/dev/kvm` exists **and is writable**, otherwise TCG.

## 4. `vmforge-net` — **experimental** (PR #2, not merged)

`args`, `hostfwd-add`, `hostfwd-remove` with
`--config/--netdev-id/--forward/--format/--qmp-unix/--qmp-tcp`. Not frozen;
SSH port-forwarding (UAT-6) is **out of wave 1**.
