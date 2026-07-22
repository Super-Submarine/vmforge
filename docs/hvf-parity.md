# KVM vs HVF backend parity matrix (MVP surface)

**Scope:** the `Hypervisor` trait MVP surface — create/boot/pause/resume/stop
lifecycle plus snapshot save/restore/branch — per `docs/interface-contracts.md`
and the MVP PRD. Both backends drive QEMU over QMP through the shared
`vmforge-engine-qemu` crate (Phase 1); only `-accel`/`-cpu` differ.

Legend: **✔ impl** = implemented behind the trait · **TCG-verified** =
exercised end-to-end in Linux CI via `-accel tcg` (identical invocation and
QMP path) · **needs macOS HW** = requires real Apple Silicon + Hypervisor.framework
to verify.

| Capability | KVM (Linux) | HVF (macOS) | HVF verification status |
|---|---|---|---|
| `create` (define + invocation validation) | trait stub (engine lives in core PR #3) | ✔ impl | TCG-verified (CI `tcg-integration`) |
| `boot` (spawn, UEFI, QMP handshake, `cont`) | trait stub | ✔ impl | TCG-verified |
| `pause` / `resume` (QMP stop/cont) | trait stub | ✔ impl | TCG-verified |
| `stop` (QMP quit, graceful teardown) | trait stub | ✔ impl | TCG-verified |
| `snapshot` save (external qcow2 overlay + RAM state via migrate-to-file) | trait stub | ✔ impl (pause window; no userfaultfd/background-snapshot on macOS) | TCG-verified |
| `restore` (instant resume: `-incoming defer` + `migrate-incoming`) | trait stub | ✔ impl | TCG-verified |
| snapshot **branch** (re-restore a node, snapshot with `parent`) | trait stub | ✔ impl | TCG-verified |
| `delete` / `state` / FSM validation | trait stub | ✔ impl | unit + TCG-verified |
| `capabilities` reporting | ✔ impl | ✔ impl | unit-tested |
| `-accel hvf` init, `-cpu host`, HVF vGIC, vtimer fidelity across restore | n/a | ✔ impl (flags emitted) | **needs macOS HW** (planned self-hosted M-series runner, port plan §4) |
| Hypervisor entitlement / signing (`com.apple.security.hypervisor`) | n/a | not in scope (see `docs/macos-packaging-todo.md`) | needs macOS HW |
| virtio-gpu 3D (Venus/MoltenVK) | ✔ capability advertised | not yet (pending MoltenVK validation) | needs macOS HW |

Note: the KVM backend crate on `main` is still the scaffold stub; the working
Linux/KVM engine is in core engine PR #3 and converges on the same
`vmforge-engine-qemu` path at M1 integration. HVF is the first backend fully
implemented behind the trait.

## What CI covers vs. what needs real hardware

- **CI (`tcg-integration` job):** full lifecycle + snapshot + restore + branch
  through the hvf backend code path under `-accel tcg` on ubuntu-latest, and a
  `cross-check-macos` job compiling the workspace for `aarch64-apple-darwin`.
- **Needs real HVF hardware:** accelerator init and `-cpu host` behavior,
  HVF vGIC interrupt delivery, generic timer (vtimer) consistency across
  pause/snapshot/restore, wall-clock behavior after instant resume, and
  performance (pause-window length, resume latency). Tracked for the
  self-hosted macOS runner (M2).
