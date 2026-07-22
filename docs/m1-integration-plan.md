# VMForge M1 integration milestone & landing plan

**Status:** Binding for M1 · **Owner:** Arjun (VP Engineering) · **Date:** 2026-07-22
**Companion:** `docs/interface-contracts.md` (the subsystem contracts this milestone integrates).
**Inputs:** MVP PRD v1 (Doc 019f8a41-69a4-76fe-af3d-71589745cfa1), HAL spike (Doc 019f8a48-b4bb-74d3-8a8a-1671ce28677e), open PRs #1–#5.

## 1. Why now

Six v0 workstreams are in flight on parallel branches (core engine, networking,
storage, guest tools, GUI alpha, QA). Today `main` contains only a README; the
branches have already drifted (two competing Rust layouts: `core/` on PR #3 vs
the `crates/` workspace on PR #1; a stale guest-agent socket path; `.pyc`
files committed). M1 exists to force integration **early** on the smallest
end-to-end slice, with `main` as the single integration point.

## 2. M1 definition — the smallest end-to-end slice

**On a Linux/KVM host, entirely via the `vmforge` CLI:**

```
vmforge create m1vm --cpus 2 --memory 2048 --image alpine-base --disk-size 8G --forward tcp:2222:22
vmforge boot m1vm                       # → RUNNING; guest agent connects
vmforge status m1vm --json              # guest_agent: "connected"; ssh -p 2222 works
vmforge stop m1vm                       # graceful via guest agent
vmforge snapshot m1vm s1                # offline disk snapshot (frozen qcow2 layer)
vmforge boot m1vm && <mutate guest fs> && vmforge stop m1vm
vmforge restore m1vm s1                 # revert overlay to s1, boot
# → guest filesystem shows pre-mutation state; snapshot tree shows s1 with a branch
vmforge delete m1vm
```

**Scope decisions (binding):**

- **Snapshots are offline (disk-only) for M1.** Live disk+RAM snapshots
  (QMP `snapshot-save`, the instant-resume USP) are M2 — they need the
  engine/storage seam proven first. "restore" in M1 = revert overlay + boot.
- Networking = user-mode NAT + static/dynamic hostfwd only.
- Guest = a stock Alpine/Ubuntu cloud image with the guest agent installed via
  the QA harness image-bake script; no installer flow.
- GUI in M1 is read-mostly: list VMs, show status via `--json` polling, VNC
  console viewer, boot/stop buttons. Snapshot tree UI may land after M1.
- Host = Linux/KVM only. macOS/hvf backend stub compiles but is untested.

**M1 is done when:** the QA smoke suite (§4) runs the sequence above green in
CI on a KVM-enabled Linux runner, from a fresh checkout of `main`, twice in a
row (no flake), and the GUI alpha can boot/stop and view the console of the
same VM on a dev machine.

### Per-subsystem exit criteria

| Subsystem | M1 exit criteria |
|---|---|
| **Core engine** | `vmforge` CLI implements the §4-GUI command surface of the contracts doc with `--json` on all commands; FSM enforced (`InvalidState` on bad transitions); QEMU launched with storage attach path, networking argv, guest-agent argv exactly per contract; advisory per-VM lock; VNC display wired; contract-version 1. |
| **Storage** | `vmforge-storage --json` for import/create/snapshot/revert/tree/delete matching `StorageProvider`; layout & invariants S1–S4; name regex enforced; error JSON per contract; no `.pyc` in tree; engine integration verified by the smoke suite. |
| **Networking** | NAT argv generation conformance vectors published (`networking/tests/test_natgen.py`) and matched by the engine's Rust impl; dynamic hostfwd add/remove over the engine's QMP connection demonstrated in the smoke suite (N1–N3). |
| **Guest tools** | Agent autostarts in the M1 guest image; implements `ping`/`info`/`interfaces`/`shutdown` per wire protocol (G1–G3); per-VM socket path adopted (drop `/tmp/vmforge-ga.sock`); `vmforgectl.py` kept green as reference client. |
| **GUI alpha** | Lists VMs and live status by polling `vmforge ... --json` only (no direct file/socket access); boot/stop actions; embedded VNC console connects to `status.display.port`; renders `guest_agent` connectivity. |
| **QA** | Boot/snapshot smoke suite implemented per §4; runs in CI on KVM runner; image-bake script produces the agent-equipped guest image; suite is the merge gate for `main`. |

## 3. Repo mechanics

### Layout (post-unification)

```
crates/vmforge-core        # types, FSM, traits (Hypervisor, StorageProvider, NetworkBackend, GuestAgent)
crates/vmforge-backend-kvm # QEMU+QMP backend (absorbs core/ engine v0 from PR #3)
crates/vmforge-backend-hvf # stub
crates/vmforge-cli         # `vmforge`
storage/                   # Python vmforge-storage (M1 authoritative storage impl)
networking/                # Python vmforge_net (reference impl + conformance vectors)
guest-tools/               # guest agent + reference host client
gui/                       # GUI alpha
qa/                        # smoke suite + image bake
docs/                      # architecture.md, interface-contracts.md, this file
.github/workflows/ci.yml   # single top-level pipeline (see gates)
```

### Branch & PR conventions

- `main` is the only long-lived branch; it must always pass CI. Feature
  branches: `<owner>/<topic>` (current `devin/<ts>-<topic>` is fine), short-lived
  (< 1 week), rebased on `main` before merge; squash-merge.
- PR title: `<area>: <imperative summary>` (`core:`, `storage:`, `net:`,
  `guest:`, `gui:`, `qa:`, `docs:`). One subsystem per PR.
- **Contract changes:** any PR touching `docs/interface-contracts.md` or a
  trait in `vmforge-core` requires review from the owners of every affected
  subsystem (CODEOWNERS enforces).
- No generated artifacts in-tree (`.pyc`, `__pycache__`, target/) — enforced
  by `.gitignore` + CI check.

### CI gates (required checks before merge to `main`)

1. **rust**: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`
   (workspace).
2. **python** (per Python subsystem): `ruff check`, `pytest`.
3. **conformance**: engine's Rust NAT argv vs `networking` test vectors;
   guest-agent protocol golden tests (recorded request/response transcripts).
4. **hygiene**: no committed bytecode/build artifacts; docs build.
5. **smoke (KVM)**: QA boot/snapshot smoke suite on a KVM-enabled runner
   (nested virt or self-hosted). Until such a runner exists, the smoke job
   runs QEMU with TCG (`--accel tcg`) — slower but exercises the full stack;
   flip to KVM when the runner lands. Required for merges to `main` once QA
   v0 lands.

### Integration test harness sketch (qa/)

```
qa/
├── images/bake.sh          # cloud image + guest agent + sshd → images/m1-guest.qcow2
├── smoke/test_m1_slice.py  # pytest: drives `vmforge ... --json` as a subprocess
│     # create→boot→agent ping→ssh via hostfwd→stop→snapshot→mutate→restore→assert→delete
│     # asserts on JSON schemas from the contracts doc (schema fixtures in qa/schemas/)
└── conformance/            # cross-language golden tests (natgen argv, guest-agent transcripts)
```

Harness principles: black-box through the CLI JSON surface only (same seam the
GUI uses — testing the contract, not internals); each test gets a temp
`$VMFORGE_HOME`; hard 10-min timeout; serial.log + QMP transcript uploaded as
CI artifacts on failure.

## 4. Sequencing plan for the six v0 branches

Order matters because PR #1 and PR #3 conflict structurally, and everything
else depends on the engine seam.

1. **PR #0 (this one) — docs/contracts.** Merge first; it is the referee.
2. **PR #1 — hypervisor scaffold (`crates/` workspace).** Merge second: it
   establishes the canonical Rust layout, traits, and CI skeleton. Before
   merge: add `StorageProvider`/`NetworkBackend`/`GuestAgent` trait stubs per
   the contracts doc.
3. **PR #3 — core engine v0.** Rebase onto the workspace: move `core/src/*`
   into `crates/vmforge-backend-kvm` (QEMU/QMP/launch) and reconcile its
   `VmConfig`/error types with `vmforge-core`. This is the one heavy rebase;
   engine owner + me pair on it. Adopt the guest-agent argv + per-VM socket
   path and storage attach path while rebasing.
4. **PR #5 — storage v0.** Independent tree (`storage/`); merge after #3 with
   two contract fixes: `--json` on all CLI commands, `--contract-version`,
   drop committed `.pyc`. Engine switches disk creation from its internal
   `snapshot::create_disk` to `SubprocessStore` in a follow-up PR.
5. **PR #2 — networking v0.** Independent tree; merge after #5 (any order vs
   #5 is fine). Publish argv conformance vectors; engine implements Rust
   `qemu_args` against them in a follow-up.
6. **PR #4 — guest tools v0.** Merge after path fix (`/tmp` → per-VM socket)
   and `.pyc` removal; QA's bake script then installs the agent.
7. **GUI alpha skeleton (not yet pushed).** Lands any time after #3 — it only
   needs the CLI JSON surface; use recorded `status --json` fixtures until the
   engine is merged.
8. **QA v0 (not yet pushed).** Conformance + hygiene jobs can land right after
   #1; the KVM/TCG smoke job lands last and becomes the required M1 gate.

Post-M1 (M2 preview, not binding): live disk+RAM snapshots/instant-resume via
QMP `snapshot-save`; JSON-RPC daemon + event stream replacing CLI polling;
port storage to Rust behind `StorageProvider`; TAP/bridged networking per the
networking v1 design.
