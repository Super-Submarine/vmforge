# VMForge wedge-claim benchmark results

| Stack | Cold boot (s) | Snapshot create, live w/ RAM (s) | Snapshot restore / revert (s) | Instant resume from disk (s) | Snapshot storage overhead (MiB) |
|---|---|---|---|---|---|
| Firecracker (KVM microVM, snapshot/restore API) | 1.396 (min 1.363 / max 1.407, n=5) | 0.576 (min 0.574 / max 0.579, n=5) | 0.005 (min 0.005 / max 0.025, n=5) | 0.005 (min 0.005 / max 0.025, n=5) | 512.015 (min 512.015 / max 512.015, n=5) |
| vmforge-proxy (QEMU/KVM via QMP snapshot jobs) | 19.121 (min 18.934 / max 19.220, n=5) | 0.391 (min 0.203 / max 0.657, n=5) | 0.199 (min 0.127 / max 0.255, n=5) | 0.161 (min 0.154 / max 0.164, n=5) | 129.328 (min 128.328 / max 129.641, n=5) |
| raw QEMU/KVM (HMP savevm/loadvm + qemu-img) | 19.109 (min 19.040 / max 19.406, n=5) | 0.349 (min 0.208 / max 0.456, n=5) | 0.122 (min 0.118 / max 0.160, n=5) | 0.167 (min 0.163 / max 0.174, n=5) | 129.453 (min 129.328 / max 129.578, n=5) |
| VirtualBox | BLOCKED: vboxdrv kernel module not loaded (no matching kernel headers on this host) | — | — | — | — |

## Machine

- **hostname**: devin-box
- **kernel**: 5.15.200
- **os**: Ubuntu 22.04.5 LTS
- **cpu_model**: INTEL(R) XEON(R) PLATINUM 8559C
- **cpus**: 8
- **mem_total_kb**: 32881112
- **kvm_available**: True
- **qemu_version**: QEMU emulator version 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31)
- **qemu_img_version**: qemu-img version 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31)
- **virtualbox_version**: 6.1.50_Ubuntur161033 (vboxdrv module not loadable on this host)
- **firecracker_version**: Firecracker v1.10.1
- **disk**: /dev/root      ext4  122G
- **timestamp_utc**: 2026-07-22T15:56:38Z
