"""`vmforge-net doctor`: guest-connectivity diagnostics.

Runs a series of host/VM networking checks and reports each as PASS, FAIL,
or SKIP with a remediation hint. Designed for wave-1 beta triage: everything
here is best-effort and read-only — no host or VM state is modified except
transient bind/connect probes on loopback.

Surface stability: EXPERIMENTAL (vmforge-net is not in the wave-1 CLI
freeze; see docs/cli-freeze-v1.0-beta.md §4).
"""

from __future__ import annotations

import errno
import json
import os
import shlex
import shutil
import socket
import stat
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

from .config import NatConfig

PASS = "PASS"
FAIL = "FAIL"
SKIP = "SKIP"

SCHEMA_VERSION = 1
STABILITY = "experimental"

#: Gateway address of the QEMU user-mode (SLIRP) stack, as seen from a guest.
SLIRP_HOST_GATEWAY = "10.0.2.2"
#: Built-in DNS forwarder of the SLIRP stack, as seen from a guest.
SLIRP_DNS = "10.0.2.3"

#: Sane MTU bounds: IPv6 minimum link MTU .. largest sensible jumbo frame.
MTU_MIN = 1280
MTU_MAX = 65536

_BRIDGE_HELPER_PATHS = (
    "/usr/lib/qemu/qemu-bridge-helper",
    "/usr/libexec/qemu-bridge-helper",
    "/usr/lib/qemu-bridge-helper",
)

_NETWORK_CONFIG_NAMES = ("network.json", "net.json")


@dataclass
class CheckResult:
    """Outcome of a single doctor check."""

    id: str
    title: str
    status: str
    detail: str = ""
    hint: str = ""

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "title": self.title,
            "status": self.status,
            "detail": self.detail,
            "hint": self.hint,
        }


@dataclass
class DoctorOptions:
    """Inputs to a doctor run (all optional; defaults probe the live host)."""

    home: Path = field(default_factory=lambda: _default_home())
    vm: str | None = None
    config: Path | None = None
    guest_exec: str | None = None
    timeout: float = 3.0
    proc_root: Path = Path("/proc")
    sys_class_net: Path = Path("/sys/class/net")
    dev_net_tun: Path = Path("/dev/net/tun")
    bridge_helper_paths: tuple[str, ...] = _BRIDGE_HELPER_PATHS
    bridge_conf: Path = Path("/etc/qemu/bridge.conf")


def _default_home() -> Path:
    env = os.environ.get("VMFORGE_HOME")
    return Path(env) if env else Path.home() / ".vmforge"


def run_doctor(opts: DoctorOptions) -> list[CheckResult]:
    """Run all checks and return their results in a stable order."""
    results: list[CheckResult] = []
    results.append(check_tun(opts))
    results.append(check_bridge_helper(opts))
    results.append(check_nat_firewall(opts))
    configs = load_vm_configs(opts, results)
    results.append(check_mtu(opts))
    results.extend(check_port_forwards(opts, configs))
    results.append(check_guest_to_host(opts))
    results.append(check_guest_to_internet(opts))
    results.append(check_guest_dns(opts))
    return results


# ---------------------------------------------------------------------------
# Host prerequisites
# ---------------------------------------------------------------------------


def check_tun(opts: DoctorOptions) -> CheckResult:
    cid, title = "host.tun", "tun/tap device availability"
    if sys.platform != "linux":
        return CheckResult(
            cid, title, SKIP, f"not a Linux host ({sys.platform})",
            "tun/tap checks only apply to Linux hosts",
        )
    tun = opts.dev_net_tun
    if not tun.exists():
        return CheckResult(
            cid, title, FAIL, f"{tun} does not exist",
            "load the tun module: sudo modprobe tun "
            "(persist via /etc/modules-load.d/); user-mode (SLIRP) NAT "
            "still works without it",
        )
    try:
        fd = os.open(tun, os.O_RDWR)
        os.close(fd)
    except OSError as e:
        return CheckResult(
            cid, title, FAIL, f"{tun} exists but cannot be opened: {e.strerror}",
            "grant your user access, e.g. add a udev rule or run: "
            f"sudo chmod 0666 {tun} (or use user-mode NAT, which needs no tun)",
        )
    return CheckResult(cid, title, PASS, f"{tun} present and openable read/write")


def check_bridge_helper(opts: DoctorOptions) -> CheckResult:
    cid, title = "host.bridge_helper", "bridge helper permissions"
    helper = next((p for p in opts.bridge_helper_paths if Path(p).is_file()), None)
    if helper is None:
        return CheckResult(
            cid, title, SKIP, "qemu-bridge-helper not installed",
            "only needed for bridged/TAP mode; install the qemu-system "
            "package (Debian/Ubuntu: qemu-system-common) to use it",
        )
    st = Path(helper).stat()
    setuid = bool(st.st_mode & stat.S_ISUID)
    caps = ""
    getcap = shutil.which("getcap")
    if getcap:
        try:
            out = subprocess.run(
                [getcap, helper], capture_output=True, text=True,
                timeout=opts.timeout,
            )
            caps = out.stdout.strip()
        except (OSError, subprocess.SubprocessError):
            caps = ""
    if setuid or "cap_net_admin" in caps:
        detail = f"{helper}: " + ("setuid root" if setuid else caps)
        if not opts.bridge_conf.exists():
            return CheckResult(
                cid, title, FAIL,
                detail + f"; {opts.bridge_conf} missing",
                f"create it, e.g.: echo 'allow br0' | sudo tee {opts.bridge_conf}",
            )
        return CheckResult(cid, title, PASS, detail + f"; {opts.bridge_conf} present")
    return CheckResult(
        cid, title, FAIL,
        f"{helper} present but neither setuid root nor cap_net_admin",
        f"sudo chmod u+s {helper} — or setcap cap_net_admin+ep — so "
        "unprivileged VMs can attach TAP devices to a bridge",
    )


def check_nat_firewall(opts: DoctorOptions) -> CheckResult:
    cid, title = "host.nat_firewall", "iptables/nftables presence (NAT)"
    found = [b for b in ("nft", "iptables") if shutil.which(b)]
    if found:
        return CheckResult(cid, title, PASS, f"found: {', '.join(found)}")
    return CheckResult(
        cid, title, FAIL, "neither nft nor iptables found on PATH",
        "install nftables (or iptables); required for routed/bridged NAT "
        "masquerading — user-mode (SLIRP) NAT works without it",
    )


# ---------------------------------------------------------------------------
# Per-VM network config
# ---------------------------------------------------------------------------


def load_vm_configs(
    opts: DoctorOptions, results: list[CheckResult]
) -> list[tuple[str, NatConfig]]:
    """Validate per-VM network configs, appending one result per source.

    Returns the configs that parsed, tagged with their VM name (or the
    config path when `--config` was given explicitly).
    """
    cid, title = "config.valid", "per-VM network config validity"
    configs: list[tuple[str, NatConfig]] = []

    if opts.config is not None:
        name = str(opts.config)
        try:
            cfg = NatConfig.from_dict(json.loads(opts.config.read_text()))
            configs.append((name, cfg))
            results.append(CheckResult(cid, title, PASS, f"{name}: valid"))
        except (OSError, ValueError, KeyError, TypeError) as e:
            results.append(CheckResult(
                cid, title, FAIL, f"{name}: {e}",
                "fix the JSON config; see networking/examples/nat.json "
                "for the expected shape",
            ))
        return configs

    vms_dir = opts.home / "vms"
    vm_dirs = []
    if vms_dir.is_dir():
        vm_dirs = sorted(
            d for d in vms_dir.iterdir()
            if d.is_dir() and (opts.vm is None or d.name == opts.vm)
        )
    if opts.vm is not None and not vm_dirs:
        results.append(CheckResult(
            cid, title, FAIL, f"unknown VM {opts.vm!r} under {vms_dir}",
            "check the VM name (vmforge list) or pass --home",
        ))
        return configs
    found_any = False
    for vm_dir in vm_dirs:
        for cfg_name in _NETWORK_CONFIG_NAMES:
            path = vm_dir / cfg_name
            if not path.is_file():
                continue
            found_any = True
            try:
                cfg = NatConfig.from_dict(json.loads(path.read_text()))
                configs.append((vm_dir.name, cfg))
                results.append(CheckResult(
                    cid, title, PASS, f"{vm_dir.name}/{cfg_name}: valid",
                ))
            except (OSError, ValueError, KeyError, TypeError) as e:
                results.append(CheckResult(
                    cid, title, FAIL, f"{vm_dir.name}/{cfg_name}: {e}",
                    "fix the JSON config; see networking/examples/nat.json "
                    "for the expected shape",
                ))
            break
    if not found_any:
        results.append(CheckResult(
            cid, title, SKIP,
            f"no per-VM network config found under {vms_dir}",
            "VMs without a network.json use engine defaults; pass --config "
            "to validate a specific file",
        ))
    return configs


# ---------------------------------------------------------------------------
# MTU / interface sanity
# ---------------------------------------------------------------------------


def check_mtu(opts: DoctorOptions) -> CheckResult:
    cid, title = "host.mtu", "MTU/interface sanity"
    route = opts.proc_root / "net" / "route"
    if not route.is_file():
        return CheckResult(
            cid, title, SKIP, f"{route} unavailable",
            "MTU check requires Linux procfs",
        )
    default_iface = None
    try:
        for line in route.read_text().splitlines()[1:]:
            fields = line.split()
            if len(fields) >= 2 and fields[1] == "00000000":
                default_iface = fields[0]
                break
    except OSError as e:
        return CheckResult(cid, title, SKIP, f"cannot read {route}: {e.strerror}")
    if default_iface is None:
        return CheckResult(
            cid, title, FAIL, "no default route on the host",
            "guest internet access (NAT) needs host connectivity; check "
            "the host network connection",
        )
    mtu_path = opts.sys_class_net / default_iface / "mtu"
    try:
        mtu = int(mtu_path.read_text().strip())
    except (OSError, ValueError):
        return CheckResult(
            cid, title, SKIP, f"default route via {default_iface}; MTU unreadable",
        )
    if MTU_MIN <= mtu <= MTU_MAX:
        return CheckResult(
            cid, title, PASS, f"default route via {default_iface}, mtu {mtu}",
        )
    return CheckResult(
        cid, title, FAIL, f"default route via {default_iface}, mtu {mtu}",
        f"MTU outside sane bounds [{MTU_MIN}, {MTU_MAX}]; large-packet "
        f"traffic will stall — set it back, e.g.: "
        f"sudo ip link set {default_iface} mtu 1500",
    )


# ---------------------------------------------------------------------------
# Port-forward rule health
# ---------------------------------------------------------------------------


def check_port_forwards(
    opts: DoctorOptions, configs: list[tuple[str, NatConfig]]
) -> list[CheckResult]:
    cid, title = "forwards.health", "port-forward rule health"
    results: list[CheckResult] = []
    all_forwards = [
        (vm, cfg, fwd) for vm, cfg in configs for fwd in cfg.forwards
    ]
    if not all_forwards:
        return [CheckResult(
            cid, title, SKIP, "no port-forward rules configured",
            "add forwards to the VM's network.json (or --config) to check them",
        )]
    seen: dict[tuple[str, str, int], str] = {}
    for vm, cfg, fwd in all_forwards:
        key = (fwd.proto, fwd.host_ip, fwd.host_port)
        rule = fwd.to_hostfwd()
        if key in seen and seen[key] != vm:
            results.append(CheckResult(
                cid, title, FAIL,
                f"{vm}: {rule} duplicates a rule of VM {seen[key]!r}",
                "two VMs cannot bind the same host proto/ip/port; change "
                "one host_port",
            ))
            continue
        seen[key] = vm
        running = _vm_running(opts, vm)
        if fwd.proto != "tcp":
            results.append(CheckResult(
                cid, title, PASS, f"{vm}: {rule} (rule present; UDP not probed)",
            ))
            continue
        if running:
            ok, why = _tcp_connect(fwd.host_ip, fwd.host_port, opts.timeout)
            if ok:
                results.append(CheckResult(
                    cid, title, PASS, f"{vm}: {rule} — forward accepting connections",
                ))
            else:
                results.append(CheckResult(
                    cid, title, FAIL, f"{vm}: {rule} — {why}",
                    "VM is running but the forward is not reachable: check "
                    "that QEMU was started with this hostfwd rule and that "
                    "the guest service is listening on the guest port",
                ))
        else:
            bindable, why = _port_bindable(fwd.host_ip, fwd.host_port)
            if bindable:
                results.append(CheckResult(
                    cid, title, PASS,
                    f"{vm}: {rule} — host port free (VM not running)",
                ))
            else:
                results.append(CheckResult(
                    cid, title, FAIL,
                    f"{vm}: {rule} — host port not bindable: {why}",
                    "another process holds this port; find it with "
                    f"`ss -ltnp 'sport = :{fwd.host_port}'` or pick a "
                    "different host_port",
                ))
    return results


def _vm_running(opts: DoctorOptions, vm: str) -> bool:
    """Best-effort running probe: same pidfile convention as `vmforge diagnose`."""
    vm_dir = opts.home / "vms" / vm
    for pidfile in (vm_dir / "run" / "qemu.pid", vm_dir / "qemu.pid"):
        try:
            pid = pidfile.read_text().strip()
        except OSError:
            continue
        return bool(pid) and (opts.proc_root / pid).exists()
    return False


def _port_bindable(host_ip: str, port: int) -> tuple[bool, str]:
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            s.bind((host_ip or "127.0.0.1", port))
        return True, ""
    except OSError as e:
        if e.errno == errno.EADDRINUSE:
            return False, "address already in use"
        return False, e.strerror or str(e)


def _tcp_connect(host_ip: str, port: int, timeout: float) -> tuple[bool, str]:
    try:
        with socket.create_connection((host_ip or "127.0.0.1", port), timeout):
            return True, ""
    except OSError as e:
        if isinstance(e, socket.timeout) or e.errno == errno.ETIMEDOUT:
            return False, "connection timed out (rule missing or guest silent)"
        if e.errno == errno.ECONNREFUSED:
            return False, "connection refused (no listener on the host port)"
        return False, e.strerror or str(e)


# ---------------------------------------------------------------------------
# Guest-side probes (need a guest-exec bridge)
# ---------------------------------------------------------------------------


def _guest_probe(
    opts: DoctorOptions, cid: str, title: str, probe: str, skip_hint: str,
    fail_hint: str, pass_detail: str,
) -> CheckResult:
    if not opts.guest_exec:
        return CheckResult(
            cid, title, SKIP, "no guest-exec bridge configured",
            skip_hint,
        )
    cmd = shlex.split(opts.guest_exec) + [probe]
    try:
        proc = subprocess.run(
            cmd, capture_output=True, text=True, timeout=opts.timeout * 4,
        )
    except FileNotFoundError:
        return CheckResult(
            cid, title, SKIP, f"guest-exec command not found: {cmd[0]}",
            "pass a working --guest-exec command",
        )
    except subprocess.TimeoutExpired:
        return CheckResult(
            cid, title, FAIL, "guest probe timed out",
            "guest agent unresponsive; check the VM is booted and the "
            "guest agent is running",
        )
    if proc.returncode == 0:
        return CheckResult(cid, title, PASS, pass_detail)
    detail = (proc.stderr.strip() or proc.stdout.strip() or
              f"probe exited {proc.returncode}")
    return CheckResult(cid, title, FAIL, detail, fail_hint)


def check_guest_to_host(opts: DoctorOptions) -> CheckResult:
    probe = (
        "sh -c 'exec 3<>/dev/tcp/%s/80 || ping -c1 -W2 %s'"
        % (SLIRP_HOST_GATEWAY, SLIRP_HOST_GATEWAY)
    )
    return _guest_probe(
        opts, "nat.guest_to_host", "NAT reachability: guest -> host",
        probe,
        skip_hint=("start the VM's guest agent and pass "
                   "--guest-exec 'vmforge guest exec <vm> --' to probe "
                   f"the SLIRP gateway {SLIRP_HOST_GATEWAY} from the guest"),
        fail_hint=(f"guest cannot reach the host gateway {SLIRP_HOST_GATEWAY}; "
                   "check the guest got a DHCP lease (10.0.2.15 by default) "
                   "and that the NIC is user-mode (SLIRP) NAT"),
        pass_detail=f"guest reached SLIRP gateway {SLIRP_HOST_GATEWAY}",
    )


def check_guest_to_internet(opts: DoctorOptions) -> CheckResult:
    probe = "sh -c 'exec 3<>/dev/tcp/1.1.1.1/443'"
    return _guest_probe(
        opts, "nat.guest_to_internet", "NAT reachability: guest -> internet",
        probe,
        skip_hint=("start the VM's guest agent and pass "
                   "--guest-exec 'vmforge guest exec <vm> --' to probe "
                   "outbound connectivity from the guest"),
        fail_hint=("guest has no outbound connectivity: check the host's own "
                   "internet access, that the NIC config does not set "
                   "restrict=on, and any host firewall egress rules"),
        pass_detail="guest reached 1.1.1.1:443 through the user-mode stack",
    )


def check_guest_dns(opts: DoctorOptions) -> CheckResult:
    probe = ("sh -c 'getent hosts example.com || nslookup example.com || "
             "host example.com'")
    return _guest_probe(
        opts, "dns.guest", "DNS resolution from guest",
        probe,
        skip_hint=("start the VM's guest agent and pass "
                   "--guest-exec 'vmforge guest exec <vm> --' to test DNS "
                   "resolution inside the guest"),
        fail_hint=(f"guest cannot resolve names: point the guest resolver at "
                   f"the SLIRP DNS forwarder {SLIRP_DNS} (usually set by "
                   "DHCP), or set an explicit dns= in the NIC config"),
        pass_detail="guest resolved example.com",
    )


# ---------------------------------------------------------------------------
# Rendering
# ---------------------------------------------------------------------------


def summarize(results: list[CheckResult]) -> dict:
    return {
        "pass": sum(1 for r in results if r.status == PASS),
        "fail": sum(1 for r in results if r.status == FAIL),
        "skip": sum(1 for r in results if r.status == SKIP),
    }


def to_json(results: list[CheckResult]) -> dict:
    from . import __version__

    return {
        "tool": "vmforge-net doctor",
        "version": __version__,
        "schema": SCHEMA_VERSION,
        "stability": STABILITY,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "checks": [r.to_dict() for r in results],
        "summary": summarize(results),
    }


def render_table(results: list[CheckResult]) -> str:
    rows = [("RESULT", "CHECK", "DETAIL")]
    for r in results:
        rows.append((r.status, r.id, r.detail))
    w0 = max(len(r[0]) for r in rows)
    w1 = max(len(r[1]) for r in rows)
    lines = [f"{r[0]:<{w0}}  {r[1]:<{w1}}  {r[2]}" for r in rows]
    for idx, r in enumerate(results):
        if r.status == FAIL and r.hint:
            lines[idx + 1] += f"\n{'':<{w0}}  {'':<{w1}}  hint: {r.hint}"
    counts = summarize(results)
    lines.append(
        f"\n{counts['pass']} passed, {counts['fail']} failed, "
        f"{counts['skip']} skipped"
    )
    return "\n".join(lines) + "\n"


def exit_code(results: list[CheckResult]) -> int:
    return 1 if any(r.status == FAIL for r in results) else 0
