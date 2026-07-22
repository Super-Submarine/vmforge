# Troubleshooting by error code (engine error taxonomy)

> **Status: NOT YET MERGED.** The structured error taxonomy, exit codes
> 10–20, JSON error output, and `vmforge doctor` are engine error-path
> hardening v1, in review on PR
> [#30](https://github.com/Super-Submarine/vmforge/pull/30)
> (`devin/1784739142-error-taxonomy`, `docs/error-taxonomy.md`). Codes and
> exit codes are designed to be **stable once released** and additive to the
> frozen wave-1 CLI surface. Until PR #30 (and the engine lifecycle verbs,
> PR #3) merge, current `main` builds only use exit codes 0/1/2 — this page
> is keyed to the taxonomy so it is ready the moment they land.

When a `vmforge` command fails it prints:

```
error: <message> (<code>)
recovery: <recovery>
```

or, with `--json`, one JSON document on stderr:

```json
{"error": {"code": "...", "message": "...", "recovery": "...", "details": {}}}
```

Look up the `code` (or the exit code) below. Exit codes 0 (success),
1 (generic error) and 2 (usage error) keep their frozen meanings; taxonomy
classes occupy the reserved 10–20 range.

Severity triage per the [tester guide rubric](README.md#severity-rubric-used-everywhere-in-this-guide):
a failure that loses data or breaks snapshot-restore is **P1** regardless of
its code.

## `kvm_unavailable` — exit 10

- **Symptom:** boot fails immediately with "KVM is not available on this
  host (/dev/kvm missing)"; `vmforge doctor` fails its first probe.
- **Likely cause:** the KVM module is not loaded, virtualization (VT-x /
  AMD-V) is disabled in firmware, or you are inside a VM without nested
  virtualization.
- **Recovery:** enable VT-x/AMD-V in BIOS/UEFI; `sudo modprobe kvm_intel`
  (or `kvm_amd`); enable nested virt when running inside a VM. TCG fallback
  is possible but slow (see [Troubleshooting T4](troubleshooting.md#t4-fell-back-to-tcg-everything-is-very-slow)).

## `kvm_permission_denied` — exit 11

- **Symptom:** "permission denied opening /dev/kvm" although `/dev/kvm`
  exists.
- **Likely cause:** your user is not in the `kvm` group (or the device node
  has restrictive permissions).
- **Recovery:** `sudo usermod -aG kvm $USER`, then log out and back in — or
  fix the permissions on `/dev/kvm`. Details in
  [Troubleshooting T1](troubleshooting.md#t1-kvm-not-available-or-not-writable).

## `qemu_binary_missing` — exit 12

- **Symptom:** "QEMU binary not found on this host".
- **Likely cause:** no `qemu-system-*` binary on `PATH` (and
  `VMFORGE_QEMU_BIN` unset) — QEMU is not redistributed in wave-1 packages.
- **Recovery:** `sudo apt install qemu-system-x86` (Debian/Ubuntu), or point
  `VMFORGE_QEMU_BIN` at your QEMU binary.

## `boot_timeout` — exit 13

- **Symptom:** "the VM did not become ready before the boot timeout"; the VM
  process may still be running but never reached the ready marker.
- **Likely cause:** the disk image has no bootable OS, the guest is very slow
  (TCG fallback), or the readiness marker never appears on the serial
  console.
- **Recovery:** check `vms/<vm>/serial.log`; verify the disk image boots
  (e.g. manually with QEMU); retry with a larger timeout. If everything is
  slow, check for TCG fallback (T4).

## `qemu_crashed` — exit 14

- **Symptom:** "QEMU exited unexpectedly" during or after startup.
- **Likely cause:** bad QEMU arguments for your QEMU version, a QEMU bug, or
  the process was killed (OOM killer, manual `kill`).
- **Recovery:** inspect the QEMU stderr carried in the error's `details` and
  the serial log; verify your QEMU version and the VM config. If
  reproducible, file a **P1/P2** bug with a diagnose bundle.

## `disk_full` — exit 15

- **Symptom:** "not enough free disk space for VM state" — on boot,
  snapshot, or mid-run (ENOSPC).
- **Likely cause:** the volume holding `$VMFORGE_HOME` is out of space;
  qcow2 overlays and snapshot layers grow on write.
- **Recovery:** delete unused VMs/snapshots (`vmforge-storage delete`,
  `vmforge-storage snapshot delete`), or move `$VMFORGE_HOME` to a larger
  disk. `vmforge doctor` warns when free space is below 512 MiB.

## `disk_image_missing` — exit 16

- **Symptom:** "disk image not found" at boot or storage operation.
- **Likely cause:** the path in the VM config points at a moved/deleted
  file, or a base image of a linked clone was removed.
- **Recovery:** check the disk path in the VM config; re-create or re-import
  the image via `vmforge-storage import`/`create`; `vmforge-storage info
  <vm> <disk>` shows the full backing chain.

## `disk_image_corrupt` — exit 17

- **Symptom:** "disk image is not a valid qcow2 file" — bad magic, truncated
  header, or unsupported version.
- **Likely cause:** interrupted copy/download, disk-level corruption, or the
  file is not actually qcow2 (e.g. raw/ISO imported without `--format`).
- **Recovery:** `vmforge-storage check <vm> <disk> --repair`; if
  unrepairable, restore the disk from a snapshot or a
  [backup bundle](backup-restore.md). Corruption that loses data is **P1**.

## `snapshot_conflict` — exit 18

- **Symptom:** "snapshot operation conflicts with existing state" — creating
  a tag that already exists, restoring/deleting a tag that doesn't, or
  running an offline operation on a running VM.
- **Likely cause:** tag-name collision or wrong VM state for the operation.
- **Recovery:** `vmforge snapshot list <vm>` (engine) or `vmforge-storage
  tree <vm> <disk>` to see existing tags; pick an unused (create) or
  existing (restore/delete) tag; stop the VM for offline operations.

## `port_in_use` — exit 19

- **Symptom:** "a required host port is already in use" — boot fails setting
  up VNC, a port forward, or QMP TCP.
- **Likely cause:** another process (or another VM) already binds the host
  port.
- **Recovery:** `ss -ltnp` to find the holder; stop it or configure a
  different port. `vmforge-net doctor`'s `forwards.health` check flags
  cross-VM port collisions (see [Diagnostics](diagnostics.md)).

## `internal` — exit 20

- **Symptom:** "internal engine error" — any engine failure not classified
  above; `details` carries the raw error.
- **Likely cause:** a bug in VMForge.
- **Recovery:** retry once; if it persists, file a bug per
  [Reporting bugs](reporting-bugs.md) with the full JSON error and a
  `vmforge diagnose` bundle.

## Reference

Authoritative table (messages, wire format, probe order, failure-injection
test matrix): [`docs/error-taxonomy.md`](../error-taxonomy.md) on PR #30.
Symptom-first (rather than code-first) recipes remain in
[Troubleshooting & FAQ](troubleshooting.md).
