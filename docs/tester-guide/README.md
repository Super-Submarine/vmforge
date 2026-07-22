# VMForge Beta Tester Guide (v1 — wave 1, Linux)

Welcome to the VMForge private beta. Wave 1 is **Linux/KVM only** (macOS/Apple Silicon
testers join in wave 2, gated on HVF snapshot parity).

What's in this build and what's known-broken: **[release notes](../release-notes.md)**.
Installing from a signed release instead of building from source:
**[install & verification guide](install.md)**.

Start here, in order:

1. **[Linux quickstart](quickstart-linux.md)** — install prerequisites, verify KVM,
   create/boot/snapshot/restore your first VM, with expected timings.
2. **[CLI reference](cli-reference.md)** — every `vmforge` command and flag, verified
   against `main`.
3. **[Troubleshooting & FAQ](troubleshooting.md)** — the top failure modes (KVM
   permissions, qcow2 disk issues, networking) with diagnose/fix recipes and how each
   maps to our P1/P2/P3 triage severity.
4. **[Reporting bugs](reporting-bugs.md)** — how to file issues, which template to
   use, and the minimal diagnostics output to attach.
5. **[GUI alpha user guide](gui-guide.md)** — the Tauri VM-manager preview:
   launching it, one-click VM creation, the console viewer, what is stubbed vs.
   real, and a CLI ↔ GUI feature-parity table with CLI fallbacks.

Topic guides (new in docs v1.3 — the features they cover are **in flight**,
not on `main` yet; each page states its exact merge status):

- **[Backup & restore](backup-restore.md)** — whole-VM portable bundles
  (`vmforge-storage backup`/`restore`, PR #28), plus the manual backup
  recipe that works on `main` today.
- **[Port forwarding & guest SSH](port-forwarding.md)** — `--forward` specs
  and `vmforge net` helpers (PR #15), plus the manual QEMU `hostfwd` recipe.
- **[Shared folders](shared-folders.md)** — status page: announced but not
  yet in the repository; workarounds for moving files host ↔ guest.
- **[Diagnostics](diagnostics.md)** — `vmforge doctor` (PR #30),
  `vmforge-net doctor` (branch), `vmforge diagnose` (PR #17), and when to
  use each.
- **[Troubleshooting by error code](error-codes.md)** — the 11-class engine
  error taxonomy (exit codes 10–20, PR #30): symptom, likely cause, and
  recovery for every code.

## Severity rubric (used everywhere in this guide)

| Severity | Definition | Our response |
|---|---|---|
| **P1** | Data loss, snapshot-**restore failure**, or crash of a running VM | Acknowledged < 24 h; fix or workaround < 1 week |
| **P2** | A golden-path (AT/UAT script) step fails but you can recover | Triaged < 3 days |
| **P3** | Paper cuts, UX friction, confusing flows | Batched weekly |

Every snapshot-restore failure is automatically **P1** — file it immediately.

## Privacy

VMForge is local-first. There is **no telemetry**: nothing is collected or uploaded
automatically. Diagnostics are attached to bug reports only when *you* choose to
paste them (see [Reporting bugs](reporting-bugs.md)).
