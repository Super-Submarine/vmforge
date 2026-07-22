# QA v2 — beta-readiness coverage summary

Scope: move QA from happy-path (v0 smoke) to beta-grade robustness ahead of beta
wave 1 (10 external testers, M2). Companion to `qa/TEST_PLAN.md` and `docs/ci.md`.

## What is covered now

### 1. Cross-backend test matrix
The smoke + negative suites run against multiple backends via a single `BACKEND`
parameter (`qa/README.md` § Cross-backend matrix):

| Backend | What it exercises | Where it runs |
|---------|-------------------|---------------|
| `auto`/`kvm` (x86_64) | The Linux KVM backend path | per-PR (`qa-smoke.yml`) + nightly |
| `tcg` (x86_64) | KVM-less fallback | per-PR + nightly |
| `tcg-aarch64` | TCG-emulated aarch64 (`-machine virt` + UEFI) — the CI stand-in for the macOS HVF/ARM backend | nightly (`qa-extended.yml`) |

New backends (e.g. a real HVF runner, `DRIVER=vmforge` once the CLI lands) slot in
as a driver/matrix row without touching assertions.

### 2. Negative / failure-mode suite (`qa/failure/failure_suite.sh`)
Each case asserts a clean error, no orphaned QEMU processes, and no corrupted
state dir (`qemu-img check`):

- X1 VM process crash mid-run (SIGKILL) → cleanup + relaunch on the same disk
- X2 disk-full during snapshot (savevm on a 64 MB tmpfs) → clean error, VM keeps running
- X3 corrupt/truncated qcow2 snapshot file → refused at open or on loadvm
- X4 invalid VM config (negative RAM, bogus machine type) → fast startup error
- X5 double-boot of the same VM disk → second boot refused by the qcow2 write lock
- X6 snapshot-restore of a deleted branch (delvm → loadvm) → clean error; surviving branch restorable

### 3. Nightly extended CI (`.github/workflows/qa-extended.yml`)
Schedule-triggered (02:30 UTC) + manual dispatch; full backend matrix × (smoke +
negative + failure suite) + integration job. Deliberately out of the per-PR fast
path (`ci.yml` + `qa-smoke.yml` stay the merge gate).

### 4. Subsystem integration tests (`qa/integration/`)
- `net_hostfwd_test.sh` — boots a guest with NIC args from `vmforge-net args`
  including a hostfwd rule and asserts host-port reachability (SSH banner).
- `guest_agent_test.sh` — runs the guest-tools e2e smoke (agent ping/info/exec/shutdown
  over virtio-serial).

Both **skip with a reason** until networking (PR #2) and guest-tools (PR #4) merge,
then activate automatically on the next nightly run.

## Known gaps before beta wave 1

1. **No real HVF coverage** — `tcg-aarch64` only proves the ARM guest path, not
   Hypervisor.framework. Needs a macOS runner (hosted macOS runners support HVF)
   running the same suite with `-accel hvf`.
2. **QA v1 (snapshot-tree regression + PRD F1–F5 acceptance) is not on `main`** —
   referenced by the QA roadmap but not merged; the cross-backend matrix currently
   parameterizes smoke + negative suites only. Re-point the matrix at those suites
   when they land.
3. **Suites still drive raw QEMU, not the product** — engine (PR #3), storage (PR #5)
   CLI surfaces should be exercised via `DRIVER=vmforge` once merged, so beta bugs
   surface in VMForge code paths, not just QEMU.
4. **Integration tests are gated but unproven in CI** — hostfwd/guest-agent tests
   activate only after PRs #2/#4 merge; their first nightly run needs watching.
5. **Guest-image diversity** — Alpine only in automation; Debian/Ubuntu rows (M3–M6)
   of the matrix remain manual/nightly candidates.
6. **Windows/WHP host path** — out of scope for MVP QA, still untested.
7. **Restore-under-load, instant-resume latency targets** — no perf/soak coverage yet;
   beta USP claims (instant resume) are unmeasured.
