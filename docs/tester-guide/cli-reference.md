# CLI Reference

Verified against the code on `main` (`crates/vmforge-cli/src/main.rs`) and by
running the binary — output samples below are real, not invented.

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

That is the complete CLI surface on `main`: **one command**. There are no other
verbs, no flags, no `--version`, no `--help`.

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

## Landing at M1 (provisional — not on `main` yet)

The M1 integration (open PRs #2–#5; see `docs/m1-integration-plan.md`) replaces
the scaffold with lifecycle verbs:

```
vmforge create <name> [--cpus N] [--memory MiB] [--disk PATH] [--disk-size SIZE] [--iso PATH]
vmforge start <name>
vmforge stop <name> [--grace secs]
vmforge status <name>
vmforge list
vmforge snapshot <create|restore|delete|list> <name> [tag]
```

These shapes are **provisional until the CLI-freeze gate** and are documented
here only so you recognize them when they land; this page will be regenerated
from the merged code at that point. Do not script against them yet.
