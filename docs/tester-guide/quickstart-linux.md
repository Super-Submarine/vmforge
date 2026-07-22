# Linux Quickstart — first VM in ~10 minutes

Everything below is copy-pasteable and verified against `main`. Target: fresh
checkout → booted VM → snapshot → restore.

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

## 5. Snapshot / restore by hand (what the suite does underneath)

Disks are qcow2 with copy-on-write overlays; live snapshots capture RAM +
device state + disk in one tag:

```sh
cd qa/smoke/.work
qemu-img create -f qcow2 -b nocloud_alpine-3.20.3-x86_64-bios-cloudinit-r0.qcow2 -F qcow2 mydisk.qcow2
qemu-img snapshot -l mydisk.qcow2     # list snapshot tags on a disk
qemu-img check mydisk.qcow2           # verify disk integrity
```

For managed disks and git-like **offline snapshot trees** (create / branch /
revert / delete, VM powered off), use the `vmforge-storage` CLI shipped on
`main` — see the [CLI reference](cli-reference.md#vmforge-storage--qcow2-disks--snapshot-trees-shipped-on-main):

```sh
cd storage && pip install -e .
vmforge-storage create demo root 10G
vmforge-storage snapshot create demo root clean-install
vmforge-storage tree demo root        # * marks the current base
```

Lifecycle verbs (`vmforge create/start/snapshot/restore/stop`) land with the M1
merge — see the [CLI reference](cli-reference.md) for what is shipped today vs.
still in open PRs (engine verbs, `vmforge-net` networking, `vmforgectl` guest
tools). Curious about the desktop app? See the
[GUI alpha user guide](gui-guide.md) — it is a UX preview, not yet wired to
real VMs.

## 6. What to do next

Run your acceptance scripts (AT-1…AT-5 from your welcome email), file anything
that fails per [Reporting bugs](reporting-bugs.md), and keep rough wall-clock
timings — the day-3 survey asks for install → first boot time.
