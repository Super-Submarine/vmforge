# VMForge Guest Tools

Guest agent + host client for VMForge VMs, communicating over a **virtio-serial**
channel (no guest networking required). The wire protocol is newline-delimited
JSON modeled on [qemu-guest-agent](https://qemu-project.gitlab.io/qemu/interop/qemu-ga.html)
QMP conventions — full spec, versioning and compatibility rules in
[`PROTOCOL.md`](PROTOCOL.md); the binding interface contract is
[`docs/interface-contracts.md` §3](../docs/interface-contracts.md).

## Components

| Path | What |
|---|---|
| `agent/vmforge-ga.py` | Guest agent (Python 3 stdlib only; runs on Alpine/Debian/any Linux with `iproute2`) |
| `agent/vmforge-ga.service` | systemd unit (Debian/Fedora/Ubuntu guests) |
| `agent/vmforge-ga.openrc` | OpenRC service (Alpine guests) |
| `host/vmforgectl.py` | Host-side client library (`GuestAgentClient`) and CLI |
| `PROTOCOL.md` | Wire protocol v1.1: commands, error codes, versioning |
| `tests/` | Protocol unit tests, golden transcript, Alpine end-to-end smoke |

## QEMU flags the core engine must pass

Per the M1 contract (`guest_agent: true` in `vm.json`):

```
-device virtio-serial-pci,id=vmforge-vs0
-chardev socket,id=vmforge-ga0,path=$VMFORGE_HOME/vms/<vm>/guest-agent.sock,server=on,wait=off
-device virtserialport,bus=vmforge-vs0.0,chardev=vmforge-ga0,name=org.vmforge.agent.0
```

- The socket lives under `$VMFORGE_HOME` (default `~/.vmforge`) — a
  user-private directory. Never place it in `/tmp`: a predictable
  world-writable path is squattable by other local users.
- `-chardev socket,...,server=on,wait=off` creates a host UNIX socket QEMU
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

When QA's M1 image bake (`qa/images/bake.sh`) lands, it should run the Alpine
steps above so the agent ships pre-installed in the smoke image.

## Host usage

`--vm <name>` resolves the contract socket path
`$VMFORGE_HOME/vms/<name>/guest-agent.sock`; `--socket <path>` overrides it
for debugging.

```sh
host/vmforgectl.py --vm myvm wait-ready       # poll until agent up
host/vmforgectl.py --vm myvm ping             # heartbeat -> {}
host/vmforgectl.py --vm myvm info             # os/kernel/hostname/agent_version
host/vmforgectl.py --vm myvm interfaces       # [{name, mac, ips}] per NIC
host/vmforgectl.py --vm myvm net-info         # {hostname, ips} for GUI/CLI display
host/vmforgectl.py --vm myvm shutdown         # graceful poweroff (ack only)
host/vmforgectl.py --vm myvm shutdown --mode reboot
# graceful with timeout + hard-stop fallback (engine supplies the hard stop):
host/vmforgectl.py --vm myvm shutdown --wait --shutdown-timeout 60 \
    --hard-stop-cmd 'echo quit | nc -U $VMFORGE_HOME/vms/myvm/qmp.sock'
host/vmforgectl.py --vm myvm exec -- uname -a # run in guest, stdout/stderr/exit code
```

As a library:

```python
from vmforgectl import GuestAgentClient, default_socket_path
ga = GuestAgentClient(default_socket_path("myvm"))
ga.wait_ready()
ga.check_protocol()                 # raises GuestAgentIncompatible on v0 agents
print(ga.net_info())                # {'hostname': ..., 'ips': [...]}
r = ga.exec_in_guest(["uname", "-a"])   # {'exit_code': 0, 'stdout': ..., 'stderr': ...}
ga.shutdown_graceful(timeout=60, hard_stop=engine_kill_qemu)
```

## Tests

```sh
ruff check guest-tools
pytest guest-tools/tests            # protocol unit + golden-transcript tests
guest-tools/tests/ga_smoke.sh       # Alpine end-to-end (QEMU; KVM or TCG)
```

The end-to-end smoke boots the cached Alpine cloud image with the agent
installed via cloud-init, then exercises `wait-ready`, `info`, `net-info`,
`interfaces`, `exec` and `shutdown --wait` (with hard-stop fallback armed)
over the real virtio-serial channel. CI runs it in `.github/workflows/`
alongside the QA smoke (KVM on hosted runners, TCG fallback). Manual-only
coverage: `reboot`/`halt` modes and non-Alpine guests (documented in
`tests/ga_smoke.sh`).
