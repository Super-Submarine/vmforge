#!/usr/bin/env python3
"""VMForge guest agent.

Runs inside the guest and services host requests over a virtio-serial port.
Protocol: newline-delimited JSON, QMP-style (see guest-tools/PROTOCOL.md):
  request:  {"execute": "<command>", "arguments": {...}, "id": <int>}
  success:  {"return": ..., "id": <int>}
  failure:  {"error": {"code": "<stable-code>", "message": "..."}, "id": <int>}

M1 contract commands (docs/interface-contracts.md §3):
  ping                    -> {}
  info                    -> {os, kernel, hostname, agent_version, ...}
  interfaces              -> [{name, mac, ips: [...]}, ...]
  shutdown {mode}         -> {}   (mode: powerdown|reboot|halt)
v1.1 lifecycle commands:
  net-info                -> {hostname, ips}
  exec {argv, ...}        -> {exit_code, stdout, stderr, ...} (base64 payloads)

Deprecated v0 aliases (guest-ping, guest-info, guest-get-host-info,
guest-network-get-interfaces, guest-shutdown) are still served.
"""

import argparse
import base64
import json
import os
import platform
import socket
import subprocess
import sys
import time

AGENT_VERSION = "1.1.0"
DEFAULT_PORT = "/dev/virtio-ports/org.vmforge.agent.0"

EXEC_DEFAULT_TIMEOUT = 30.0
EXEC_MAX_TIMEOUT = 300.0
EXEC_MAX_OUTPUT = 1024 * 1024  # per stream, bytes


class AgentError(Exception):
    """Protocol error with a stable machine-readable code."""

    def __init__(self, code, message):
        super().__init__(message)
        self.code = code
        self.message = message


def read_os_release():
    info = {}
    try:
        with open("/etc/os-release") as f:
            for line in f:
                line = line.strip()
                if "=" in line:
                    k, v = line.split("=", 1)
                    info[k] = v.strip('"')
    except OSError:
        pass
    return info


def get_ip_addr_json():
    """Parse `ip -j addr` (present on Alpine/Debian via iproute2)."""
    try:
        out = subprocess.run(
            ["ip", "-j", "addr"], capture_output=True, text=True, timeout=5
        )
        return json.loads(out.stdout)
    except (OSError, ValueError, subprocess.TimeoutExpired):
        return []


def contract_interfaces():
    """Contract §3 shape: [{name, mac, ips: [..]}]."""
    ifaces = []
    for it in get_ip_addr_json():
        ifaces.append(
            {
                "name": it.get("ifname"),
                "mac": it.get("address"),
                "ips": [a.get("local") for a in it.get("addr_info", []) if a.get("local")],
            }
        )
    return ifaces


def legacy_interfaces():
    """v0 shape kept for the deprecated guest-network-get-interfaces alias."""
    ifaces = []
    for it in get_ip_addr_json():
        ifaces.append(
            {
                "name": it.get("ifname"),
                "hardware-address": it.get("address"),
                "ip-addresses": [
                    {
                        "ip-address": a.get("local"),
                        "ip-address-type": "ipv6" if a.get("family") == "inet6" else "ipv4",
                        "prefix": a.get("prefixlen"),
                    }
                    for a in it.get("addr_info", [])
                ],
            }
        )
    return ifaces


def primary_ips():
    ips = []
    for iface in contract_interfaces():
        if iface["name"] == "lo":
            continue
        ips.extend(iface["ips"])
    return ips


def cmd_info():
    osr = read_os_release()
    return {
        "os": osr.get("PRETTY_NAME", platform.system()),
        "kernel": platform.release(),
        "hostname": socket.gethostname(),
        "agent_version": AGENT_VERSION,
        # additive extras (allowed within major version, contract G3)
        "arch": platform.machine(),
        "supported_commands": sorted(COMMANDS),
    }


def cmd_net_info():
    return {"hostname": socket.gethostname(), "ips": primary_ips()}


def cmd_shutdown(args):
    mode = args.get("mode", "powerdown")
    actions = {"powerdown": "poweroff", "halt": "halt", "reboot": "reboot"}
    if mode not in actions:
        raise AgentError("invalid_args", "unsupported shutdown mode: %s" % mode)
    # Delay slightly so the response reaches the host first.
    subprocess.Popen(
        ["sh", "-c", "sleep 1 && exec %s" % actions[mode]],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return {}


def cmd_exec(args):
    argv = args.get("argv")
    if not isinstance(argv, list) or not argv or not all(isinstance(a, str) for a in argv):
        raise AgentError("invalid_args", "exec requires argv: non-empty list of strings")
    timeout = args.get("timeout", EXEC_DEFAULT_TIMEOUT)
    if not isinstance(timeout, (int, float)) or timeout <= 0 or timeout > EXEC_MAX_TIMEOUT:
        raise AgentError(
            "invalid_args", "timeout must be in (0, %d] seconds" % int(EXEC_MAX_TIMEOUT)
        )
    stdin_data = b""
    if "stdin" in args:
        try:
            stdin_data = base64.b64decode(args["stdin"], validate=True)
        except (ValueError, TypeError):
            raise AgentError("invalid_args", "stdin must be base64") from None
    try:
        proc = subprocess.run(
            argv,
            input=stdin_data,
            capture_output=True,
            timeout=timeout,
            cwd=args.get("cwd"),
        )
    except FileNotFoundError:
        raise AgentError("exec_not_found", "no such executable: %s" % argv[0]) from None
    except subprocess.TimeoutExpired:
        raise AgentError("exec_timeout", "command exceeded %ss timeout" % timeout) from None
    except OSError as e:
        raise AgentError("exec_failed", str(e)) from None

    def clamp(data):
        return data[:EXEC_MAX_OUTPUT], len(data) > EXEC_MAX_OUTPUT

    out, out_trunc = clamp(proc.stdout)
    err, err_trunc = clamp(proc.stderr)
    return {
        "exit_code": proc.returncode,
        "stdout": base64.b64encode(out).decode(),
        "stderr": base64.b64encode(err).decode(),
        "stdout_truncated": out_trunc,
        "stderr_truncated": err_trunc,
    }


COMMANDS = {
    "ping": lambda args: {},
    "info": lambda args: cmd_info(),
    "interfaces": lambda args: contract_interfaces(),
    "net-info": lambda args: cmd_net_info(),
    "shutdown": cmd_shutdown,
    "exec": cmd_exec,
    # Deprecated v0 aliases.
    "guest-ping": lambda args: {},
    "guest-info": lambda args: {
        "version": AGENT_VERSION,
        "agent_version": AGENT_VERSION,
        "supported_commands": sorted(COMMANDS),
    },
    "guest-get-host-info": lambda args: dict(
        cmd_info(),
        **{"ip-addresses": primary_ips(), "agent-version": AGENT_VERSION, "time": time.time()},
    ),
    "guest-network-get-interfaces": lambda args: {"interfaces": legacy_interfaces()},
    "guest-shutdown": lambda args: dict(
        cmd_shutdown(args), mode=args.get("mode", "powerdown")
    ),
}


def handle(req):
    cmd = req.get("execute")
    args = req.get("arguments") or {}
    fn = COMMANDS.get(cmd)
    if fn is None:
        raise AgentError("unknown_command", "unknown command: %r" % cmd)
    return fn(args)


def process_line(line):
    """One request line -> one response dict. Never raises (contract G1)."""
    rid = None
    try:
        req = json.loads(line)
        if not isinstance(req, dict):
            raise AgentError("invalid_request", "request must be a JSON object")
        rid = req.get("id")
        resp = {"return": handle(req)}
    except AgentError as e:
        resp = {"error": {"code": e.code, "message": e.message, "desc": e.message}}
    except ValueError as e:
        resp = {"error": {"code": "invalid_request", "message": str(e), "desc": str(e)}}
    except Exception as e:  # noqa: BLE001 - report all errors to host, never crash
        resp = {"error": {"code": "internal_error", "message": str(e), "desc": str(e)}}
    if rid is not None:
        resp["id"] = rid
    return resp


def serve(port_path):
    while True:
        try:
            # O_RDWR on the virtio port; blocks until the host side connects.
            fd = os.open(port_path, os.O_RDWR)
        except OSError as e:
            sys.stderr.write("vmforge-ga: cannot open %s: %s\n" % (port_path, e))
            time.sleep(2)
            continue
        sys.stderr.write("vmforge-ga: serving on %s (agent %s)\n" % (port_path, AGENT_VERSION))
        buf = b""
        while True:
            try:
                chunk = os.read(fd, 4096)
            except OSError:
                break
            if not chunk:
                time.sleep(0.2)
                continue
            buf += chunk
            while b"\n" in buf:
                line, buf = buf.split(b"\n", 1)
                line = line.strip()
                if not line:
                    continue
                resp = process_line(line)
                try:
                    os.write(fd, (json.dumps(resp) + "\n").encode())
                except OSError:
                    break
        os.close(fd)


def main():
    ap = argparse.ArgumentParser(description="VMForge guest agent")
    ap.add_argument("--port", default=DEFAULT_PORT, help="virtio-serial port device path")
    args = ap.parse_args()
    serve(args.port)


if __name__ == "__main__":
    main()
