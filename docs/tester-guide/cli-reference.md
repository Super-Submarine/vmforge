# CLI Reference

Verified against the code on `main` (`crates/vmforge-cli/src/main.rs`) and by
running the binary — output samples below are real, not invented.

> **CLI freeze:** the wave-1 command-line surface is frozen. The authoritative
> list of stable verbs, flags, and exit codes is
> [`docs/cli-freeze-v1.0-beta.md`](../cli-freeze-v1.0-beta.md), enforced in CI
> by `qa/cli-freeze/check.py`. Everything marked *stable* there will not
> change during the beta; anything *experimental* may change without notice.

## Shipped on `main` today

### `vmforge info`

Shows the selected hypervisor backend and its capabilities. Running `vmforge`
with **no arguments** is equivalent to `vmforge info`.

```sh
$ cargo run -p vmforge-cli -- info      # or: vmforge info
backend: kvm
accelerator: kvm
accelerated guest archs: [X86_64]
live snapshot: true
virtio-gpu 3D: true
```

Backend selection: KVM if `/dev/kvm` exists (Linux), else HVF (macOS), else no
backend.

### Exit codes (verified)

| Exit | Meaning |
|---|---|
| 0 | Backend found; capabilities printed |
| 1 | `no hardware-accelerated backend available on this host` — no `/dev/kvm` (Linux) / no HVF (macOS) |
| 2 | `unknown command: <cmd> (scaffold supports: info)` — includes flags like `--version` and `--help`, which do **not** exist yet |

That is the complete surface of the `vmforge` binary on `main`: **one
command**. There are no other verbs, no flags, no `--version`, no `--help`.

### `vmforge-storage` — qcow2 disks & snapshot trees (shipped on `main`)

Storage v1 (merged) ships a second CLI, `vmforge-storage`, wrapping `qemu-img`
for disk and **offline snapshot-tree** management. Install: `cd storage &&
pip install -e .` (Python 3.10+, `qemu-utils` required). All storage lives
under `$VMFORGE_HOME` (default `~/.vmforge`); pass `--json` for
machine-readable output.

```
vmforge-storage [--home DIR] [--json] <command> ...

# disks
vmforge-storage create  <vm> <disk> <size> [--preallocation off|metadata|falloc|full] [--cluster-size 64k]
vmforge-storage resize  <vm> <disk> <size> [--shrink]
vmforge-storage import  <src> --name <image> [--format raw|vmdk|vdi|...] [--compress]   # → shared images/
vmforge-storage import  <src> --vm <vm> --disk <disk> [--format ...]                    # → VM disk
vmforge-storage clone   <base-image-or-path> <vm> <disk> [--size 20G]                   # linked clone
vmforge-storage delete  <vm> <disk> [--force]
vmforge-storage info    <vm> <disk>            # disk info incl. backing chain
vmforge-storage check   <vm> <disk>            # qemu-img check health

# snapshot tree (offline — VM must be powered off)
vmforge-storage snapshot create <vm> <disk> <name>   # freeze current state
vmforge-storage snapshot list   <vm> <disk>          # show the tree (* = current base)
vmforge-storage tree            <vm> <disk>          # alias of snapshot list
vmforge-storage snapshot revert <vm> <disk> <name>   # discard active state, branch from a snapshot
vmforge-storage snapshot delete <vm> <disk> <name>   # delete a leaf/single-child snapshot
```

Snapshots are external qcow2 layers (immutable, mode 0444); the tree is
derived from qcow2 backing-file metadata, so nothing can go stale. Live
(RAM+device) snapshots remain a core-engine concern and are **not** part of
this CLI. Full details: `storage/README.md`.

## Driving VMs on `main` today: the QA smoke suite

Until the lifecycle verbs merge, the supported way to create/boot/snapshot/
restore is `qa/smoke/smoke_test.sh` (see the
[quickstart](quickstart-linux.md#4-create--boot--snapshot--restore-your-first-vm)):

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

Accelerator selection is automatic: KVM when `/dev/kvm` exists **and is
writable**, otherwise TCG.

## In flight (open PRs — not on `main` yet)

Documented so you recognize them when they land; **do not script against
them yet**.

### Networking: `vmforge-net` (PR #2)

User-mode **NAT** networking: generates the QEMU `-netdev user` /
`-device virtio-net-pci` arguments (with `hostfwd` port forwards) and can
hot-add/remove forwards on a running VM via QMP:

```
python -m vmforge_net args [--config cfg.json] [-f 8080:80] [--format shell|lines|json]
python -m vmforge_net hostfwd-add    --qmp-unix /tmp/qmp.sock --netdev-id net0 tcp:127.0.0.1:2222-:22
python -m vmforge_net hostfwd-remove --qmp-unix /tmp/qmp.sock --netdev-id net0 tcp:127.0.0.1:2222-:22
```

Forward specs: `proto:hostip:hostport-guestip:guestport` or the shorthand
`hostport:guestport` (TCP, bound to 127.0.0.1). **Bridged/TAP mode is a
design document only** (`networking/DESIGN.md`) — no bridged/TAP
implementation exists yet.

### Guest tools: `vmforgectl` (PR #4)

Guest agent + host client over **virtio-serial** (no guest networking
required). Host-side verbs, once the agent is installed in the guest:

```
vmforgectl.py --vm <name> wait-ready       # poll until agent up
vmforgectl.py --vm <name> ping             # heartbeat
vmforgectl.py --vm <name> info             # os/kernel/hostname/agent version
vmforgectl.py --vm <name> interfaces       # [{name, mac, ips}] per NIC
vmforgectl.py --vm <name> net-info         # {hostname, ips} — the guest IP
vmforgectl.py --vm <name> shutdown [--mode reboot|halt] [--wait --shutdown-timeout N --hard-stop-cmd CMD]
vmforgectl.py --vm <name> exec -- uname -a # run a command in the guest
```

### Engine lifecycle verbs (PR #3)

The M1 integration (see `docs/m1-integration-plan.md`) replaces
the scaffold with lifecycle verbs. Shapes as implemented on the PR #3 branch
(enumerated in the [freeze doc §1.3](../cli-freeze-v1.0-beta.md), **experimental**):

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

Exit codes on that branch: 0 success, 1 any error, 2 usage error.

Snapshots there are **live** (RAM + device + disk via QMP savevm/loadvm) when
the VM runs, disk-only when stopped.

### Storage v1.2: whole-VM backup/restore (PR #28)

Two additive `vmforge-storage` verbs — portable single-file VM bundles with
manifest checksums and restore-time health checks. Additive under the freeze
(§2); experimental until promoted. Guide: [Backup & restore](backup-restore.md).

```
vmforge-storage backup  <vm> <bundle>            [--snapshot NAME]     # .tar, or .tar.gz/.tgz to compress
vmforge-storage restore <bundle> [--as NEW_VM] [--force]
```

Both honor the frozen global flags (`--home`, `--json`, `--contract-version`)
and exit codes 0/1/2.

> **Freeze note:** the storage v1.1 verbs mentioned in some engineering
> updates — disk `compact` and image `export` — are **not present** on `main`
> or any pushed branch. Only `check` (v1, frozen) and `import` (v1, frozen)
> exist. This reference does not document verbs that have no implementation.

### Networking v1.2: `vmforge net` port forwarding (PR #15)

`--forward` specs (`[tcp|udp:][HOSTIP:]HOSTPORT:GUESTPORT`, repeatable,
loopback-bound by default) plus two Rust-CLI helper verbs. Experimental under
freeze doc §4. Guide: [Port forwarding](port-forwarding.md).

```
vmforge net args        [--forward SPEC]... [--id ID] [--model MODEL] [--mac MAC] [--json]   # print the QEMU argv
vmforge net ssh-command (--forward SPEC | --host-port PORT) [--user USER]
```

### Networking v1.3: `vmforge-net doctor` (branch only — **no PR open yet**)

Connectivity diagnostics on the Python `vmforge-net` CLI
(branch `devin/1784739117-net-doctor`). Experimental. Guide:
[Diagnostics](diagnostics.md#vmforge-net-doctor--guest-connectivity-branch-no-pr-yet).

```
vmforge-net doctor [--json] [--vm NAME] [--config FILE] [--guest-exec CMD] [--timeout SECS] [--home PATH]
```

Exit codes: 0 no failures, 1 at least one FAIL, 2 usage error.

### Guest tools v1.2: `vmforge diagnose` (PR #17)

Redacted host/VM diagnostics bundle for bug reports. Additive; experimental
until promoted. Guide: [Diagnostics](diagnostics.md#vmforge-diagnose--bug-report-bundle-pr-17).

```
vmforge diagnose [--vm NAME] [--output FILE(.txt|.tar)] [--home PATH]
```

### Engine error taxonomy + `vmforge doctor` (PR #30)

An 11-class structured error taxonomy with stable codes, exit codes 10–20,
JSON error output on stderr, and a host-preflight verb. Additive (exit codes
0/1/2 keep their frozen meanings; 10–20 was reserved); the PR notes that
`doctor` and the 10–20 range must be added to the freeze manifest on
promotion. Guides: [Diagnostics](diagnostics.md#vmforge-doctor--host-preflight-pr-30),
[Troubleshooting by error code](error-codes.md).

```
vmforge doctor [--json] [--disk PATH]...
```

### Shared folders (guest tools v1.3) — **not in the repository**

Announced (virtiofs/9p) but no code or branch has been pushed; there is
nothing to document. Status page: [Shared folders](shared-folders.md).

These shapes are **experimental under the CLI freeze**
([freeze doc §1.3](../cli-freeze-v1.0-beta.md)) and are documented here only
so you recognize them when they land; on merge they can be promoted to stable
via a PR updating the freeze doc + manifest, and this page will be
regenerated from the merged code. Do not script against them yet.
