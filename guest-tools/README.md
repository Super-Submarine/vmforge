# VMForge Guest Tools v0

Guest agent + host client for VMForge VMs, communicating over a **virtio-serial**
channel (no guest networking required). The wire protocol is newline-delimited
JSON modeled on [qemu-guest-agent](https://qemu-project.gitlab.io/qemu/interop/qemu-ga.html)
QMP conventions, so we can later interoperate with or replace qemu-ga.

## Components

| Path | What |
|---|---|
| `agent/vmforge-ga.py` | Guest agent (Python 3 stdlib only; runs on Alpine/Debian/any Linux with `iproute2`) |
| `agent/vmforge-ga.service` | systemd unit (Debian/Fedora/Ubuntu guests) |
| `agent/vmforge-ga.openrc` | OpenRC service (Alpine guests) |
| `host/vmforgectl.py` | Host-side client library (`GuestAgentClient`) and CLI |

## QEMU flags the core engine must pass

```
-device virtio-serial-pci,id=vmforge-vs0
-chardev socket,id=vmforge-ga0,path=/tmp/vmforge-ga.sock,server=on,wait=off
-device virtserialport,bus=vmforge-vs0.0,chardev=vmforge-ga0,name=org.vmforge.agent.0
```

- The `-chardev socket,...,server=on,wait=off` creates a host UNIX socket QEMU
  listens on; `wait=off` lets the VM boot before any client connects.
- `virtserialport` with `name=org.vmforge.agent.0` surfaces the channel in the
  guest as `/dev/virtio-ports/org.vmforge.agent.0` (udev symlink to `/dev/vportNpM`).
- One `virtio-serial-pci` controller supports multiple ports; future channels
  (clipboard, time-sync — see `DESIGN.md`) add more `virtserialport`+`-chardev`
  pairs on the same `vmforge-vs0` bus.

Reference: [QEMU virtio-serial / chardev docs](https://www.qemu.org/docs/master/system/invocation.html#hxtool-6),
[qemu-ga interop docs](https://qemu-project.gitlab.io/qemu/interop/qemu-ga.html).

## Guest install

```sh
# Debian-family
install -m 0755 agent/vmforge-ga.py /usr/local/bin/vmforge-ga.py
install -m 0644 agent/vmforge-ga.service /etc/systemd/system/
systemctl enable --now vmforge-ga

# Alpine
apk add python3
install -m 0755 agent/vmforge-ga.py /usr/local/bin/vmforge-ga.py
install -m 0755 agent/vmforge-ga.openrc /etc/init.d/vmforge-ga
rc-update add vmforge-ga && rc-service vmforge-ga start
```

## Host usage

```sh
host/vmforgectl.py --socket /tmp/vmforge-ga.sock wait-ready   # poll until agent up
host/vmforgectl.py --socket /tmp/vmforge-ga.sock ping         # heartbeat -> {}
host/vmforgectl.py --socket /tmp/vmforge-ga.sock host-info    # hostname/OS/kernel/IPs
host/vmforgectl.py --socket /tmp/vmforge-ga.sock interfaces   # full per-NIC detail
host/vmforgectl.py --socket /tmp/vmforge-ga.sock shutdown     # graceful poweroff
host/vmforgectl.py --socket /tmp/vmforge-ga.sock shutdown --mode reboot
```

As a library:

```python
from vmforgectl import GuestAgentClient
ga = GuestAgentClient("/tmp/vmforge-ga.sock")
ga.wait_ready()
print(ga.host_info()["ip-addresses"])
ga.shutdown()
```

## Protocol

Request/response are single JSON lines:

```
-> {"execute": "guest-get-host-info", "id": 1}
<- {"return": {"hostname": "vmforge-test", "ip-addresses": ["10.0.2.15", ...], ...}, "id": 1}
-> {"execute": "guest-shutdown", "arguments": {"mode": "powerdown"}, "id": 2}
<- {"return": {"mode": "powerdown"}, "id": 2}
```

Errors come back as `{"error": {"desc": "..."}, "id": N}`. Commands:
`guest-ping`, `guest-info`, `guest-get-host-info`,
`guest-network-get-interfaces`, `guest-shutdown` (`powerdown|reboot|halt`).
