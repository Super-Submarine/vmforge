# Engine error taxonomy (wave-1)

**Status:** v1 · **Scope:** every user-visible engine failure on boot/KVM/disk
paths. Consumed by the CLI, the GUI (design spec v2 error states) and tester
docs. Codes and exit codes are **stable**: once released they may not change
meaning; new classes are additive only (per the wave-1 CLI freeze,
`docs/cli-freeze-v1.0-beta.md` on PR #16).

Implementation: `crates/vmforge-core/src/taxonomy.rs` (`ErrorClass`,
`EngineError`, classifiers) and `crates/vmforge-core/src/doctor.rs` (host
preflight probes). CLI surface: `vmforge doctor [--json] [--disk PATH]...`
plus taxonomy exit codes on every failing verb.

## 1. Taxonomy table

| Code | Exit | Cause | User-facing message | Recovery |
|---|---|---|---|---|
| `kvm_unavailable` | 10 | `/dev/kvm` missing: no KVM module, virtualization disabled in firmware, or no nested virt | KVM is not available on this host (/dev/kvm missing) | Enable VT-x/AMD-V in firmware; `modprobe kvm_intel`/`kvm_amd`; enable nested virt inside VMs. TCG fallback is possible but slow |
| `kvm_permission_denied` | 11 | `/dev/kvm` exists but the user cannot open it read/write | permission denied opening /dev/kvm | `sudo usermod -aG kvm $USER` and re-login, or fix /dev/kvm permissions |
| `qemu_binary_missing` | 12 | No `qemu-system-*` binary found (PATH or `VMFORGE_QEMU_BIN`) | QEMU binary not found on this host | `sudo apt install qemu-system-x86` or set `VMFORGE_QEMU_BIN` |
| `boot_timeout` | 13 | Guest did not become ready before the boot timeout | the VM did not become ready before the boot timeout | Check `vms/<vm>/serial.log`; verify the disk image boots; retry with a larger timeout |
| `qemu_crashed` | 14 | QEMU exited unexpectedly during or after startup | QEMU exited unexpectedly | Inspect QEMU stderr in `details` and the serial log; verify QEMU version and VM config |
| `disk_full` | 15 | Volume holding `$VMFORGE_HOME` out of space (ENOSPC or preflight) | not enough free disk space for VM state | Delete unused VMs/snapshots or move `VMFORGE_HOME` to a larger disk |
| `disk_image_missing` | 16 | Referenced disk image path does not exist | disk image not found | Check the disk path in the VM config; re-create/import via `vmforge-storage` |
| `disk_image_corrupt` | 17 | Disk exists but fails qcow2 validation (bad magic, truncated header, bad version) | disk image is not a valid qcow2 file | `vmforge-storage check <vm> <disk> --repair`, or restore from snapshot/backup |
| `snapshot_conflict` | 18 | Snapshot tag already exists (create), tag missing (restore/delete), or VM in the wrong state for an offline operation | snapshot operation conflicts with existing state | `vmforge snapshot list <vm>`; pick an unused/existing tag; stop the VM for offline ops |
| `port_in_use` | 19 | Host port needed for VNC/port-forward/QMP TCP already bound | a required host port is already in use | `ss -ltnp` to find the holder; stop it or configure a different port |
| `internal` | 20 | Any engine failure not classified above; `details` carries the raw error | internal engine error | Retry; if persistent, file a bug per `docs/tester-guide/reporting-bugs.md` |

Exit codes 0 (success), 1 (generic error) and 2 (usage error) keep their
frozen meanings; taxonomy classes occupy the reserved 10–20 range so they can
never collide with them.

## 2. Wire format (contract §0/§4)

On failure, `--json` mode prints exactly one JSON document on **stderr**
(stdout stays reserved for the command's success document):

```json
{"error": {
  "code": "kvm_permission_denied",
  "message": "cannot open /dev/kvm read/write: Permission denied (os error 13)",
  "recovery": "Add your user to the kvm group (`sudo usermod -aG kvm $USER`) and re-login, or fix permissions on /dev/kvm.",
  "details": {}
}}
```

`code` is machine-stable; `message` may carry instance specifics; `recovery`
is display-ready for CLI and GUI; `details` is optional (e.g.
`{"qemu_stderr": "..."}` for `qemu_crashed`).

Without `--json`, the human form is:

```
error: <message> (<code>)
recovery: <recovery>
```

## 3. `vmforge doctor` — host preflight

`vmforge doctor [--json] [--disk PATH]...` runs the probes in order and exits
with the first failing probe's taxonomy exit code (0 when all pass):

1. **kvm** — `/dev/kvm` exists (`kvm_unavailable`) and opens read/write
   (`kvm_permission_denied`)
2. **qemu** — a `qemu-system-x86_64`/`qemu-system-aarch64` binary is found
   (`qemu_binary_missing`)
3. **home** — `$VMFORGE_HOME` is writable and its volume has ≥ 512 MiB free
   (`disk_full`)
4. **disk** (per `--disk PATH`) — image exists (`disk_image_missing`) and has
   a valid qcow2 header (`disk_image_corrupt`)

With `--json`, the full probe report is one JSON document on stdout
(`{"ok": bool, "probes": [...]}`); on failure the first error object is also
emitted on stderr in the §2 shape.

## 4. Failure-injection testing

Automated coverage lives in
`crates/vmforge-cli/tests/failure_injection.rs` (runs in the CI
`build-lint-test` job, plus a dedicated `failure-injection` step) and the unit
tests in `taxonomy.rs`:

- **Real-condition injection** (no special hardware): nonexistent KVM node
  (`VMFORGE_KVM_PATH`), `chmod 000` device node, missing QEMU binary
  (`VMFORGE_QEMU_BIN`), free-space threshold (`VMFORGE_MIN_FREE_BYTES`),
  missing disk image, truncated/garbage qcow2.
- **Classifier unit tests**: raw `io::Error` mapping (ENOSPC → `disk_full`,
  EADDRINUSE → `port_in_use`, …) and QEMU stderr pattern mapping for every
  class.
- **End-to-end injection knob**: `VMFORGE_INJECT_ERROR=<code>` makes any verb
  fail with that class (JSON + exit code), covering the hardware-bound
  classes in CI and letting GUI development exercise every error state.

The three env knobs above are **test/dev only** and not part of the frozen
CLI surface.

### Manual procedures (hardware-bound classes)

These need a real KVM host to trigger naturally; run them before each wave
release on the KVM smoke machine (see `docs/ci.md` KVM smoke plan):

| Class | Procedure | Expected |
|---|---|---|
| `boot_timeout` | Boot a VM from a blank qcow2 (no OS) with a short timeout | exit 13, message names the VM and timeout |
| `qemu_crashed` | Boot a VM, then `kill -9` the QEMU process | exit 14, `details.qemu_stderr`/exit status captured |
| `snapshot_conflict` | `snapshot create` twice with the same tag; `snapshot restore` with a bogus tag | exit 18 both times |
| `port_in_use` | Bind the VNC port (`nc -l 5901`) then boot a VM configured with `-vnc :1` | exit 19, message names the port |
| `disk_full` (natural) | Point `VMFORGE_HOME` at a small loop-mounted fs (`mount -t tmpfs -o size=8m`), snapshot a VM | exit 15 on ENOSPC |

## 5. Freeze-doc reconciliation

`vmforge doctor` and exit codes 10–20 are **additive** to the frozen wave-1
surface: `info`'s behavior and exit codes 0/1/2 are unchanged. When the
freeze manifest (`qa/cli-freeze/frozen-surface.json`, PR #16) merges, add
`doctor` and the 10–20 range to it in the promotion PR.
