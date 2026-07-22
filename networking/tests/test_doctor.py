"""Tests for `vmforge-net doctor`. No KVM, QEMU, or real guests required —
guest-side probes use a fake guest-exec bridge, host probes use fixture
paths, and network probes stay on loopback.
"""

from __future__ import annotations

import json
import os
import socket
import sys

import pytest

from vmforge_net import cli
from vmforge_net.doctor import (
    FAIL,
    PASS,
    SKIP,
    CheckResult,
    DoctorOptions,
    check_guest_dns,
    check_guest_to_host,
    check_guest_to_internet,
    check_mtu,
    check_port_forwards,
    check_tun,
    exit_code,
    load_vm_configs,
    render_table,
    run_doctor,
    to_json,
)


def opts(tmp_path, **kw) -> DoctorOptions:
    defaults = dict(
        home=tmp_path / "home",
        proc_root=tmp_path / "proc",
        sys_class_net=tmp_path / "sys-net",
        dev_net_tun=tmp_path / "tun-missing",
        bridge_helper_paths=(str(tmp_path / "no-helper"),),
        bridge_conf=tmp_path / "bridge.conf",
    )
    defaults.update(kw)
    return DoctorOptions(**defaults)


def write_vm_config(home, vm, data) -> None:
    vm_dir = home / "vms" / vm
    vm_dir.mkdir(parents=True, exist_ok=True)
    (vm_dir / "network.json").write_text(json.dumps(data))


# -- host prerequisites ------------------------------------------------------


def test_tun_missing_fails_with_hint(tmp_path):
    r = check_tun(opts(tmp_path))
    if sys.platform != "linux":
        assert r.status == SKIP
    else:
        assert r.status == FAIL
        assert "modprobe tun" in r.hint


@pytest.mark.skipif(sys.platform != "linux", reason="tun is Linux-only")
def test_tun_openable_passes(tmp_path):
    # Any rw-openable path stands in for /dev/net/tun.
    fake = tmp_path / "tun"
    fake.write_bytes(b"")
    r = check_tun(opts(tmp_path, dev_net_tun=fake))
    assert r.status == PASS


# -- config validity ---------------------------------------------------------


def test_config_valid_and_invalid_per_vm(tmp_path):
    o = opts(tmp_path)
    write_vm_config(o.home, "good", {"forwards": [{"host_port": 2222, "guest_port": 22}]})
    write_vm_config(o.home, "bad", {"netdev_id": "!!!bad id"})
    results: list[CheckResult] = []
    configs = load_vm_configs(o, results)
    statuses = {r.detail.split("/")[0]: r.status for r in results}
    assert statuses["good"] == PASS
    assert statuses["bad"] == FAIL
    assert len(configs) == 1
    assert configs[0][0] == "good"


def test_config_none_found_skips(tmp_path):
    results: list[CheckResult] = []
    load_vm_configs(opts(tmp_path), results)
    assert results[0].status == SKIP


def test_config_unknown_vm_fails(tmp_path):
    o = opts(tmp_path, vm="ghost")
    (o.home / "vms").mkdir(parents=True)
    results: list[CheckResult] = []
    load_vm_configs(o, results)
    assert results[0].status == FAIL
    assert "ghost" in results[0].detail


def test_config_explicit_file(tmp_path):
    cfg = tmp_path / "nat.json"
    cfg.write_text(json.dumps({"forwards": [{"host_port": 8080, "guest_port": 80}]}))
    o = opts(tmp_path, config=cfg)
    results: list[CheckResult] = []
    configs = load_vm_configs(o, results)
    assert results[0].status == PASS
    assert len(configs) == 1


# -- port-forward health -----------------------------------------------------


def test_forwards_none_skips(tmp_path):
    rs = check_port_forwards(opts(tmp_path), [])
    assert [r.status for r in rs] == [SKIP]


def test_forward_free_port_passes_when_vm_stopped(tmp_path):
    o = opts(tmp_path)
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        free_port = probe.getsockname()[1]
    write_vm_config(o.home, "vm1", {"forwards": [{"host_port": free_port, "guest_port": 22}]})
    results: list[CheckResult] = []
    configs = load_vm_configs(o, results)
    rs = check_port_forwards(o, configs)
    assert rs[0].status == PASS
    assert "host port free" in rs[0].detail


def test_forward_port_in_use_fails_when_vm_stopped(tmp_path):
    o = opts(tmp_path)
    with socket.socket() as held:
        held.bind(("127.0.0.1", 0))
        held.listen(1)
        port = held.getsockname()[1]
        write_vm_config(o.home, "vm1", {"forwards": [{"host_port": port, "guest_port": 22}]})
        results: list[CheckResult] = []
        configs = load_vm_configs(o, results)
        rs = check_port_forwards(o, configs)
    assert rs[0].status == FAIL
    assert "already in use" in rs[0].detail
    assert "ss -ltnp" in rs[0].hint


def test_forward_reachable_when_vm_running(tmp_path):
    o = opts(tmp_path)
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen(1)
        port = listener.getsockname()[1]
        write_vm_config(o.home, "vm1", {"forwards": [{"host_port": port, "guest_port": 22}]})
        # Mark vm1 running: pidfile with our own pid, resolved via proc_root.
        (o.home / "vms" / "vm1" / "qemu.pid").write_text(str(os.getpid()))
        (o.proc_root / str(os.getpid())).mkdir(parents=True)
        results: list[CheckResult] = []
        configs = load_vm_configs(o, results)
        rs = check_port_forwards(o, configs)
    assert rs[0].status == PASS
    assert "accepting connections" in rs[0].detail


def test_forward_unreachable_when_vm_running(tmp_path):
    o = opts(tmp_path)
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        dead_port = probe.getsockname()[1]
    write_vm_config(o.home, "vm1", {"forwards": [{"host_port": dead_port, "guest_port": 22}]})
    (o.home / "vms" / "vm1" / "qemu.pid").write_text(str(os.getpid()))
    (o.proc_root / str(os.getpid())).mkdir(parents=True)
    results: list[CheckResult] = []
    configs = load_vm_configs(o, results)
    rs = check_port_forwards(o, configs)
    assert rs[0].status == FAIL
    assert "connection refused" in rs[0].detail


def test_duplicate_host_port_across_vms_fails(tmp_path):
    o = opts(tmp_path)
    write_vm_config(o.home, "vm1", {"forwards": [{"host_port": 2222, "guest_port": 22}]})
    write_vm_config(o.home, "vm2", {"forwards": [{"host_port": 2222, "guest_port": 22}]})
    results: list[CheckResult] = []
    configs = load_vm_configs(o, results)
    rs = check_port_forwards(o, configs)
    assert any(r.status == FAIL and "duplicates" in r.detail for r in rs)


# -- guest-side probes -------------------------------------------------------


def fake_bridge(tmp_path, exit_code_: int, stderr: str = "") -> str:
    script = tmp_path / "fake-guest-exec.sh"
    script.write_text(
        f"#!/bin/sh\n[ -n \"{stderr}\" ] && echo \"{stderr}\" >&2\nexit {exit_code_}\n"
    )
    script.chmod(0o755)
    return str(script)


@pytest.mark.parametrize(
    "check", [check_guest_to_host, check_guest_to_internet, check_guest_dns]
)
def test_guest_probes_skip_without_bridge(tmp_path, check):
    r = check(opts(tmp_path))
    assert r.status == SKIP
    assert "--guest-exec" in r.hint


@pytest.mark.parametrize(
    "check", [check_guest_to_host, check_guest_to_internet, check_guest_dns]
)
def test_guest_probes_pass_with_working_bridge(tmp_path, check):
    r = check(opts(tmp_path, guest_exec=fake_bridge(tmp_path, 0)))
    assert r.status == PASS


def test_guest_probe_failure_carries_stderr_and_hint(tmp_path):
    o = opts(tmp_path, guest_exec=fake_bridge(tmp_path, 1, "no route to host"))
    r = check_guest_to_internet(o)
    assert r.status == FAIL
    assert "no route to host" in r.detail
    assert "restrict=on" in r.hint


def test_guest_probe_missing_bridge_command_skips(tmp_path):
    r = check_guest_dns(opts(tmp_path, guest_exec=str(tmp_path / "nope")))
    assert r.status == SKIP


# -- MTU / interface sanity --------------------------------------------------


def _route_fixture(tmp_path, iface="eth0", mtu="1500"):
    proc_net = tmp_path / "proc" / "net"
    proc_net.mkdir(parents=True, exist_ok=True)
    (proc_net / "route").write_text(
        "Iface\tDestination\tGateway\n"
        f"{iface}\t00000000\t0102A8C0\n"
    )
    sys_net = tmp_path / "sys-net" / iface
    sys_net.mkdir(parents=True, exist_ok=True)
    (sys_net / "mtu").write_text(mtu)


def test_mtu_sane_passes(tmp_path):
    _route_fixture(tmp_path)
    r = check_mtu(opts(tmp_path))
    assert r.status == PASS
    assert "eth0" in r.detail


def test_mtu_absurd_fails(tmp_path):
    _route_fixture(tmp_path, mtu="600")
    r = check_mtu(opts(tmp_path))
    assert r.status == FAIL
    assert "ip link set" in r.hint


def test_no_default_route_fails(tmp_path):
    proc_net = tmp_path / "proc" / "net"
    proc_net.mkdir(parents=True)
    (proc_net / "route").write_text("Iface\tDestination\tGateway\n")
    r = check_mtu(opts(tmp_path))
    assert r.status == FAIL


# -- CLI / output shapes -----------------------------------------------------


def test_cli_doctor_json_shape_and_exit(tmp_path, capsys):
    code = cli.main(["doctor", "--json", "--home", str(tmp_path / "home")])
    out = capsys.readouterr()
    assert "EXPERIMENTAL" in out.err
    doc = json.loads(out.out)
    assert doc["tool"] == "vmforge-net doctor"
    assert doc["stability"] == "experimental"
    assert doc["schema"] == 1
    assert {"pass", "fail", "skip"} <= set(doc["summary"])
    statuses = {c["status"] for c in doc["checks"]}
    assert statuses <= {"PASS", "FAIL", "SKIP"}
    for c in doc["checks"]:
        assert set(c) == {"id", "title", "status", "detail", "hint"}
        if c["status"] == "FAIL":
            assert c["hint"], f"FAIL without remediation hint: {c['id']}"
    assert code == (1 if doc["summary"]["fail"] else 0)


def test_cli_doctor_table_output(tmp_path, capsys):
    cli.main(["doctor", "--home", str(tmp_path / "home")])
    out = capsys.readouterr().out
    assert "RESULT" in out and "CHECK" in out
    assert "passed" in out and "skipped" in out


def test_exit_code_and_table_hints(tmp_path):
    results = [
        CheckResult("a", "A", PASS, "ok"),
        CheckResult("b", "B", FAIL, "broken", hint="fix it"),
    ]
    assert exit_code(results) == 1
    assert exit_code(results[:1]) == 0
    table = render_table(results)
    assert "hint: fix it" in table


def test_run_doctor_is_stable_and_json_serializable(tmp_path):
    results = run_doctor(opts(tmp_path))
    ids = [r.id for r in results]
    assert ids[0] == "host.tun"
    assert "nat.guest_to_host" in ids
    assert "dns.guest" in ids
    json.dumps(to_json(results))  # must not raise
