# CI pipeline

Two required workflows gate merges to `main` (see `docs/m1-integration-plan.md` §3):

## `ci.yml`
- **rust**: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build`, `cargo test` on the `crates/` workspace.
- **hygiene**: rejects committed build artifacts (`__pycache__/`, `*.pyc`, `*.egg-info/`, `target/`).
- A **python** job (`ruff check` + `pytest` per Python subsystem) is added when the first Python subsystem (`storage/`, `networking/`, `guest-tools/`) merges. `guest-tools/` is covered by `guest-tools.yml` (below).

## `guest-tools.yml`
- **lint + protocol unit tests**: `ruff check guest-tools` + `pytest guest-tools/tests` (protocol conformance incl. the golden transcript `guest-tools/tests/golden_transcript.jsonl`).
- **agent smoke** (`auto`/`tcg` matrix, same KVM/TCG selection as `qa-smoke.yml`): `guest-tools/tests/ga_smoke.sh` boots Alpine with the agent installed via cloud-init and exercises wait-ready, `info`/`net-info`/`interfaces`, `exec` and `shutdown --wait` with hard-stop fallback over the real virtio-serial channel.

## `qa-smoke.yml`
Boot + snapshot/restore smoke suite (`qa/smoke/smoke_test.sh`), two-job matrix on every PR and push to `main`:

- **auto** — uses **KVM** when the runner exposes `/dev/kvm` (GitHub `ubuntu-latest` runners currently do; boot-to-ready ≈ 22 s), otherwise falls back to TCG.
- **tcg** — `FORCE_TCG=1` always exercises the KVM-less TCG path (boot-to-ready ≈ 90 s), so the suite stays green on any standard runner even if hosted runners lose `/dev/kvm`.

Serial logs are uploaded as artifacts on every run.

## Full KVM smoke (M1 gate)
The M1 exit criterion is the smoke sequence green **under KVM** twice in a row from a fresh checkout of `main`:

1. Today: the `auto` job already runs KVM on hosted `ubuntu-latest` runners (`/dev/kvm` exposed). Treat a TCG fallback in the `auto` job as an infra regression, not a pass.
2. If hosted KVM becomes unreliable: move the `auto` job to a self-hosted Linux runner with `/dev/kvm` (label `kvm`), keeping the `tcg` job on hosted runners as the portable path.
3. When the core engine lands its CLI: run the same suite with `DRIVER=vmforge` (see `qa/README.md`) so the smoke gate exercises the real `vmforge ... --json` surface instead of raw QEMU.
