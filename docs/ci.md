# CI pipeline

Two required workflows gate merges to `main` (see `docs/m1-integration-plan.md` §3):

## `ci.yml`
- **rust**: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build`, `cargo test` on the `crates/` workspace.
- **hygiene**: rejects committed build artifacts (`__pycache__/`, `*.pyc`, `*.egg-info/`, `target/`).
- A **python** job (`ruff check` + `pytest` per Python subsystem) is added when the first Python subsystem (`storage/`, `networking/`, `guest-tools/`) merges.

## `qa-smoke.yml`
Boot + snapshot/restore smoke suite (`qa/smoke/smoke_test.sh`), two-job matrix on every PR and push to `main`:

- **auto** — uses **KVM** when the runner exposes `/dev/kvm` (GitHub `ubuntu-latest` runners currently do; boot-to-ready ≈ 22 s), otherwise falls back to TCG.
- **tcg** — `FORCE_TCG=1` always exercises the KVM-less TCG path (boot-to-ready ≈ 90 s), so the suite stays green on any standard runner even if hosted runners lose `/dev/kvm`.

Serial logs are uploaded as artifacts on every run.

## `qa-extended.yml` (nightly, not per-PR)
QA v2 beta-readiness suite, scheduled nightly (02:30 UTC) + `workflow_dispatch`. Kept
out of the per-PR fast path on purpose (slow TCG/aarch64 boots, long failure cases):

- **matrix-smoke** — the smoke suite across the full backend matrix: `auto` (KVM),
  `tcg`, and `tcg-aarch64` (TCG-emulated ARM, the CI stand-in for the HVF backend;
  see `qa/README.md` § Cross-backend matrix). New backends are added as matrix rows.
- **failure-suite** — `qa/failure/failure_suite.sh` (crash mid-run, disk-full during
  snapshot, corrupt/truncated snapshot, invalid config, double-boot, deleted-branch
  restore) on the same backend matrix; every case asserts clean errors, no orphaned
  QEMU processes, and an uncorrupted state dir.
- **integration** — `qa/integration/net_hostfwd_test.sh` (hostfwd reachability via
  `vmforge-net`) and `qa/integration/guest_agent_test.sh` (agent ping/exec); both
  skip with a reason until PRs #2/#4 merge, then activate automatically.

## Full KVM smoke (M1 gate)
The M1 exit criterion is the smoke sequence green **under KVM** twice in a row from a fresh checkout of `main`:

1. Today: the `auto` job already runs KVM on hosted `ubuntu-latest` runners (`/dev/kvm` exposed). Treat a TCG fallback in the `auto` job as an infra regression, not a pass.
2. If hosted KVM becomes unreliable: move the `auto` job to a self-hosted Linux runner with `/dev/kvm` (label `kvm`), keeping the `tcg` job on hosted runners as the portable path.
3. When the core engine lands its CLI: run the same suite with `DRIVER=vmforge` (see `qa/README.md`) so the smoke gate exercises the real `vmforge ... --json` surface instead of raw QEMU.
