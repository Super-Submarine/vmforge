# Wedge-claim validation run — 2026-07-22, Linux/KVM host (nested KVM)

Raw results for the wedge-claim validation report (contract 019f8a93-26bc).
This directory is a committed snapshot of one full harness run; the live
`bench/results/*.json` outputs stay gitignored.

## Host

- Ubuntu 22.04.5 LTS, kernel 5.15.200, Intel Xeon Platinum 8375C (8 vCPU), 32 GiB RAM, ext4 on /dev/root
- **Acceleration: nested KVM** (`/dev/kvm` present and used via `-accel kvm`; not TCG).
  Validity limit: nested virtualization adds VM-exit overhead vs bare metal, so
  absolute timings are conservative upper bounds; cross-stack ratios on the same
  host remain valid.
- QEMU 6.2.0 (Ubuntu), Firecracker v1.10.1
- VirtualBox: **BLOCKED** on this host (no `vboxdrv` — custom kernel 5.15.200
  without matching headers, see `bench/README.md` caveats). Its row uses the
  published/user-measured baseline numbers instead (doc 019f8a88-79f0).

## Guest

- QEMU stacks: Alpine Linux 3.22 nocloud qcow2 (x86_64 BIOS), 512 MiB RAM, 1 vCPU, no net
- Firecracker: CI vmlinux-5.10.x + Ubuntu 22.04 ext4 rootfs, 512 MiB RAM

## Reproduce

```sh
sudo apt-get install -y qemu-system-x86 qemu-utils python3 bc   # + virtualbox if the kernel supports vboxdrv
cd bench
./run_all.sh 5     # fetch assets, run all stacks x5, write results/report.md
```

n=5 iterations per stack per metric; `report.md` aggregates median (min–max).
Files:

- `qemu-proxy.json` — vmforge proxy: QEMU/KVM driven over QMP snapshot-save/-load jobs (the Phase-1 architecture)
- `qemu-raw.json` — raw QEMU/KVM HMP savevm/loadvm baseline
- `firecracker.json` — Firecracker snapshot/restore reference (instant-resume ceiling)
- `virtualbox.json` — machine-readable `blocked` marker for this host
- `report.md` — aggregated median/min/max table + machine block

## Known caveats of this run

- **vmforge numbers are a proxy**: the real KVM backend is not merged; QEMU/KVM
  over QMP is benchmarked exactly the way `docs/architecture.md` specifies.
- Cold-boot for the Alpine guest clusters tightly around ~20 s because boot is
  dominated by the guest's fixed init/getty sequence, not hypervisor latency.
- Firecracker boot numbers are not apples-to-apples with the QEMU rows
  (direct-kernel boot, different guest); snapshot/restore numbers are comparable
  (identical RAM size).
