# VMForge Guest Tools — Design & v1 Roadmap

Status: v0 shipped (this directory) — virtio-serial guest agent with
IP/hostname/OS reporting, heartbeat ping, graceful shutdown/reboot, plus a
host-side client library/CLI. This document covers the v1 roadmap.

## v0 architecture recap

- Transport: one virtio-serial port (`org.vmforge.agent.0`) exposed by QEMU as
  a host UNIX socket (see `README.md` for exact flags). Virtio-serial is the
  same transport qemu-guest-agent uses and requires no guest networking
  ([qemu-ga interop docs](https://qemu-project.gitlab.io/qemu/interop/qemu-ga.html)).
- Protocol: newline-delimited JSON with qemu-ga/QMP-style
  `execute`/`return`/`error` envelopes, so VMForge can later drive a stock
  qemu-guest-agent with the same client, or extend with `vmforge-*` commands.
- Agent: single-file Python 3 (stdlib only) for v0 iteration speed. v1 should
  be rewritten in Rust or Go as a static binary so guests need no interpreter
  (qemu-ga itself is C; the Windows port ships as a service in virtio-win —
  https://github.com/virtio-win/kvm-guest-drivers-windows).

## v1 features

### 1. Clipboard sharing

**Choice: SPICE vdagent protocol over a dedicated virtio-serial port
(`com.redhat.spice.0`), or qemu-ga clipboard commands as fallback.**

- The established mechanism is spice-vdagent: guest daemon + `virtserialport`
  channel; QEMU's GTK/VNC UIs (not just SPICE) gained vdagent clipboard support
  via `-device virtio-serial-pci -chardev qemu-vdagent,id=vdagent,clipboard=on
  -device virtserialport,chardev=vdagent,name=com.redhat.spice.0`
  ([QEMU 6.1 `qemu-vdagent` chardev](https://www.qemu.org/docs/master/system/devices/virtio-serial.html),
  [spice-vdagent](https://gitlab.freedesktop.org/spice/linux/vd_agent)).
- This means VMForge gets Linux-guest clipboard for free by shipping
  spice-vdagent in guest tools and passing the `qemu-vdagent` chardev —
  regardless of whether we ever adopt the full SPICE display stack.
- Wayland guests need `gtk` clipboard integration in the vdagent
  (supported since spice-vdagent 0.22,
  https://gitlab.freedesktop.org/spice/linux/vd_agent/-/releases).
- v1 plan: bundle spice-vdagent; VMForge UI wires host clipboard <-> chardev.
  Text first; images/files later.

### 2. Shared folders: virtio-fs vs 9p

**Choice: virtio-fs primary, 9p fallback for macOS hosts.**

| | virtio-fs | virtio-9p |
|---|---|---|
| Performance | Near-native; DAX-capable shared memory mappings ([virtio-fs.gitlab.io](https://virtio-fs.gitlab.io/)) | Significantly slower; chatty protocol ([virtio-fs FAQ / comparison](https://virtio-fs.gitlab.io/howto-qemu.html)) |
| Semantics | Local FS semantics, POSIX-coherent between guests/host | Network-FS semantics, weaker caching |
| Host daemon | Requires `virtiofsd` (Rust, separate process, https://gitlab.com/virtio-fs/virtiofsd) + `memory-backend` shared RAM | Built into QEMU (`-virtfs local,...`) |
| Host support | Linux hosts (vhost-user); **not available on macOS QEMU** | Works on Linux and macOS hosts |
| Guest support | Linux ≥ 5.4 (`CONFIG_VIRTIO_FS`); Windows driver in virtio-win | Linux 9p.ko, mature |

- Linux hosts (primary target): virtio-fs via
  `virtiofsd --socket-path=... --shared-dir=...` +
  `-chardev socket,id=charfs0 -device vhost-user-fs-pci,chardev=charfs0,tag=vmforge-share`
  + `-object memory-backend-memfd,share=on` ([QEMU virtio-fs howto](https://virtio-fs.gitlab.io/howto-qemu.html)).
- macOS hosts: vhost-user requires host features QEMU/hvf lacks, so fall back
  to `-virtfs local,path=...,mount_tag=...,security_model=mapped-xattr`
  (9p; [QEMU 9psetup wiki](https://wiki.qemu.org/Documentation/9psetup)).
- Guest tools role: auto-mount tags advertised by the host (`mount -t virtiofs
  vmforge-share /mnt/...` or `mount -t 9p -o trans=virtio`), report mounts back
  to the UI via a new `vmforge-mounts` agent command.

### 3. Time synchronization

**Choice: layered — kvm-clock/PTP passive sync + agent-driven step correction
after resume/snapshot-restore.**

- Steady state: guests should use the paravirtual clock (kvm-clock on KVM) and
  optionally `ptp_kvm`, which exposes host time as `/dev/ptp0` for chrony with
  sub-microsecond accuracy ([chrony FAQ on PHC/ptp_kvm](https://chrony-project.org/faq.html),
  [kernel ptp_kvm docs](https://docs.kernel.org/virt/kvm/api.html)).
- After resume-from-pause, snapshot restore, or instant-resume (a VMForge USP),
  the guest clock is stale; qemu-ga solves this with `guest-set-time`, invoked
  by the host after such events ([qemu-ga reference: guest-set-time](https://qemu-project.gitlab.io/qemu/interop/qemu-ga-ref.html)).
- v1 plan: add `guest-set-time` (settimeofday + hwclock sync) to vmforge-ga;
  core engine calls it automatically after resume/restore. Ship chrony config
  enabling `refclock PHC /dev/ptp0` when present.

### 4. Display resize (dynamic guest resolution)

**Choice: virtio-gpu (`virtio-vga`) + guest resize handling; X11 guests need
spice-vdagent/mutter support, modern guests handle it natively via KMS.**

- With `-device virtio-vga` (or `virtio-gpu-pci`), QEMU can announce a new
  display size to the guest; the virtio-gpu KMS driver exposes it as a hotplug
  display mode change ([QEMU virtio-gpu docs](https://www.qemu.org/docs/master/system/devices/virtio-gpu.html),
  [kraxel: VGA and other display devices in QEMU](https://www.kraxel.org/blog/2019/09/display-devices-in-qemu/)).
- Wayland desktops (GNOME/mutter) auto-resize to the announced mode; X11
  sessions need a resize agent — spice-vdagent again performs the
  `xrandr`-equivalent on the vdagent channel ([spice-vdagent](https://gitlab.freedesktop.org/spice/linux/vd_agent)).
- Windows guests: virtio-gpu DOD driver from virtio-win handles resolution
  changes (https://github.com/virtio-win/kvm-guest-drivers-windows).
- v1 plan: core engine sends QMP display-size changes on window resize; guest
  tools ship spice-vdagent for X11 guests; nothing needed for Wayland/KMS
  console guests.

## Channel map (reserved names)

| virtio-serial port name | Purpose |
|---|---|
| `org.vmforge.agent.0` | control agent (v0, this repo) |
| `com.redhat.spice.0` | vdagent: clipboard + X11 resize (v1) |
| `org.vmforge.events.0` | async guest→host events, e.g. IP change (v1, planned) |

## Security notes

- The agent executes only an allow-listed command set; no arbitrary exec in v0.
  If v1 adds `guest-exec`, gate it behind a per-VM policy flag like qemu-ga's
  allow/block lists (`qemu-ga --block-rpcs`, see qemu-ga docs above).
- Host socket is a UNIX socket owned by the VMForge engine user; no TCP.
