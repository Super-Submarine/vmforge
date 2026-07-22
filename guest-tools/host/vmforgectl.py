#!/usr/bin/env python3
"""VMForge host-side client for the guest agent.

Usable both as a library (class GuestAgentClient) and a CLI. The socket is
the per-VM UNIX socket QEMU exposes for the virtio-serial channel
($VMFORGE_HOME/vms/<vm>/guest-agent.sock; see guest-tools/README.md for the
exact QEMU flags and guest-tools/PROTOCOL.md for the wire protocol):

    vmforgectl.py --vm myvm wait-ready            # poll until agent answers
    vmforgectl.py --vm myvm ping
    vmforgectl.py --vm myvm info                  # os/kernel/hostname/agent_version
    vmforgectl.py --vm myvm interfaces            # [{name, mac, ips}]
    vmforgectl.py --vm myvm net-info              # {hostname, ips}
    vmforgectl.py --vm myvm shutdown [--mode powerdown|reboot|halt] \
        [--wait --shutdown-timeout 60 --hard-stop-cmd 'kill -9 <qemu-pid>']
    vmforgectl.py --vm myvm exec -- uname -a

`--socket PATH` overrides the per-VM path for debugging.
"""

import argparse
import base64
import json
import os
import socket
import subprocess
import sys
import time

PROTOCOL_MAJOR = 1


def default_socket_path(vm_name):
    home = os.environ.get("VMFORGE_HOME") or os.path.expanduser("~/.vmforge")
    return os.path.join(home, "vms", vm_name, "guest-agent.sock")


class GuestAgentError(Exception):
    """code is the stable machine-readable error code (contract §0/§3)."""

    def __init__(self, message, code="unknown"):
        super().__init__(message)
        self.code = code


class GuestAgentTimeout(GuestAgentError):
    def __init__(self, message):
        super().__init__(message, code="timeout")


class GuestAgentIncompatible(GuestAgentError):
    """Agent speaks an incompatible protocol major version."""

    def __init__(self, message):
        super().__init__(message, code="incompatible_agent")


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
                    ) from None
                if not chunk:
                    raise GuestAgentError("connection closed by QEMU", code="disconnected")
                buf += chunk
                while b"\n" in buf:
                    line, buf = buf.split(b"\n", 1)
                    if not line.strip():
                        continue
                    resp = json.loads(line)
                    if resp.get("id") != self._id:
                        continue  # stale response from a previous client
                    if "error" in resp:
                        err = resp["error"]
                        raise GuestAgentError(
                            err.get("message") or err.get("desc") or "unknown error",
                            code=err.get("code", "unknown"),
                        )
                    return resp.get("return", {})

    # Contract commands ----------------------------------------------------
    def ping(self, timeout=None):
        return self.execute("ping", timeout=timeout)

    def info(self):
        return self.execute("info")

    def interfaces(self):
        return self.execute("interfaces")

    def net_info(self):
        return self.execute("net-info")

    def shutdown(self, mode="powerdown"):
        return self.execute("shutdown", {"mode": mode})

    def exec_in_guest(self, argv, timeout=30.0, stdin=None, cwd=None, decode=True):
        """Run argv in the guest; returns {exit_code, stdout, stderr, ...}.

        stdout/stderr are base64 on the wire; decode=True replaces them with
        UTF-8 text (errors replaced).
        """
        args = {"argv": list(argv), "timeout": timeout}
        if stdin is not None:
            args["stdin"] = base64.b64encode(
                stdin if isinstance(stdin, bytes) else stdin.encode()
            ).decode()
        if cwd is not None:
            args["cwd"] = cwd
        # The agent-side timeout also bounds our socket wait, plus slack.
        out = self.execute("exec", args, timeout=timeout + 10.0)
        if decode:
            for key in ("stdout", "stderr"):
                out[key] = base64.b64decode(out.get(key, "")).decode(
                    "utf-8", errors="replace"
                )
        return out

    def check_protocol(self):
        """Verify the agent speaks a compatible protocol; returns its info.

        Older (v0) agents either lack `agent_version` in `info` or reject the
        contract `info` command entirely — both fail gracefully here with
        GuestAgentIncompatible rather than a confusing downstream error.
        """
        try:
            info = self.info()
        except GuestAgentError as e:
            if e.code in ("unknown_command", "unknown"):
                raise GuestAgentIncompatible(
                    "agent does not support the `info` command (v0 agent?); "
                    "upgrade guest tools"
                ) from None
            raise
        version = info.get("agent_version")
        if not version:
            raise GuestAgentIncompatible(
                "agent did not report agent_version (v0 agent?); upgrade guest tools"
            )
        try:
            major = int(str(version).split(".")[0])
        except ValueError:
            raise GuestAgentIncompatible(
                "unparseable agent_version: %r" % version
            ) from None
        if major > PROTOCOL_MAJOR:
            raise GuestAgentIncompatible(
                "agent protocol major %d is newer than client major %d"
                % (major, PROTOCOL_MAJOR)
            )
        return info

    def wait_ready(self, total_timeout=120.0, interval=2.0):
        """Poll ping until the agent answers (e.g. during boot)."""
        deadline = time.monotonic() + total_timeout
        while time.monotonic() < deadline:
            try:
                self.ping(timeout=interval)
                return True
            except (GuestAgentError, OSError):
                time.sleep(interval)
        return False

    def wait_down(self, total_timeout=60.0, interval=1.0):
        """Wait until the agent stops answering ping (guest going down)."""
        deadline = time.monotonic() + total_timeout
        while time.monotonic() < deadline:
            try:
                self.ping(timeout=interval)
            except (GuestAgentError, OSError):
                return True
            time.sleep(interval)
        return False

    def shutdown_graceful(self, mode="powerdown", timeout=60.0, hard_stop=None):
        """Graceful shutdown with timeout + hard-stop fallback.

        Sends `shutdown {mode}`, then waits until the agent goes silent. If
        the guest is still up after `timeout` seconds (or the shutdown command
        itself fails), calls `hard_stop()` (e.g. QMP `quit` / SIGKILL of the
        QEMU process — the engine owns that) and reports how it ended.

        Returns {"graceful": bool, "hard_stopped": bool}.
        """
        sent = False
        try:
            self.shutdown(mode=mode)
            sent = True
        except (GuestAgentError, OSError):
            pass
        if sent and self.wait_down(total_timeout=timeout):
            return {"graceful": True, "hard_stopped": False}
        if hard_stop is not None:
            hard_stop()
            return {"graceful": False, "hard_stopped": True}
        return {"graceful": False, "hard_stopped": False}


def main():
    ap = argparse.ArgumentParser(description="VMForge guest-agent client")
    tgt = ap.add_mutually_exclusive_group(required=True)
    tgt.add_argument("--vm", help="VM name; uses $VMFORGE_HOME/vms/<vm>/guest-agent.sock")
    tgt.add_argument("--socket", help="explicit chardev UNIX socket path (debugging)")
    ap.add_argument("--timeout", type=float, default=10.0)
    ap.add_argument(
        "--skip-version-check",
        action="store_true",
        help="do not verify agent protocol compatibility first",
    )
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("ping")
    sub.add_parser("info")
    sub.add_parser("interfaces")
    sub.add_parser("net-info")
    sp = sub.add_parser("shutdown")
    sp.add_argument("--mode", default="powerdown", choices=["powerdown", "reboot", "halt"])
    sp.add_argument("--wait", action="store_true", help="wait for the guest to go down")
    sp.add_argument("--shutdown-timeout", type=float, default=60.0)
    sp.add_argument(
        "--hard-stop-cmd",
        help="shell command to run if the guest is still up after the timeout "
        "(e.g. a QMP quit or kill of the QEMU process)",
    )
    ep = sub.add_parser("exec")
    ep.add_argument("--exec-timeout", type=float, default=30.0)
    ep.add_argument("--cwd")
    ep.add_argument("argv", nargs="+", help="command and arguments (prefix with --)")
    wp = sub.add_parser("wait-ready")
    wp.add_argument("--total-timeout", type=float, default=120.0)
    args = ap.parse_args()

    sock = args.socket or default_socket_path(args.vm)
    client = GuestAgentClient(sock, timeout=args.timeout)

    try:
        if args.cmd not in ("ping", "wait-ready") and not args.skip_version_check:
            client.check_protocol()
        if args.cmd == "ping":
            out = client.ping()
        elif args.cmd == "info":
            out = client.info()
        elif args.cmd == "interfaces":
            out = client.interfaces()
        elif args.cmd == "net-info":
            out = client.net_info()
        elif args.cmd == "shutdown":
            if args.wait or args.hard_stop_cmd:
                hard_stop = None
                if args.hard_stop_cmd:
                    hard_stop = lambda: subprocess.run(  # noqa: E731
                        ["sh", "-c", args.hard_stop_cmd], check=False
                    )
                out = client.shutdown_graceful(
                    mode=args.mode, timeout=args.shutdown_timeout, hard_stop=hard_stop
                )
                if not out["graceful"] and not out["hard_stopped"]:
                    print(json.dumps(out))
                    sys.exit(1)
            else:
                out = client.shutdown(args.mode)
        elif args.cmd == "exec":
            out = client.exec_in_guest(
                args.argv, timeout=args.exec_timeout, cwd=args.cwd
            )
            sys.stdout.write(out["stdout"])
            sys.stderr.write(out["stderr"])
            sys.exit(out["exit_code"])
        elif args.cmd == "wait-ready":
            ok = client.wait_ready(total_timeout=args.total_timeout)
            out = {"ready": ok}
            if not ok:
                print(json.dumps(out))
                sys.exit(1)
    except GuestAgentError as e:
        print(
            json.dumps({"error": {"code": e.code, "message": str(e)}}),
            file=sys.stderr,
        )
        sys.exit(1)
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
