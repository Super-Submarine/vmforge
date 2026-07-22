"""Minimal QMP client for hot-adding/removing hostfwd rules.

QEMU has no native QMP command for user-mode hostfwd management, so we use
the QMP `human-monitor-command` passthrough to invoke the HMP commands
`hostfwd_add` / `hostfwd_remove` (supported for -netdev user backends).
"""

from __future__ import annotations

import json
import socket

from .config import PortForward


class QMPError(RuntimeError):
    """Raised when QMP negotiation or a command fails."""


class QMPClient:
    """Small synchronous QMP client over a UNIX or TCP socket."""

    def __init__(self, sock: socket.socket):
        self._sock = sock
        self._buf = b""
        self._negotiate()

    @classmethod
    def connect_unix(cls, path: str, timeout: float = 5.0) -> "QMPClient":
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(timeout)
        sock.connect(path)
        return cls(sock)

    @classmethod
    def connect_tcp(cls, host: str, port: int, timeout: float = 5.0) -> "QMPClient":
        sock = socket.create_connection((host, port), timeout=timeout)
        return cls(sock)

    def close(self) -> None:
        self._sock.close()

    def __enter__(self) -> "QMPClient":
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    def _read_json(self) -> dict:
        while b"\n" not in self._buf:
            chunk = self._sock.recv(4096)
            if not chunk:
                raise QMPError("QMP connection closed")
            self._buf += chunk
        line, self._buf = self._buf.split(b"\n", 1)
        return json.loads(line)

    def _negotiate(self) -> None:
        greeting = self._read_json()
        if "QMP" not in greeting:
            raise QMPError(f"unexpected QMP greeting: {greeting}")
        self.execute("qmp_capabilities")

    def execute(self, command: str, arguments: dict | None = None) -> object:
        msg: dict = {"execute": command}
        if arguments:
            msg["arguments"] = arguments
        self._sock.sendall(json.dumps(msg).encode() + b"\n")
        while True:
            resp = self._read_json()
            if "event" in resp:
                continue  # skip async events
            if "error" in resp:
                raise QMPError(f"{command}: {resp['error']}")
            if "return" in resp:
                return resp["return"]

    def hmp(self, command_line: str) -> str:
        """Run an HMP command via QMP passthrough; returns its text output."""
        out = self.execute(
            "human-monitor-command", {"command-line": command_line}
        )
        return str(out)

    def hostfwd_add(self, netdev_id: str, fwd: PortForward) -> None:
        out = self.hmp(f"hostfwd_add netdev {netdev_id} {fwd.to_hostfwd()}")
        if out.strip():
            raise QMPError(f"hostfwd_add failed: {out.strip()}")

    def hostfwd_remove(self, netdev_id: str, fwd: PortForward) -> None:
        spec = f"{fwd.proto}:{fwd.host_ip}:{fwd.host_port}"
        out = self.hmp(f"hostfwd_remove netdev {netdev_id} {spec}")
        if "removed" not in out and out.strip():
            raise QMPError(f"hostfwd_remove failed: {out.strip()}")
