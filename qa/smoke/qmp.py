#!/usr/bin/env python3
"""Minimal QMP client (stdlib only).

Usage:
    qmp.py SOCKET_PATH COMMAND [JSON_ARGS]
    qmp.py SOCKET_PATH human-monitor-command 'savevm smoke1'

Prints the JSON "return" value on success; exits 1 with the error on failure.
"""
import json
import socket
import sys


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    sock_path, command = sys.argv[1], sys.argv[2]

    if command == "human-monitor-command":
        arguments = {"command-line": sys.argv[3]}
    elif len(sys.argv) > 3:
        arguments = json.loads(sys.argv[3])
    else:
        arguments = {}

    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(120)
    s.connect(sock_path)
    f = s.makefile("rw", encoding="utf-8")

    json.loads(f.readline())  # greeting
    f.write(json.dumps({"execute": "qmp_capabilities"}) + "\n")
    f.flush()
    _read_return(f)

    f.write(json.dumps({"execute": command, "arguments": arguments}) + "\n")
    f.flush()
    ret = _read_return(f)
    print(json.dumps(ret))
    return 0


def _read_return(f):
    while True:
        msg = json.loads(f.readline())
        if "return" in msg:
            return msg["return"]
        if "error" in msg:
            print(json.dumps(msg["error"]), file=sys.stderr)
            sys.exit(1)
        # ignore async events


if __name__ == "__main__":
    sys.exit(main())
