# macOS packaging & entitlement follow-ups (NOT implemented in backend v0)

Tracked follow-ups from the HVF port plan doc ("macOS/HVF host backend:
requirements, risks & port plan", company doc 019f8a66-5e0b-7bca-911b-cad816b646ac).
Backend v0 ships the `hvf` driver only; everything below is deliberately
deferred to the M0/M4 milestones of the port plan.

## TODO — QEMU toolchain & pin (port plan M0)

- [ ] Pin a QEMU release ≥ the one containing the `virt-11.1` machine type
      (hardware-assisted HVF vGIC enabled by default,
      https://gitlab.com/qemu-project/qemu/-/commit/37863fff) and the
      explicit missing-entitlement diagnostics
      (https://gitlab.com/qemu-project/qemu/-/commit/5f3bfbd8).
- [ ] Reproducible build scripts for a bundled `qemu-system-aarch64`
      (no Homebrew dependency in the shipping app); publish exact sources +
      build scripts for GPL-2.0 compliance
      (https://www.gnu.org/licenses/gpl-faq.html#MereAggregation).
- [ ] Watch host-CPU-drift regressions per chip generation (e.g. SME on M4:
      https://gitlab.com/qemu-project/qemu/-/issues/2721); maintain a
      `-cpu host` fallback matrix test.

## TODO — Signing, entitlements, notarization (port plan M4)

- [ ] Apply the `com.apple.security.hypervisor` entitlement to the bundled
      `qemu-system-aarch64` binary itself (it is the process calling `hv_*`
      APIs). Un-entitled QEMU fails hvf init with `HV_DENIED`
      (https://gitlab.com/qemu-project/qemu/-/issues/2800).
- [ ] Add `com.apple.security.cs.allow-jit` for the TCG fallback (host JIT
      under Hardened Runtime).
- [ ] Sign every Mach-O in the bundle (GUI, engine, QEMU, dylibs, firmware)
      with a Developer ID Application cert; Hardened Runtime + notarize +
      staple the DMG
      (https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution).
- [ ] Procurement: Apple Developer Program membership for Developer ID
      certs (flagged to Mira in the port plan §3.6).
- [ ] Probe hvf availability in the engine before spawning QEMU rather than
      relying on QEMU's `accel=hvf:tcg` fallback chain (asserts when hvf is
      unavailable: https://gitlab.com/qemu-project/qemu/-/issues/2981).
      v0 exposes `HvfBackend::is_available()` (`kern.hv_support`); wire it
      into CLI/GUI preflight.

## TODO — macOS CI on real hardware (port plan M2)

- [ ] GitHub-hosted arm64 macOS runners cannot run HVF (no nested virt);
      add one self-hosted Apple Silicon runner (e.g. MacStadium M4 mini)
      with labels `[self-hosted, macOS, arm64, hvf]` and re-run the QA v0
      smoke suite plus `tcg_lifecycle.rs`'s lifecycle with `-accel hvf`.

## Post-MVP (port plan M6)

- vmnet bridged networking (`com.apple.vm.networking` — restricted
  entitlement, Apple grants case-by-case); MVP ships user-mode NAT only.
- Intel-Mac (x86/hvf) support; USB passthrough (broken on Apple Silicon,
  https://gitlab.com/qemu-project/qemu/-/issues/2178).
- RAM-snapshot `restore` on hvf (M3 snapshot-fidelity spike): vtimer
  correctness after restore, per-machine portability rule.
