"""Shared helpers for the VMForge wedge-claim benchmark harness."""

import json
import os
import platform
import socket
import subprocess
import time
from pathlib import Path

BENCH_DIR = Path(__file__).resolve().parent
IMAGES_DIR = BENCH_DIR / "images"
RESULTS_DIR = BENCH_DIR / "results"
WORK_DIR = BENCH_DIR / "work"

BOOT_MARKER = b"login:"
BOOT_TIMEOUT_S = 180


def ensure_dirs() -> None:
    for d in (IMAGES_DIR, RESULTS_DIR, WORK_DIR):
        d.mkdir(parents=True, exist_ok=True)


def now() -> float:
    return time.monotonic()


def wait_for_marker(log_path: Path, marker: bytes, timeout_s: float, start: float) -> float:
    """Poll a serial log file until `marker` appears; return elapsed seconds."""
    deadline = start + timeout_s
    while time.monotonic() < deadline:
        if log_path.exists() and marker in log_path.read_bytes():
            return time.monotonic() - start
        time.sleep(0.01)
    raise TimeoutError(f"marker {marker!r} not found in {log_path} within {timeout_s}s")


def file_size(path: Path) -> int:
    return path.stat().st_size if path.exists() else 0


def disk_usage(path: Path) -> int:
    """Actual allocated bytes (matters for sparse files)."""
    return path.stat().st_blocks * 512 if path.exists() else 0


class QMPClient:
    """Minimal QMP client over a unix socket."""

    def __init__(self, sock_path: Path, timeout: float = 120.0):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(timeout)
        deadline = time.monotonic() + 30
        while True:
            try:
                self.sock.connect(str(sock_path))
                break
            except (FileNotFoundError, ConnectionRefusedError):
                if time.monotonic() > deadline:
                    raise
                time.sleep(0.05)
        self.buf = b""
        self._recv_msg()  # greeting
        self.command("qmp_capabilities")

    def _recv_msg(self) -> dict:
        while b"\n" not in self.buf:
            self.buf += self.sock.recv(65536)
        line, self.buf = self.buf.split(b"\n", 1)
        return json.loads(line)

    def command(self, name: str, **args) -> dict:
        msg = {"execute": name}
        if args:
            msg["arguments"] = args
        self.sock.sendall(json.dumps(msg).encode() + b"\n")
        while True:
            resp = self._recv_msg()
            if "return" in resp or "error" in resp:
                if "error" in resp:
                    raise RuntimeError(f"QMP {name}: {resp['error']}")
                return resp

    def wait_event(self, event: str, timeout_s: float = 120.0) -> dict:
        deadline = time.monotonic() + timeout_s
        while time.monotonic() < deadline:
            resp = self._recv_msg()
            if resp.get("event") == event:
                return resp
        raise TimeoutError(f"QMP event {event} not seen within {timeout_s}s")

    def wait_job_done(self, job_id: str, timeout_s: float = 300.0) -> None:
        """Poll query-jobs until the given job reaches 'concluded'."""
        deadline = time.monotonic() + timeout_s
        while time.monotonic() < deadline:
            jobs = self.command("query-jobs")["return"]
            for j in jobs:
                if j["id"] == job_id:
                    if j["status"] == "concluded":
                        if j.get("error"):
                            raise RuntimeError(f"job {job_id} failed: {j['error']}")
                        self.command("job-dismiss", id=job_id)
                        return
            time.sleep(0.01)
        raise TimeoutError(f"job {job_id} not concluded within {timeout_s}s")

    def close(self) -> None:
        self.sock.close()


def machine_specs() -> dict:
    def sh(cmd: str) -> str:
        return subprocess.run(cmd, shell=True, capture_output=True, text=True).stdout.strip()

    return {
        "hostname": platform.node(),
        "kernel": platform.release(),
        "os": sh(". /etc/os-release && echo \"$PRETTY_NAME\""),
        "cpu_model": sh("lscpu | awk -F': +' '/Model name/{print $2; exit}'"),
        "cpus": os.cpu_count(),
        "mem_total_kb": int(sh("awk '/MemTotal/{print $2}' /proc/meminfo") or 0),
        "kvm_available": os.path.exists("/dev/kvm"),
        "qemu_version": sh("qemu-system-x86_64 --version | head -1"),
        "qemu_img_version": sh("qemu-img --version | head -1"),
        "virtualbox_version": sh("VBoxManage --version 2>/dev/null | tail -1 || true") or "not available",
        "firecracker_version": sh(f"{BENCH_DIR}/work/firecracker --version 2>/dev/null | head -1 || echo 'not available'"),
        "disk": sh("df -h --output=source,fstype,size / | tail -1"),
        "timestamp_utc": sh("date -u +%Y-%m-%dT%H:%M:%SZ"),
    }


def write_result(name: str, data: dict) -> Path:
    ensure_dirs()
    out = RESULTS_DIR / f"{name}.json"
    out.write_text(json.dumps(data, indent=2))
    print(f"wrote {out}")
    return out
