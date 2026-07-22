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
4. **[Backup & restore](backup-restore.md)** — export a whole VM (disk chain +
   snapshot tree + config) to a portable bundle and restore it, with integrity
   verification. New in storage v1.2, experimental.
5. **[Networking](networking.md)** — port forwarding with `vmforge net`, plus the
   two-VM connectivity task. Host-only/internal networks are not shipped yet.
6. **[Reporting bugs](reporting-bugs.md)** — how to file issues, which template to
   use, and the minimal diagnostics output to attach.

## Post-freeze feature tasks (new — please exercise these)

In addition to AT-1…AT-5 from your welcome email, wave-1 testers should run:

| Task | What to do | Expected outcome |
|---|---|---|
| **AT-6 backup/restore** | [Back up a VM with snapshots, restore it under a new name, then corrupt a copy of the bundle and try restoring it](backup-restore.md) | Restored tree identical (`vmforge-storage tree`), health check clean; tampered bundle rejected with exit 1 and a `checksum mismatch` JSON error, nothing left behind |
| **AT-7 two-VM networking** | [Connect two VMs via a host port-forward](networking.md#task-connect-two-vms-at-7) | Guest A reaches guest B's sshd through `10.0.2.2:<host-port>` |
| **AT-8 diagnostics** | Run `vmforge diagnose --output diag.tar` and inspect it (`tar -tf`) | Bundle contains `report.txt` (+ per-VM logs); secrets/IPs redacted; nothing uploaded |

Any AT-6 restore failure on an *untampered* bundle is **P1** (data loss class) —
file it immediately.

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
