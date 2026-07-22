# VMForge Beta Tester Guide (v1 — wave 1, Linux)

Welcome to the VMForge private beta. Wave 1 is **Linux/KVM only** (macOS/Apple Silicon
testers join in wave 2, gated on HVF snapshot parity).

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
