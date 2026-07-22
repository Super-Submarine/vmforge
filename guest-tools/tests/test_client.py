"""Host-client tests against an in-process fake agent over a real UNIX socket."""

import json
import socket
import threading

import pytest
from conftest import load_agent, load_client

agent = load_agent()
ctl = load_client()


class FakeAgentServer:
    """Serves the real agent's process_line over a UNIX socket, like QEMU
    bridges the chardev. handler can be overridden to fake old agents."""

    def __init__(self, sock_path, handler=None):
        self.sock_path = str(sock_path)
        self.handler = handler or (lambda line: agent.process_line(line))
        self._srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._srv.bind(self.sock_path)
        self._srv.listen(4)
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._serve, daemon=True)
        self._thread.start()

    def _serve(self):
        self._srv.settimeout(0.2)
        while not self._stop.is_set():
            try:
                conn, _ = self._srv.accept()
            except socket.timeout:
                continue
            with conn:
                buf = b""
                conn.settimeout(2)
                try:
                    while b"\n" not in buf:
                        chunk = conn.recv(4096)
                        if not chunk:
                            break
                        buf += chunk
                    for line in buf.split(b"\n"):
                        if line.strip():
                            resp = self.handler(line)
                            conn.sendall((json.dumps(resp) + "\n").encode())
                except OSError:
                    pass

    def close(self):
        self._stop.set()
        self._thread.join()
        self._srv.close()


@pytest.fixture
def server(tmp_path):
    srv = FakeAgentServer(tmp_path / "guest-agent.sock")
    yield srv
    srv.close()


def make_client(srv, timeout=5.0):
    return ctl.GuestAgentClient(srv.sock_path, timeout=timeout)


def test_ping_roundtrip(server):
    assert make_client(server).ping() == {}


def test_check_protocol_accepts_current_agent(server):
    info = make_client(server).check_protocol()
    assert info["agent_version"] == agent.AGENT_VERSION


def test_error_code_surfaced(server):
    client = make_client(server)
    with pytest.raises(ctl.GuestAgentError) as e:
        client.execute("no-such-cmd")
    assert e.value.code == "unknown_command"


def test_exec_in_guest_decodes_output(server):
    out = make_client(server).exec_in_guest(["sh", "-c", "echo hi; echo err >&2; exit 2"])
    assert out == {
        "exit_code": 2,
        "stdout": "hi\n",
        "stderr": "err\n",
        "stdout_truncated": False,
        "stderr_truncated": False,
    }


def test_check_protocol_rejects_v0_agent_without_info(tmp_path):
    """v0 agents answer `info` with a legacy no-code error."""

    def v0_handler(line):
        req = json.loads(line)
        return {"error": {"desc": "unknown command: 'info'"}, "id": req.get("id")}

    srv = FakeAgentServer(tmp_path / "v0.sock", handler=v0_handler)
    try:
        with pytest.raises(ctl.GuestAgentIncompatible):
            make_client(srv).check_protocol()
    finally:
        srv.close()


def test_check_protocol_rejects_missing_agent_version(tmp_path):
    def handler(line):
        req = json.loads(line)
        return {"return": {"hostname": "x"}, "id": req.get("id")}

    srv = FakeAgentServer(tmp_path / "nover.sock", handler=handler)
    try:
        with pytest.raises(ctl.GuestAgentIncompatible):
            make_client(srv).check_protocol()
    finally:
        srv.close()


def test_check_protocol_rejects_newer_major(tmp_path):
    def handler(line):
        req = json.loads(line)
        return {"return": {"agent_version": "99.0.0"}, "id": req.get("id")}

    srv = FakeAgentServer(tmp_path / "future.sock", handler=handler)
    try:
        with pytest.raises(ctl.GuestAgentIncompatible):
            make_client(srv).check_protocol()
    finally:
        srv.close()


def test_shutdown_graceful_hard_stop_fallback(tmp_path):
    """Agent acks shutdown but never goes down -> hard_stop fires."""

    def handler(line):
        req = json.loads(line)
        cmd = req.get("execute")
        if cmd in ("shutdown", "ping"):
            return {"return": {}, "id": req.get("id")}
        return agent.process_line(line)

    srv = FakeAgentServer(tmp_path / "stuck.sock", handler=handler)
    hard_stopped = []
    try:
        result = make_client(srv, timeout=2.0).shutdown_graceful(
            timeout=2.0, hard_stop=lambda: hard_stopped.append(True)
        )
    finally:
        srv.close()
    assert result == {"graceful": False, "hard_stopped": True}
    assert hard_stopped == [True]


def test_shutdown_graceful_success(tmp_path):
    """Agent acks shutdown then goes silent -> graceful."""
    state = {"down": False}

    def handler(line):
        req = json.loads(line)
        if req.get("execute") == "shutdown":
            state["down"] = True
            return {"return": {}, "id": req.get("id")}
        if state["down"]:
            return {"error": {"code": "internal_error", "message": "down"},
                    "id": req.get("id")}
        return {"return": {}, "id": req.get("id")}

    srv = FakeAgentServer(tmp_path / "ok.sock", handler=handler)
    try:
        result = make_client(srv, timeout=2.0).shutdown_graceful(timeout=5.0)
    finally:
        srv.close()
    assert result == {"graceful": True, "hard_stopped": False}


def test_default_socket_path(monkeypatch):
    monkeypatch.setenv("VMFORGE_HOME", "/srv/vmf")
    assert ctl.default_socket_path("vm1") == "/srv/vmf/vms/vm1/guest-agent.sock"
    monkeypatch.delenv("VMFORGE_HOME")
    assert ctl.default_socket_path("vm1").endswith("/.vmforge/vms/vm1/guest-agent.sock")
