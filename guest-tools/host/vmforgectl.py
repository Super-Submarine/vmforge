#!/usr/bin/env python3
"""VMForge host-side client for the guest agent.

Usable both as a library (class GuestAgentClient) and a CLI:

    vmforgectl.py --socket /tmp/vmforge-ga.sock ping
    vmforgectl.py --socket /tmp/vmforge-ga.sock info
    vmforgectl.py --socket /tmp/vmforge-ga.sock host-info
    vmforgectl.py --socket /tmp/vmforge-ga.sock interfaces
    vmforgectl.py --socket /tmp/vmforge-ga.sock shutdown [--mode powerdown|reboot|halt]

The socket is the UNIX socket QEMU exposes for the virtio-serial channel.
The core engine must start QEMU with:

    -device virtio-serial-pci,id=vmforge-vs0
    -chardev socket,id=vmforge-ga0,path=/tmp/vmforge-ga.sock,server=on,wait=off
    -device virtserialport,bus=vmforge-vs0.0,chardev=vmforge-ga0,name=org.vmforge.agent.0

Inside the guest the channel appears as /dev/virtio-ports/org.vmforge.agent.0.
"""

import argparse
import json
import socket
import sys
import time


class GuestAgentError(Exception):
    pass


class GuestAgentTimeout(GuestAgentError):
    pass


class GuestAgentClient:
    def __init__(self, socket_path, timeout=10.0):
        self.socket_path = socket_path
        self.timeout = timeout
        self._id = 0

    def execute(self, command, arguments=None, timeout=None):
        """Send one command and wait for the matching response."""
        timeout = timeout if timeout is not None else self.timeout
        self._id += 1
        req = {"execute": command, "id": self._id}
        if arguments:
            req["arguments"] = arguments
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(timeout)
            s.connect(self.socket_path)
            s.sendall((json.dumps(req) + "\n").encode())
            buf = b""
            deadline = time.monotonic() + timeout
            while True:
                if time.monotonic() > deadline:
                    raise GuestAgentTimeout(
                        "no response to %s within %.1fs" % (command, timeout)
                    )
                try:
                    chunk = s.recv(4096)
                except socket.timeout:
                    raise GuestAgentTimeout(
                        "no response to %s within %.1fs" % (command, timeout)
                    )
                if not chunk:
                    raise GuestAgentError("connection closed by QEMU")
                buf += chunk
                while b"\n" in buf:
                    line, buf = buf.split(b"\n", 1)
                    if not line.strip():
                        continue
                    resp = json.loads(line)
                    if resp.get("id") != self._id:
                        continue  # stale response from a previous client
                    if "error" in resp:
                        raise GuestAgentError(resp["error"].get("desc", "unknown error"))
                    return resp.get("return", {})

    # Convenience wrappers -------------------------------------------------
    def ping(self, timeout=None):
        return self.execute("guest-ping", timeout=timeout)

    def info(self):
        return self.execute("guest-info")

    def host_info(self):
        return self.execute("guest-get-host-info")

    def interfaces(self):
        return self.execute("guest-network-get-interfaces")

    def shutdown(self, mode="powerdown"):
        return self.execute("guest-shutdown", {"mode": mode})

    def wait_ready(self, total_timeout=120.0, interval=2.0):
        """Poll guest-ping until the agent answers (e.g. during boot)."""
        deadline = time.monotonic() + total_timeout
        while time.monotonic() < deadline:
            try:
                self.ping(timeout=interval)
                return True
            except (GuestAgentError, OSError):
                time.sleep(interval)
        return False


def main():
    ap = argparse.ArgumentParser(description="VMForge guest-agent client")
    ap.add_argument("--socket", required=True, help="QEMU chardev UNIX socket path")
    ap.add_argument("--timeout", type=float, default=10.0)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("ping")
    sub.add_parser("info")
    sub.add_parser("host-info")
    sub.add_parser("interfaces")
    sp = sub.add_parser("shutdown")
    sp.add_argument("--mode", default="powerdown", choices=["powerdown", "reboot", "halt"])
    wp = sub.add_parser("wait-ready")
    wp.add_argument("--total-timeout", type=float, default=120.0)
    args = ap.parse_args()

    client = GuestAgentClient(args.socket, timeout=args.timeout)
    if args.cmd == "ping":
        out = client.ping()
    elif args.cmd == "info":
        out = client.info()
    elif args.cmd == "host-info":
        out = client.host_info()
    elif args.cmd == "interfaces":
        out = client.interfaces()
    elif args.cmd == "shutdown":
        out = client.shutdown(args.mode)
    elif args.cmd == "wait-ready":
        ok = client.wait_ready(total_timeout=args.total_timeout)
        out = {"ready": ok}
        if not ok:
            print(json.dumps(out))
            sys.exit(1)
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
