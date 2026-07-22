import json
import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from vmforge_net import NatConfig, PortForward, build_qemu_args
from vmforge_net.natgen import build_netdev_arg


def test_default_args():
    args = build_qemu_args(NatConfig())
    assert args == [
        "-netdev",
        "user,id=vmforge-nat0",
        "-device",
        "virtio-net-pci,netdev=vmforge-nat0",
    ]


def test_full_config():
    cfg = NatConfig(
        netdev_id="net0",
        mac="52:54:00:12:34:56",
        net="10.0.2.0/24",
        host="10.0.2.2",
        dns="10.0.2.3",
        dhcp_start="10.0.2.15",
        hostname="alpine",
        forwards=[
            PortForward(host_port=8080, guest_port=80),
            PortForward(host_port=2222, guest_port=22, host_ip="0.0.0.0"),
        ],
    )
    assert build_qemu_args(cfg) == [
        "-netdev",
        "user,id=net0,net=10.0.2.0/24,host=10.0.2.2,dns=10.0.2.3,"
        "dhcpstart=10.0.2.15,hostname=alpine,"
        "hostfwd=tcp:127.0.0.1:8080-:80,hostfwd=tcp:0.0.0.0:2222-:22",
        "-device",
        "virtio-net-pci,netdev=net0,mac=52:54:00:12:34:56",
    ]


def test_restrict():
    assert "restrict=on" in build_netdev_arg(NatConfig(restrict=True))


def test_forward_parse():
    f = PortForward.parse("tcp:0.0.0.0:8080-:80")
    assert f.to_hostfwd() == "tcp:0.0.0.0:8080-:80"
    f = PortForward.parse("2222:22")
    assert f.to_hostfwd() == "tcp:127.0.0.1:2222-:22"
    f = PortForward.parse("udp:127.0.0.1:5353-10.0.2.15:53")
    assert f.to_hostfwd() == "udp:127.0.0.1:5353-10.0.2.15:53"


@pytest.mark.parametrize(
    "kwargs",
    [
        dict(host_port=0, guest_port=80),
        dict(host_port=8080, guest_port=70000),
        dict(host_port=8080, guest_port=80, proto="sctp"),
        dict(host_port=8080, guest_port=80, host_ip="not-an-ip"),
    ],
)
def test_invalid_forward(kwargs):
    with pytest.raises(ValueError):
        PortForward(**kwargs)


def test_invalid_config():
    with pytest.raises(ValueError):
        NatConfig(mac="zz:54:00:12:34:56")
    with pytest.raises(ValueError):
        NatConfig(netdev_id="bad id")
    with pytest.raises(ValueError):
        NatConfig(net="999.0.0.0/24")


def test_from_dict_roundtrip():
    d = {
        "netdev_id": "net0",
        "net": "10.0.2.0/24",
        "forwards": [{"host_port": 8080, "guest_port": 80}],
    }
    cfg = NatConfig.from_dict(d)
    assert cfg.forwards[0].to_hostfwd() == "tcp:127.0.0.1:8080-:80"


def test_cli_args_json(tmp_path):
    cfg = {"netdev_id": "net0", "forwards": [{"host_port": 8080, "guest_port": 80}]}
    p = tmp_path / "nat.json"
    p.write_text(json.dumps(cfg))
    out = subprocess.run(
        [sys.executable, "-m", "vmforge_net", "args", "--config", str(p), "--format", "json"],
        capture_output=True,
        text=True,
        check=True,
        cwd=Path(__file__).resolve().parents[1],
    )
    assert json.loads(out.stdout) == [
        "-netdev",
        "user,id=net0,hostfwd=tcp:127.0.0.1:8080-:80",
        "-device",
        "virtio-net-pci,netdev=net0",
    ]
