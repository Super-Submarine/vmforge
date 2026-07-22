# Engine performance benchmarks (v1.5)

Reproducible benchmarks for the engine's resume/boot hot paths — the numbers
behind the instant-resume / git-like-branching wedge (see the wedge validation
harness in `bench/`, PR #25). Where the `bench/` suite compares VMForge's
architecture against other stacks with real guest OSes, this harness measures
the **engine's own hot-path latencies** through the `Hypervisor` trait, exactly
the way product code drives it, so regressions are caught as features accrete.

## Harness

`crates/vmforge-bench` (binary `vmforge-bench`) runs the full lifecycle
n times (default 5, plus one untimed warm-up) against the QEMU engine
(`vmforge-engine-qemu` via `vmforge-backend-hvf`, the backend with the full
Phase-1 lifecycle) and reports **medians and nearest-rank p95** as
machine-readable JSON plus a markdown summary — the same methodology
(n>=5, medians + p95, monotonic clock) as the wedge validation run.

| Metric | Definition |
|---|---|
| `boot_ready_ms` | cold boot: QEMU process spawn + QMP handshake + `cont` → run state `running` |
| `snapshot_save_ms` | live snapshot: pause window + external qcow2 overlay (`blockdev-snapshot-sync`) + RAM/device state to file (`migrate`) + resume |
| `restore_resume_ms` | instant resume: fresh QEMU with `-incoming defer` + fresh overlay + `migrate-incoming` state load + `cont` → `running` |
| `branch_switch_ms` | restore to a *different* snapshot DAG node (fresh overlays on that node's frozen layers) after branching |

Guest is a 256 MiB (configurable) aarch64 `virt` machine with UEFI firmware
and one 64 MiB qcow2 disk; "ready" is engine-ready (run state `running`), not
guest-OS login — guest-OS-level boot numbers live in `bench/` and the QA smoke.

```sh
cargo build --release -p vmforge-bench
VMFORGE_BENCH_ACCEL=tcg ./target/release/vmforge-bench \
  --iterations 5 --json results.json --markdown results.md \
  --baseline bench/engine/baseline.json --threshold 25
```

Accelerator: `VMFORGE_BENCH_ACCEL=kvm|hvf|tcg`, defaulting to KVM on aarch64
Linux hosts with `/dev/kvm`, HVF on Apple Silicon, else TCG. CI pins TCG —
nested-virt friendly (no `/dev/kvm` required) and runnable on any runner,
matching the wedge-run methodology.

## CI regression guard

`.github/workflows/engine-bench.yml` runs nightly (alongside the QA smoke),
on `workflow_dispatch`, and on PRs/pushes touching `crates/**`. It executes
the harness under TCG with n=5 and **fails if any metric's median regresses
by more than 25%** against the committed baseline
[`bench/engine/baseline.json`](../bench/engine/baseline.json). Results are
uploaded as the `engine-bench-results` artifact and shown in the job summary.

To refresh the baseline after an intentional change (or runner upgrade),
download `bench-results.json` from the artifact of a green run on `main`
and commit it as `bench/engine/baseline.json`.

## v1.5 resume-path optimizations (before/after)

Measured with the harness above (TCG, 256 MiB guest, n=5, medians;
QEMU 6.2 host — the `file:` URI change additionally applies on QEMU >= 8.2
hosts such as CI's Ubuntu 24.04):

| Metric | Before (ms) | After (ms) | Δ |
|---|---:|---:|---:|
| boot_ready_ms | 59.32 | 57.78 | −2.6% |
| snapshot_save_ms | 108.78 | 41.70 | **−61.7%** |
| restore_resume_ms | 158.76 | 61.98 | **−61.0%** |
| branch_switch_ms | 159.89 | 62.94 | **−60.6%** |

Changes landed in `vmforge-engine-qemu`:

1. **Adaptive QMP completion polling.** `restore_incoming` and
   `wait_migration` polled run/migration state at a fixed 100 ms, so every
   snapshot save and every resume paid up to ~100 ms of pure quantization
   latency per wait. Polling now starts at 1 ms and backs off exponentially
   (cap 20 ms for state load, 50 ms for migration), keeping short operations
   short without hammering QMP on long ones.
2. **Faster QMP socket acquisition.** The connect retry loop after QEMU spawn
   slept 50 ms per attempt; now 5 ms, shaving startup latency off boot *and*
   resume (both spawn a fresh QEMU process).
3. **`file:` migration URI on QEMU >= 8.2** (detected from the QMP greeting
   version). RAM state save/load previously piped through `exec:cat`,
   spawning a subprocess and copying every page through a pipe; `file:` lets
   QEMU read/write the state file directly
   (https://www.qemu.org/docs/master/devel/migration/main.html).
4. **Streaming snapshot-ID hashing.** The content-addressed `SnapshotId`
   hashed the state file via `fs::read`, loading the entire guest-RAM-sized
   file into memory on every save; it now streams in 1 MiB chunks.

The committed baseline reflects the post-optimization numbers.
