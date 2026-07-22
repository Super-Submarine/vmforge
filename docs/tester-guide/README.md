# VMForge Beta Tester Guide (wave 1, Linux)

Welcome to the VMForge private beta. Wave 1 is **Linux/KVM only** (macOS/Apple Silicon
testers join in wave 2, gated on HVF snapshot parity).

Start here, in order:

1. **[Installing VMForge on Linux](install-linux.md)** — packaged `.deb`/`.AppImage`
   installs, including GPG signature verification.
2. **[Linux quickstart](quickstart-linux.md)** — install prerequisites, verify KVM,
   create/boot/snapshot/restore your first VM, with expected timings.
3. **[CLI reference](cli-reference.md)** — the complete frozen wave-1 surface
   (`vmforge`, `vmforge-storage`, smoke suite): every command, flag, and exit code,
   verified against the code.
4. **[Working with snapshot trees](snapshot-trees.md)** — git-like snapshots:
   create, branch, revert, and delete with `vmforge-storage`.
5. **[Troubleshooting & FAQ](troubleshooting.md)** — the top failure modes (KVM
   permissions, qcow2 disk issues, networking) with diagnose/fix recipes and how each
   maps to our P1/P2/P3 triage severity.
6. **[Reporting bugs](reporting-bugs.md)** — how to file issues, which template to
   use, and the minimal diagnostics output to attach.

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
