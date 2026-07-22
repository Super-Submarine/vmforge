# VMForge wedge-claim benchmark harness

Reproducible performance evidence for the wedge claims from the market-wedge
synthesis (D1: git-like snapshots + instant resume; D2: one engine, three
hosts): VM **cold-boot time**, **live snapshot create** (incl. RAM),
**snapshot restore / instant resume** latency, and **snapshot storage
overhead**, measured for a small guest across multiple virtualization stacks
on a Linux/KVM host.

## Stacks measured

| Stack | Script | What it represents |
|---|---|---|
| vmforge proxy — QEMU/KVM driven over QMP (`snapshot-save`/`snapshot-load` jobs, qcow2) | `bench_qemu.py --mode proxy` | The VMForge Phase-1 architecture (`docs/architecture.md`): QEMU child process controlled over QMP. **The real vmforge KVM driver is not merged yet** (backend crates are stubs, CLI is `vmforge info` only), so raw QEMU/KVM+qcow2 driven exactly the way vmforge will drive it is benchmarked as its stated proxy. |
| raw QEMU/KVM (HMP `savevm`/`loadvm`, qcow2 internal snapshots) | `bench_qemu.py --mode raw` | What a user gets from QEMU by hand today — the baseline vmforge must beat on UX, not raw speed. |
| VirtualBox (live snapshots, savestate/resume) | `bench_vbox.sh` | The main free desktop-hypervisor incumbent. Requires a working `vboxdrv` kernel module; the script emits a machine-readable `blocked` result when the host kernel cannot build it (see caveats). |
| Firecracker (KVM microVM, snapshot/restore API) | `bench_firecracker.py` | State-of-the-art snapshot/instant-resume reference — the latency ceiling for the D1 "instant resume" claim. |

**UTM is deferred to the HVF milestone**: UTM is macOS-only (Hypervisor.framework
/ Apple silicon; https://mac.getutm.app/, https://docs.getutm.app/), so it
cannot run on this Linux/KVM host at all. It will be benchmarked when the
`vmforge-backend-hvf` milestone brings CI/bench coverage to a macOS host.
UTM also has no snapshot UI (its top open feature request,
https://github.com/utmapp/UTM/issues/5484), so only its boot numbers would be
comparable anyway.

## Guest images

- QEMU/VirtualBox: Alpine Linux 3.22 `nocloud` tiny cloud image (x86_64, BIOS)
  from https://dl-cdn.alpinelinux.org/alpine/v3.22/releases/cloud/ — 512 MiB
  RAM, 1 vCPU, no network.
- Firecracker: the Firecracker CI kernel (`vmlinux-5.10.x`) and Ubuntu 22.04
  rootfs from `spec.ccfc.min` (the upstream getting-started assets,
  https://github.com/firecracker-microvm/firecracker/blob/main/docs/getting-started.md),
  since Firecracker boots an uncompressed kernel directly rather than a disk
  image with a bootloader. Boot numbers across the Firecracker and QEMU rows
  are therefore **not** an apples-to-apples guest comparison — the
  snapshot/restore numbers are the comparable ones (RAM size is identical).

## Metrics (per iteration)

| Key | Definition |
|---|---|
| `boot_s` | process exec → `login:` prompt on the guest serial console |
| `snapshot_create_s` | live snapshot of the running VM including RAM state |
| `snapshot_restore_s` | revert the running VM to that snapshot, back to `running` |
| `resume_from_disk_s` | fresh hypervisor process restoring the snapshot at launch → `running` (the "instant resume" number) |
| `storage_overhead_bytes` | allocated bytes added by one snapshot (disk-usage delta, or snapshot file sizes) |

Timing uses a monotonic clock; boot detection polls the serial log every 10 ms.

## Running

```sh
# deps (Ubuntu): qemu-system-x86 qemu-utils virtualbox python3 bc
sudo apt-get install -y qemu-system-x86 qemu-utils virtualbox python3 bc
./run_all.sh 5          # fetch assets, run all stacks x5, write results/report.md
```

Individual stacks: `./fetch_assets.sh` then any of the per-stack scripts above.
Raw per-iteration numbers land in `results/*.json` (gitignored); the
aggregated median + min–max table in `results/report.md`.

## Caveats / honesty notes

- **vmforge numbers are a proxy.** Until the KVM backend is merged, the
  "vmforge proxy" row is QEMU/KVM driven over QMP the way the architecture
  doc specifies — it bounds what vmforge Phase 1 can achieve, it is not a
  measurement of shipped vmforge code.
- **VirtualBox needs `vboxdrv`.** On hosts running a custom kernel without
  matching headers (like the CI host this was first run on, kernel 5.15.200),
  the DKMS module cannot be built and `bench_vbox.sh` reports `blocked`
  rather than fabricating numbers. Run it on a stock-kernel Linux machine to
  fill in that row.
- Internal qcow2 snapshots (`savevm`) write the full RAM image into the
  qcow2, so storage overhead ≈ guest RAM in use; Firecracker's full snapshot
  writes RAM to a separate file (= configured RAM size). Both are reported
  under the same `storage_overhead_bytes` key.
- Results depend on host CPU, storage, and page-cache state; always compare
  numbers from the same machine (`machine` block is embedded in every
  results JSON) and report medians across ≥5 iterations.
