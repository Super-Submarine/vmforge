# Linux Quickstart — first VM in ~10 minutes

Everything below is copy-pasteable, verified against `main`, and uses only
commands frozen for the wave-1 beta (see the [CLI reference](cli-reference.md)).
Target: fresh checkout → booted VM → snapshot → restore.

> Installing from a packaged release (`.deb`/`.AppImage`) instead of building
> from source? See [Installing VMForge on Linux](install-linux.md), then
> rejoin at step 2.

## 1. Prerequisites

Debian/Ubuntu:

```sh
sudo apt update
sudo apt install -y qemu-system-x86 qemu-utils genisoimage python3 curl git
```

Fedora:

```sh
sudo dnf install -y qemu-system-x86 qemu-img genisoimage python3 curl git
```

You also need a Rust toolchain to build the CLI (until packaged binaries ship):
https://rustup.rs — then `rustup default stable`.

## 2. Verify KVM (do this before anything else)

```sh
ls -l /dev/kvm                                   # must exist
[ -w /dev/kvm ] && echo writable || echo NOT-writable
```

If `/dev/kvm` is missing or not writable, fix it now — see
[Troubleshooting T1](troubleshooting.md#t1-kvm-not-available-or-not-writable).
Without writable `/dev/kvm` everything still works but falls back to slow TCG
emulation (boots take minutes instead of seconds).

Quick fixes:

```sh
sudo modprobe kvm_intel          # or kvm_amd
sudo usermod -aG kvm "$USER"     # then log out/in, or: newgrp kvm
```

## 3. Build and check the backend

```sh
git clone https://github.com/Super-Submarine/vmforge.git
cd vmforge
cargo build --workspace                # ~1–3 min first build
cargo run -p vmforge-cli -- info
```

Expected on a healthy KVM host:

```
backend: kvm
accelerator: kvm
accelerated guest archs: [X86_64]
live snapshot: true
virtio-gpu 3D: true
```

If instead you get `no hardware-accelerated backend available on this host`
(exit 1), go to [Troubleshooting T1](troubleshooting.md#t1-kvm-not-available-or-not-writable).

## 4. Create → boot → snapshot → restore your first VM

The automated smoke suite runs the whole golden path (create, boot to login,
live snapshot, restore, graceful shutdown) against a known-good Alpine cloud
image:

```sh
qa/smoke/smoke_test.sh
```

Expected timings (Alpine, 1 vCPU, 512 MB — from `qa/TEST_PLAN.md`):

| Step | KVM | TCG fallback |
|---|---|---|
| Boot to login prompt | ~10–25 s | ~60–180 s |
| Full run incl. snapshot/restore + shutdown | ~2–4 min | ~5–8 min |

(First run also downloads the ~200 MB guest image; the image is cached in
`qa/smoke/.work/` afterwards.)

A passing run ends with `PASS` counts and no `FAIL` lines. If it fails, note
the failing step name — that is the step ID to put in your bug report.

Useful variants:

```sh
qa/smoke/smoke_test.sh --negative     # failure-mode cases (corrupt disk, bad restore, kill -9)
FORCE_TCG=1 qa/smoke/smoke_test.sh    # force TCG even when KVM is available
GUEST_IMAGE_URL=<url> qa/smoke/smoke_test.sh   # any NoCloud-compatible qcow2 cloud image
```

## 5. Manage disks & snapshot trees with `vmforge-storage`

Offline disk and snapshot-tree management ships today as the `vmforge-storage`
CLI (stable under the wave-1 CLI freeze). Install it once:

```sh
cd storage && pip install -e .        # requires qemu-utils (already installed in step 1)
```

Then (VM powered off):

```sh
vmforge-storage create dev root 10G                    # new qcow2 disk under ~/.vmforge
vmforge-storage snapshot create dev root clean-install # freeze current state
vmforge-storage snapshot list dev root                 # render the tree; * = current
vmforge-storage snapshot revert dev root clean-install # branch back to any snapshot
vmforge-storage check dev root                         # verify disk integrity
```

Full walkthrough incl. branching: [Working with snapshot trees](snapshot-trees.md).

Lifecycle verbs (`vmforge create/start/snapshot/restore/stop`) land with the M1
merge — see the [CLI reference](cli-reference.md) for what is stable today vs.
experimental.

## 6. What to do next

Run your acceptance scripts (AT-1…AT-5 from your welcome email), file anything
that fails per [Reporting bugs](reporting-bugs.md), and keep rough wall-clock
timings — the day-3 survey asks for install → first boot time.
