# VMForge Release Notes

## v0.1 — wave-1 Linux beta (draft)

**Audience:** wave-1 beta testers. **Platform:** Linux x86_64 with KVM only
(macOS/Apple Silicon joins wave 2, gated on HVF snapshot parity).
**CLI surface:** frozen for the duration of wave 1 — see
[`docs/cli-freeze-v1.0-beta.md`](cli-freeze-v1.0-beta.md) (PR
[#16](https://github.com/Super-Submarine/vmforge/pull/16)); anything marked
*experimental* there may change without notice.

### What's in the wave-1 build

| Feature | What you get | Source |
|---|---|---|
| Backend probe: `vmforge info` | Detects KVM and prints backend capabilities (accelerator, guest archs, live-snapshot & virtio-gpu 3D support). Exit codes 0/1/2 are frozen. | PR [#1](https://github.com/Super-Submarine/vmforge/pull/1) (hypervisor scaffold), freeze doc §1 |
| Storage CLI: `vmforge-storage` | qcow2 disk management (create/resize/import/clone/delete/info/check) and git-like **offline snapshot trees** (create/list/revert/delete, `tree` alias), with `--json` output and frozen exit codes 0/1/2/3. | PR [#5](https://github.com/Super-Submarine/vmforge/pull/5) (Storage v1), freeze doc §2 |
| Golden-path smoke suite | `qa/smoke/smoke_test.sh` — create → boot → live snapshot → restore → shutdown against a known-good Alpine image, plus `--negative` failure cases. This is the supported way to drive VMs until the engine lifecycle verbs merge. | PR [#8](https://github.com/Super-Submarine/vmforge/pull/8) (QA v0), freeze doc §3 |
| CI on every PR | fmt + clippy + build + test + smoke + hygiene gate + CLI-freeze guard. | PR [#9](https://github.com/Super-Submarine/vmforge/pull/9) (baseline CI), PR [#16](https://github.com/Super-Submarine/vmforge/pull/16) (freeze guard) |
| Tester documentation | Quickstart, CLI reference, troubleshooting/FAQ, bug-reporting guide, GUI alpha guide — `docs/tester-guide/`. | PR [#11](https://github.com/Super-Submarine/vmforge/pull/11), PR [#14](https://github.com/Super-Submarine/vmforge/pull/14) |
| Signed release artifacts | GPG-signed `.deb` + `.AppImage` with `SHA256SUMS` + detached `.asc` signatures, built on tag push. See the [install & verification guide](tester-guide/install.md). | PR [#18](https://github.com/Super-Submarine/vmforge/pull/18) (release pipeline v1), [`docs/release-pipeline.md`](release-pipeline.md) |

### Known limitations

- **Linux/KVM only.** No macOS/HVF in wave 1 (decision `019f8a7b-75b1`; HVF
  backend is in flight on PR [#12](https://github.com/Super-Submarine/vmforge/pull/12)).
- **No lifecycle verbs on `main` yet.** `vmforge create/start/stop/status/
  list/snapshot` are on PR [#3](https://github.com/Super-Submarine/vmforge/pull/3)
  and are **experimental** until promoted (freeze doc §1.3). Use the smoke
  suite and `vmforge-storage` in the meantime.
- **Networking:** user-mode NAT only; port forwards bind host `127.0.0.1`.
  Bridged/TAP is design-only. The `vmforge net` CLI (PRs
  [#2](https://github.com/Super-Submarine/vmforge/pull/2),
  [#15](https://github.com/Super-Submarine/vmforge/pull/15)) is
  **experimental**; SSH port-forward UAT-6 is out of wave 1 (freeze doc §4).
- **Guest tools** (`vmforgectl` + in-guest agent, PR
  [#4](https://github.com/Super-Submarine/vmforge/pull/4)) and
  **`vmforge diagnose`** (PR [#17](https://github.com/Super-Submarine/vmforge/pull/17))
  are in flight — use the manual diagnostics block in
  [Reporting bugs](tester-guide/reporting-bugs.md) until they land.
- **GUI alpha is a UX preview**, not wired to real VMs (PR
  [#7](https://github.com/Super-Submarine/vmforge/pull/7); see the
  [GUI guide](tester-guide/gui-guide.md) for what is stubbed vs. real).
- **QEMU is not redistributed** in wave 1: the `.deb` depends on the distro
  `qemu-system-x86` package and the `.AppImage` requires host-installed
  QEMU/KVM ([`docs/release-pipeline.md`](release-pipeline.md)).
- **Release signing key:** until the production GPG key is provisioned, tagged
  builds may be signed with a clearly-flagged ephemeral placeholder key that
  must not be trusted or distributed — see the
  [install & verification guide](tester-guide/install.md#trusting-the-signing-key).

### Supported platforms

| Platform | Status |
|---|---|
| Linux x86_64 + KVM (writable `/dev/kvm`) | **Supported** — the wave-1 target |
| Linux x86_64 without KVM | Works via slow TCG fallback (boots take minutes); file setup issues per [Troubleshooting T1](tester-guide/troubleshooting.md#t1-kvm-not-available-or-not-writable) |
| macOS (Intel or Apple Silicon) | **Not in wave 1** — wave 2 |
| Windows | Not planned for the beta |

### Privacy

Local-first, **no telemetry**: nothing is collected or uploaded automatically.
Diagnostics are attached to bug reports only when you paste them yourself
([Reporting bugs](tester-guide/reporting-bugs.md)).
