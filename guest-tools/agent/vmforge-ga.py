#!/usr/bin/env python3
"""VMForge guest agent v0.

Runs inside the guest and services host requests over a virtio-serial port.
Protocol: newline-delimited JSON, modeled on qemu-guest-agent (QMP-style):
  request:  {"execute": "<command>", "arguments": {...}, "id": <any>}
  success:  {"return": {...}, "id": <any>}
  failure:  {"error": {"desc": "..."}, "id": <any>}

Supported commands:
  guest-ping                     -> {}
  guest-info                     -> agent version + supported commands
  guest-get-host-info            -> hostname, os, kernel, ips
  guest-network-get-interfaces   -> per-interface addresses
  guest-shutdown {"mode": "powerdown"|"reboot"|"halt"}
"""

import argparse
import json
import os
import platform
import socket
import subprocess
import sys
import time

AGENT_VERSION = "0.1.0"
DEFAULT_PORT = "/dev/virtio-ports/org.vmforge.agent.0"


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


def get_interfaces():
    """Parse `ip -j addr` (present on Alpine/Debian via iproute2)."""
    try:
        out = subprocess.run(
            ["ip", "-j", "addr"], capture_output=True, text=True, timeout=5
        )
        data = json.loads(out.stdout)
    except (OSError, ValueError, subprocess.TimeoutExpired):
        return []
    ifaces = []
    for it in data:
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
    for iface in get_interfaces():
        if iface["name"] == "lo":
            continue
        for a in iface["ip-addresses"]:
            ips.append(a["ip-address"])
    return ips


SUPPORTED = [
    "guest-ping",
    "guest-info",
    "guest-get-host-info",
    "guest-network-get-interfaces",
    "guest-shutdown",
]


def handle(req):
    cmd = req.get("execute")
    args = req.get("arguments") or {}
    if cmd == "guest-ping":
        return {}
    if cmd == "guest-info":
        return {"version": AGENT_VERSION, "supported_commands": SUPPORTED}
    if cmd == "guest-get-host-info":
        osr = read_os_release()
        return {
            "hostname": socket.gethostname(),
            "os": osr.get("PRETTY_NAME", platform.system()),
            "os-id": osr.get("ID", ""),
            "kernel": platform.release(),
            "arch": platform.machine(),
            "ip-addresses": primary_ips(),
            "agent-version": AGENT_VERSION,
            "time": time.time(),
        }
    if cmd == "guest-network-get-interfaces":
        return {"interfaces": get_interfaces()}
    if cmd == "guest-shutdown":
        mode = args.get("mode", "powerdown")
        actions = {
            "powerdown": ["poweroff"],
            "halt": ["halt"],
            "reboot": ["reboot"],
        }
        if mode not in actions:
            raise ValueError("unsupported shutdown mode: %s" % mode)
        # Delay slightly so the response reaches the host first.
        subprocess.Popen(
            ["sh", "-c", "sleep 1 && exec %s" % actions[mode][0]],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        return {"mode": mode}
    raise ValueError("unknown command: %r" % cmd)


def serve(port_path):
    while True:
        try:
            # O_RDWR on the virtio port; blocks until the host side connects.
            fd = os.open(port_path, os.O_RDWR)
        except OSError as e:
            sys.stderr.write("vmforge-ga: cannot open %s: %s\n" % (port_path, e))
            time.sleep(2)
            continue
        sys.stderr.write("vmforge-ga: serving on %s\n" % port_path)
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
                rid = None
                try:
                    req = json.loads(line)
                    rid = req.get("id")
                    resp = {"return": handle(req)}
                except Exception as e:  # noqa: BLE001 - report all errors to host
                    resp = {"error": {"desc": str(e)}}
                if rid is not None:
                    resp["id"] = rid
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
