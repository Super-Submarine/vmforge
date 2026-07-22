#!/usr/bin/env python3
"""Benchmark Firecracker (KVM microVM) boot / snapshot / restore.

Included as the third measured stack on this host because VirtualBox cannot
load its vboxdrv kernel module here (see bench_vbox.sh and README). It also
serves as the state-of-the-art reference for snapshot/instant-resume
latency (https://firecracker-microvm.github.io/).

Measured per iteration:
  boot_s              exec() of firecracker + API config -> "login:" on serial
  snapshot_create_s   PATCH Pause + PUT /snapshot/create (Full)
  snapshot_restore_s  fresh firecracker process + PUT /snapshot/load
                      (+ Resume) -> VM running   [this is both "restore" and
                      "resume-from-disk" for Firecracker: restore always
                      starts a new process]
  storage_overhead_bytes  size of snapshot memory + vmstate files
"""

import argparse
import http.client
import json
import shutil
import socket
import subprocess
import time

from common import (
    BOOT_MARKER, BOOT_TIMEOUT_S, IMAGES_DIR, WORK_DIR,
    ensure_dirs, file_size, machine_specs, wait_for_marker, write_result,
)

FC_BIN = WORK_DIR / "firecracker"
KERNEL = IMAGES_DIR / "fc-vmlinux"
ROOTFS = IMAGES_DIR / "fc-rootfs.ext4"
MEM_MB = 512


class UnixHTTP(http.client.HTTPConnection):
    def __init__(self, sock_path):
        super().__init__("localhost")
        self.sock_path = str(sock_path)

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(self.sock_path)


def api(sock_path, method, path, body=None):
    deadline = time.monotonic() + 10
    while True:
        try:
            conn = UnixHTTP(sock_path)
            conn.request(method, path, json.dumps(body) if body else None,
                         {"Content-Type": "application/json"})
            resp = conn.getresponse()
            data = resp.read()
            conn.close()
            if resp.status >= 300:
                raise RuntimeError(f"{method} {path}: {resp.status} {data}")
            return data
        except (FileNotFoundError, ConnectionRefusedError):
            if time.monotonic() > deadline:
                raise
            time.sleep(0.02)


def spawn_fc(sock_path):
    sock_path.unlink(missing_ok=True)
    return subprocess.Popen(
        [str(FC_BIN), "--api-sock", str(sock_path)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def run_iteration(i):
    rootfs = WORK_DIR / f"fc-{i}.ext4"
    serial_log = WORK_DIR / f"fc-{i}.serial"
    sock = WORK_DIR / f"fc-{i}.sock"
    mem_file = WORK_DIR / f"fc-{i}.mem"
    state_file = WORK_DIR / f"fc-{i}.vmstate"
    for p in (mem_file, state_file):
        p.unlink(missing_ok=True)
    shutil.copyfile(ROOTFS, rootfs)
    serial_log.write_bytes(b"")
    r = {}

    # Firecracker writes guest serial to its stdout; redirect it to a file
    # so boot completion can be detected by watching for the login prompt.
    with open(serial_log, "wb") as slog:
        proc = subprocess.Popen([str(FC_BIN), "--api-sock", str(sock)],
                                stdout=slog, stderr=subprocess.STDOUT)
    start = time.monotonic()
    api(sock, "PUT", "/boot-source", {
        "kernel_image_path": str(KERNEL),
        "boot_args": "console=ttyS0 reboot=k panic=1 pci=off"})
    api(sock, "PUT", "/drives/rootfs", {
        "drive_id": "rootfs", "path_on_host": str(rootfs),
        "is_root_device": True, "is_read_only": False})
    api(sock, "PUT", "/machine-config", {"vcpu_count": 1, "mem_size_mib": MEM_MB})
    api(sock, "PUT", "/actions", {"action_type": "InstanceStart"})
    r["boot_s"] = wait_for_marker(serial_log, BOOT_MARKER, BOOT_TIMEOUT_S, start)

    t0 = time.monotonic()
    api(sock, "PATCH", "/vm", {"state": "Paused"})
    api(sock, "PUT", "/snapshot/create", {
        "snapshot_type": "Full",
        "snapshot_path": str(state_file), "mem_file_path": str(mem_file)})
    r["snapshot_create_s"] = time.monotonic() - t0
    r["storage_overhead_bytes"] = file_size(mem_file) + file_size(state_file)
    proc.kill()
    proc.wait()

    # Restore into a fresh process (Firecracker's only restore path).
    sock.unlink(missing_ok=True)
    proc = spawn_fc(sock)
    t0 = time.monotonic()
    api(sock, "PUT", "/snapshot/load", {
        "snapshot_path": str(state_file), "mem_backend": {
            "backend_type": "File", "backend_path": str(mem_file)},
        "resume_vm": True})
    r["snapshot_restore_s"] = time.monotonic() - t0
    r["resume_from_disk_s"] = r["snapshot_restore_s"]
    proc.kill()
    proc.wait()

    for p in (rootfs, serial_log, sock, mem_file, state_file):
        p.unlink(missing_ok=True)
    return r


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--iterations", type=int, default=5)
    args = ap.parse_args()

    ensure_dirs()
    for p in (FC_BIN, KERNEL, ROOTFS):
        if not p.exists():
            raise SystemExit(f"missing {p}; run fetch_assets.sh first")

    iters = []
    for i in range(args.iterations):
        print(f"[firecracker] iteration {i + 1}/{args.iterations}")
        iters.append(run_iteration(i))
        print(f"  {iters[-1]}")

    write_result("firecracker", {
        "stack": "Firecracker (KVM microVM, snapshot/restore API)",
        "guest": f"{KERNEL.name} + {ROOTFS.name} (Ubuntu 22.04 CI rootfs)",
        "mem_mb": MEM_MB,
        "iterations": iters,
        "machine": machine_specs(),
    })


if __name__ == "__main__":
    main()
