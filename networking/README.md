# VMForge Networking v0

User-mode NAT backend for the VMForge engine: a small library + CLI that
generates the QEMU `-netdev user` / `-device virtio-net-pci` arguments from a
simple JSON config (including host->guest `hostfwd` port-forwarding rules),
and can hot-add/remove port forwards on a running VM via QMP.

See [DESIGN.md](DESIGN.md) for the v1 design (bridged/TAP, host-only,
macOS vmnet / Windows notes).

## Library

```python
from vmforge_net import NatConfig, PortForward, build_qemu_args, QMPClient

cfg = NatConfig(
    netdev_id="net0",
    forwards=[PortForward(host_port=8080, guest_port=80)],
)
build_qemu_args(cfg)
# ['-netdev', 'user,id=net0,hostfwd=tcp:127.0.0.1:8080-:80',
#  '-device', 'virtio-net-pci,netdev=net0']

# Hot-add a forward on a running VM (QMP socket started with
# -qmp unix:/tmp/qmp.sock,server=on,wait=off):
with QMPClient.connect_unix("/tmp/qmp.sock") as qmp:
    qmp.hostfwd_add("net0", PortForward(host_port=2222, guest_port=22))
```

## CLI

No dependencies beyond Python 3.10+. Run from this directory with
`python -m vmforge_net ...`, or `pip install .` for the `vmforge-net` entry
point.

```console
$ python -m vmforge_net args --config examples/nat.json
-netdev user,id=vmforge-nat0,net=10.0.2.0/24,hostfwd=tcp:127.0.0.1:8080-:8000,hostfwd=tcp:127.0.0.1:2222-:22,... -device virtio-net-pci,netdev=vmforge-nat0,mac=52:54:00:12:34:56

$ python -m vmforge_net args -f 8080:80 --format json   # for the core engine
["-netdev", "user,id=vmforge-nat0,hostfwd=tcp:127.0.0.1:8080-:80", "-device", "virtio-net-pci,netdev=vmforge-nat0"]

# Hot-add / remove a forward on a running VM:
$ python -m vmforge_net hostfwd-add --qmp-unix /tmp/qmp.sock --netdev-id net0 tcp:127.0.0.1:2222-:22
$ python -m vmforge_net hostfwd-remove --qmp-unix /tmp/qmp.sock --netdev-id net0 tcp:127.0.0.1:2222-:22
```

Forward specs are `proto:hostip:hostport-guestip:guestport` (guest IP may be
empty for the DHCP address) or the shorthand `hostport:guestport` (TCP,
bound to 127.0.0.1).

Note on QMP hot-add: QEMU exposes no native QMP command for user-mode
hostfwd management, so `QMPClient` uses the QMP `human-monitor-command`
passthrough to run the HMP `hostfwd_add`/`hostfwd_remove` commands. This is
supported wherever the QMP monitor is available (all VMForge target hosts).

## Tests

```console
$ python -m pytest tests/ -q
```
