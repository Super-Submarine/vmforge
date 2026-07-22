# VMForge MVP Test Plan (QA v0)

Owner: Vera (QA). Scope: the Q4 MVP OKR — *create, boot, snapshot a Linux VM on 2 host platforms*.

VMForge drives QEMU as an out-of-process engine over QMP, with the native accelerator
per host OS (KVM on Linux, Hypervisor.framework/hvf on macOS, WHP on Windows), virtio
devices and qcow2 disks. This plan targets the engine-level behaviors first (exercised
today via plain `qemu-system-x86_64`), and is structured so the same matrix re-runs
against the VMForge CLI/daemon as teammates land core/networking/storage code (see
`qa/README.md` § Driver interface).

## 1. Test matrix — happy path

Operations under test: **create → boot → (snapshot / restore) → stop**.

| # | Guest image | Accel | create | boot to ready¹ | snapshot (running) | restore | stop (graceful) |
|---|-------------|-------|--------|-----------------|--------------------|---------|------------------|
| M1 | Alpine (nocloud cloud image) | KVM | ✔ | ✔ | ✔ | ✔ | ✔ |
| M2 | Alpine (nocloud cloud image) | TCG | ✔ | ✔ | ✔ | ✔ | ✔ |
| M3 | Debian (genericcloud) | KVM | ✔ | ✔ | ✔ | ✔ | ✔ |
| M4 | Debian (genericcloud) | TCG | ✔ | ✔ | ✔ | ✔ | ✔ |
| M5 | Ubuntu (cloud image, current LTS) | KVM | ✔ | ✔ | ✔ | ✔ | ✔ |
| M6 | Ubuntu (cloud image, current LTS) | TCG | ✔ | ✔ | ✔ | ✔ | ✔ |

¹ "ready" = serial console shows a login prompt **or** the cloud-init completion marker
(`VMFORGE_CLOUD_INIT_DONE`, emitted by our seed's `runcmd`), whichever comes first.

Automated today: **M1/M2** (Alpine, both accelerators — M2 always in CI, M1 when
`/dev/kvm` exists) via `qa/smoke/smoke_test.sh`. M3–M6 run the same script with
`GUEST_IMAGE_URL` overridden (documented in `qa/README.md`); they are nightly
candidates, not per-PR, because of image size (Debian ~350 MB, Ubuntu ~650 MB) and TCG
boot time.

Host platform axis (per the OKR: 2 host platforms): Linux/KVM is covered in CI;
macOS/hvf runs the same suite with `-accel hvf` on a macOS runner or dev laptop
(tracked as a follow-up — GitHub's macOS runners support hvf). Windows/WHP is
out of scope for MVP QA v0.

### Snapshot sub-matrix

| Case | Method | Expectation |
|------|--------|-------------|
| S1 | Internal snapshot of **running** VM (`savevm` via QMP `human-monitor-command`) | Snapshot listed by `info snapshots` and `qemu-img snapshot -l`; guest keeps running |
| S2 | Restore running VM to snapshot (`loadvm`) | VM returns to `running` state; serial console responsive; guest state matches snapshot point |
| S3 | Snapshot of **stopped** VM (`qemu-img snapshot -c` on the qcow2, VM powered off) | Snapshot listed; image passes `qemu-img check` |
| S4 | Restore stopped VM (`qemu-img snapshot -a`) then boot | Boots to ready from the snapshotted state |
| S5 | Multiple snapshots + restore to a non-latest one | Correct snapshot chosen; others still listed (feeds the git-like-snapshots USP later) |

Automated today: S1, S2, and the `qemu-img snapshot -l` verification of S3 (post-shutdown
listing) in the smoke suite.

## 2. Failure / negative cases

| Case | Setup | Expected behavior |
|------|-------|-------------------|
| F1 Corrupt disk | Truncate/overwrite bytes in the qcow2 header, then boot | QEMU refuses to start with a clear error (exit ≠ 0); no hang; `qemu-img check` reports corruption |
| F2 Missing ISO/image | Point `-drive`/`-cdrom` at a nonexistent path | Immediate startup failure with actionable error; no orphan process |
| F3 `kill -9` of QEMU | SIGKILL the QEMU PID mid-boot and mid-snapshot | No leftover lock prevents relaunch; qcow2 still passes `qemu-img check` (or check reports repairable leaks only); monitor/QMP sockets cleaned up by harness |
| F4 Snapshot of running vs stopped VM | S1 vs S3 above | Both succeed; internal snapshot of a running VM includes RAM state, offline one does not |
| F5 savevm with incompatible device/backing config | e.g. raw disk attached | `savevm` fails with a clear QMP error, guest unaffected |
| F6 loadvm nonexistent snapshot | `loadvm bogus` | Clear error; VM state unchanged |
| F7 Double-start on same disk (lock) | Boot two QEMUs on one qcow2 without `-snapshot` | Second instance fails with image-lock error |
| F8 Out-of-space during snapshot | Small tmpfs backing dir | Graceful failure, image not corrupted |

Automated today: F2, F3 (mid-boot variant + relaunch), F6, and F1 (header corruption)
in `qa/smoke/smoke_test.sh --negative`. F7 (double-boot/lock) and F8 (out-of-space
during savevm) plus crash-mid-run, truncated-snapshot, invalid-config and
deleted-branch-restore cases are automated in `qa/failure/failure_suite.sh`
(QA v2, nightly — see `docs/ci.md` § qa-extended). F5 stays manual until the
VMForge CLI gives us stable error codes to assert on.

## 3. CI budget & flakiness (initial observations)

- CI uses TCG when `/dev/kvm` is absent; **GitHub `ubuntu-latest` runners expose
  `/dev/kvm`**, so the per-PR job usually runs KVM and falls back to TCG only on
  constrained runners. The workflow runs one KVM-or-TCG smoke plus a forced-TCG smoke
  so the fallback path is always exercised.
- Expected runtimes (Alpine, 1 vCPU, 512 MB): KVM boot-to-ready ~10–25 s; TCG
  boot-to-ready ~60–180 s. Full smoke incl. snapshot/restore & negative cases:
  ~2–4 min (KVM) / ~5–8 min (TCG). Timeouts in the suite are 300 s (boot) and 120 s
  (QMP ops) — flag any timeout bumps in PR review, they usually hide a real hang.
- Known flaky areas to watch: (a) image mirror download (use the cache + retry; the
  workflow caches the image by URL), (b) TCG boot time variance under noisy runners,
  (c) `savevm` duration scales with guest RAM — keep smoke guests at 512 MB.

## 4. How this plan evolves with the product

Once core lands a VMForge CLI (`vmforge create/start/snapshot/restore/stop`), swap the
driver (see `qa/README.md`) and re-run the same matrix — the assertions stay identical.
Networking and storage teams add rows (virtio-net reachability, virtio-blk/scsi,
backing-file chains) to §1 rather than new suites.
