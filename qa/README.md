# VMForge QA

- `TEST_PLAN.md` — MVP test matrix (create/boot/stop/snapshot/restore × images × accelerators × failure cases).
- `smoke/` — automated smoke suite, wired into CI via `.github/workflows/qa-smoke.yml`.

## Running the smoke suite

```bash
# deps (Debian/Ubuntu): qemu-system-x86 qemu-utils genisoimage python3
qa/smoke/smoke_test.sh                # happy path: boot -> snapshot -> restore -> shutdown
qa/smoke/smoke_test.sh --negative     # failure cases (corrupt disk, missing image, kill -9, bad loadvm)
FORCE_TCG=1 qa/smoke/smoke_test.sh    # force TCG even when /dev/kvm exists
```

Accelerator selection: KVM is used automatically when `/dev/kvm` exists and is
writable; otherwise TCG. `FORCE_TCG=1` overrides.

Environment overrides:

| Var | Default | Purpose |
|-----|---------|---------|
| `GUEST_IMAGE_URL` | Alpine 3.20 nocloud qcow2 | Any NoCloud-compatible qcow2 cloud image (Debian/Ubuntu rows of the matrix) |
| `GUEST_LOGIN_REGEX` | `login:` | Serial-console readiness pattern |
| `BOOT_TIMEOUT` | `300` | Seconds to wait for boot-to-ready |
| `WORKDIR` | `qa/smoke/.work` | Scratch + image cache dir |
| `VM_MEM` | `512` | Guest RAM (MB); savevm time scales with this |

## Driver interface (slotting in team CLIs)

`smoke_test.sh` never calls QEMU directly — it goes through a **driver**: a bash file
sourced from `qa/smoke/drivers/` (default `qemu.sh`, selected with `DRIVER=<name>`)
that must implement:

```bash
vm_create <disk.qcow2> <seed.iso>   # define the VM (create overlay disk etc.)
vm_boot                             # start it, serial console logged to $SERIAL_LOG
vm_wait_ready <timeout_s>           # block until login prompt / cloud-init marker
vm_snapshot <name>                  # snapshot the RUNNING vm
vm_restore <name>                   # restore the running vm to <name>
vm_stop                             # graceful shutdown (powerdown), then wait
vm_kill                             # hard kill (used by negative tests)
vm_is_running                       # exit 0 iff the vm process is alive
```

When core lands the VMForge CLI, add `drivers/vmforge.sh` mapping these to
`vmforge create/start/snapshot/restore/stop ...` and run
`DRIVER=vmforge qa/smoke/smoke_test.sh` — assertions and CI stay unchanged. The QMP
helper `qa/smoke/qmp.py` (tiny stdlib-only QMP client) is reusable by any driver that
talks to a QEMU-backed engine.
