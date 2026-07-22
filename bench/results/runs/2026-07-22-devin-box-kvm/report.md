# VMForge wedge-claim benchmark results

| Stack | Cold boot (s) | Snapshot create, live w/ RAM (s) | Snapshot restore / revert (s) | Instant resume from disk (s) | Snapshot storage overhead (MiB) |
|---|---|---|---|---|---|
| Firecracker (KVM microVM, snapshot/restore API) | 2.100 (min 2.029 / max 2.129, n=5) | 0.649 (min 0.644 / max 0.695, n=5) | 0.023 (min 0.023 / max 0.023, n=5) | 0.023 (min 0.023 / max 0.023, n=5) | 512.015 (min 512.015 / max 512.015, n=5) |
| vmforge-proxy (QEMU/KVM via QMP snapshot jobs) | 20.048 (min 20.005 / max 20.377, n=5) | 0.226 (min 0.213 / max 0.249, n=5) | 0.123 (min 0.120 / max 0.132, n=5) | 0.162 (min 0.154 / max 0.170, n=5) | 129.203 (min 128.699 / max 129.703, n=5) |
| raw QEMU/KVM (HMP savevm/loadvm + qemu-img) | 20.015 (min 19.986 / max 20.333, n=5) | 0.219 (min 0.216 / max 0.229, n=5) | 0.124 (min 0.117 / max 0.129, n=5) | 0.163 (min 0.155 / max 0.181, n=5) | 128.203 (min 127.887 / max 129.703, n=5) |
| VirtualBox | BLOCKED: VBoxManage not installed | — | — | — | — |

## Machine

- **hostname**: devin-box
- **kernel**: 5.15.200
- **os**: Ubuntu 22.04.5 LTS
- **cpu_model**: Intel(R) Xeon(R) Platinum 8375C CPU @ 2.90GHz
- **cpus**: 8
- **mem_total_kb**: 32881112
- **kvm_available**: True
- **qemu_version**: QEMU emulator version 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31)
- **qemu_img_version**: qemu-img version 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31)
- **virtualbox_version**: not available
- **firecracker_version**: Firecracker v1.10.1
- **disk**: /dev/root      ext4  122G
- **timestamp_utc**: 2026-07-22T16:33:20Z
