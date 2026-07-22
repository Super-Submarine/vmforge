"""vmforge-net CLI.

Subcommands:
    args           Print QEMU args for a NAT NIC (one per line, or shell-quoted).
    hostfwd-add    Hot-add a port forward on a running VM via QMP.
    hostfwd-remove Hot-remove a port forward on a running VM via QMP.
    doctor         Run guest-connectivity diagnostics (EXPERIMENTAL).
"""

from __future__ import annotations

import argparse
import json
import shlex
import sys
from pathlib import Path

from . import doctor as doctor_mod
from .config import NatConfig, PortForward
from .natgen import build_qemu_args
from .qmp import QMPClient, QMPError


def _load_config(args: argparse.Namespace) -> NatConfig:
    if args.config:
        with open(args.config) as f:
            cfg = NatConfig.from_dict(json.load(f))
    else:
        cfg = NatConfig()
    if args.netdev_id:
        cfg.netdev_id = args.netdev_id
    for spec in args.forward or []:
        cfg.forwards.append(PortForward.parse(spec))
    return cfg


def _connect(args: argparse.Namespace) -> QMPClient:
    if args.qmp_unix:
        return QMPClient.connect_unix(args.qmp_unix)
    if args.qmp_tcp:
        host, _, port = args.qmp_tcp.rpartition(":")
        return QMPClient.connect_tcp(host or "127.0.0.1", int(port))
    raise SystemExit("one of --qmp-unix or --qmp-tcp is required")


def _add_qmp_opts(p: argparse.ArgumentParser) -> None:
    p.add_argument("--qmp-unix", help="path to QMP UNIX socket")
    p.add_argument("--qmp-tcp", help="QMP TCP endpoint host:port")
    p.add_argument("--netdev-id", default="vmforge-nat0", help="netdev id of the NAT backend")
    p.add_argument("forward", help="forward spec: proto:hostip:hostport-guestip:guestport or hostport:guestport")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="vmforge-net", description=__doc__)
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_args = sub.add_parser("args", help="print QEMU args for a NAT NIC")
    p_args.add_argument("--config", help="path to JSON config file")
    p_args.add_argument("--netdev-id", help="override netdev id")
    p_args.add_argument(
        "--forward", "-f", action="append",
        help="add a forward: proto:hostip:hostport-guestip:guestport or hostport:guestport",
    )
    p_args.add_argument(
        "--format", choices=("shell", "lines", "json"), default="shell",
        help="output format (default: shell)",
    )

    p_add = sub.add_parser("hostfwd-add", help="hot-add a port forward via QMP")
    _add_qmp_opts(p_add)
    p_del = sub.add_parser("hostfwd-remove", help="hot-remove a port forward via QMP")
    _add_qmp_opts(p_del)

    p_doc = sub.add_parser(
        "doctor",
        help="run guest-connectivity diagnostics (EXPERIMENTAL)",
        description="Run host/VM networking checks with PASS/FAIL/SKIP "
        "results and remediation hints. Experimental surface: output "
        "shapes may change between wave-1 builds.",
    )
    p_doc.add_argument(
        "--home", type=Path, default=None,
        help="VMForge home (default: $VMFORGE_HOME or ~/.vmforge)",
    )
    p_doc.add_argument("--vm", help="scope per-VM checks to this VM name")
    p_doc.add_argument(
        "--config", type=Path, default=None,
        help="validate this NAT JSON config instead of scanning per-VM configs",
    )
    p_doc.add_argument(
        "--guest-exec", default=None, metavar="CMD",
        help="host command prefix that runs its argument inside the guest "
        "(e.g. 'vmforge guest exec myvm --'); enables guest-side NAT/DNS probes",
    )
    p_doc.add_argument(
        "--timeout", type=float, default=3.0,
        help="per-probe timeout in seconds (default: 3)",
    )
    p_doc.add_argument(
        "--json", action="store_true",
        help="emit one machine-readable JSON document on stdout",
    )

    args = parser.parse_args(argv)

    if args.cmd == "doctor":
        return _run_doctor(args)

    if args.cmd == "args":
        cfg = _load_config(args)
        qemu_args = build_qemu_args(cfg)
        if args.format == "json":
            print(json.dumps(qemu_args))
        elif args.format == "lines":
            print("\n".join(qemu_args))
        else:
            print(" ".join(shlex.quote(a) for a in qemu_args))
        return 0

    fwd = PortForward.parse(args.forward)
    try:
        with _connect(args) as qmp:
            if args.cmd == "hostfwd-add":
                qmp.hostfwd_add(args.netdev_id, fwd)
                print(f"added {fwd.to_hostfwd()} on {args.netdev_id}")
            else:
                qmp.hostfwd_remove(args.netdev_id, fwd)
                print(f"removed {fwd.proto}:{fwd.host_ip}:{fwd.host_port} on {args.netdev_id}")
    except (QMPError, OSError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    return 0


def _run_doctor(args: argparse.Namespace) -> int:
    print(
        "vmforge-net doctor is EXPERIMENTAL: not part of the wave-1 CLI "
        "freeze; output may change.",
        file=sys.stderr,
    )
    opts = doctor_mod.DoctorOptions(
        vm=args.vm,
        config=args.config,
        guest_exec=args.guest_exec,
        timeout=args.timeout,
    )
    if args.home is not None:
        opts.home = args.home
    results = doctor_mod.run_doctor(opts)
    if args.json:
        print(json.dumps(doctor_mod.to_json(results), indent=2))
    else:
        print(doctor_mod.render_table(results), end="")
    return doctor_mod.exit_code(results)


if __name__ == "__main__":
    sys.exit(main())
