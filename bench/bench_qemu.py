#!/usr/bin/env python3
"""Benchmark QEMU/KVM + qcow2 in two modes:

  proxy — "vmforge proxy": QEMU driven over QMP exactly the way the VMForge
          Phase-1 architecture does (docs/architecture.md): snapshot-save /
          snapshot-load background jobs against a qcow2 disk. The real
          vmforge KVM driver is not merged yet (crates are stubs), so this
          stack is its stated proxy.
  raw   — raw QEMU as a user would drive it by hand: HMP savevm/loadvm via
          the monitor, qemu-img for offline snapshot bookkeeping.

Measured per iteration:
  boot_s              cold boot: exec() of qemu -> "login:" on serial
  snapshot_create_s   live snapshot of running VM incl. RAM state
  snapshot_restore_s  revert running VM to the snapshot (instant resume)
  resume_from_disk_s  fresh qemu process restoring the snapshot at launch
                      (-loadvm / snapshot-load) -> VM running
  storage_overhead_bytes  allocated bytes added to the qcow2 by one snapshot

Usage: bench_qemu.py --mode {proxy,raw} [--iterations 5]
"""

import argparse
import shutil
import subprocess
import time

from common import (
    BOOT_MARKER, BOOT_TIMEOUT_S, IMAGES_DIR, WORK_DIR, QMPClient,
    disk_usage, ensure_dirs, machine_specs, wait_for_marker, write_result,
)

BASE_IMAGE = IMAGES_DIR / "alpine-nocloud.qcow2"
MEM_MB = 512
SNAP_NAME = "bench-snap"


def qemu_cmd(disk, serial_log, qmp_sock, mode, extra=None):
    cmd = [
        "qemu-system-x86_64", "-accel", "kvm", "-cpu", "host",
        "-m", str(MEM_MB), "-smp", "1", "-display", "none",
        "-serial", f"file:{serial_log}",
        "-qmp", f"unix:{qmp_sock},server,nowait",
        "-net", "none",
    ]
    if mode == "proxy":
        cmd += [
            "-blockdev", f"driver=file,filename={disk},node-name=file0",
            "-blockdev", "driver=qcow2,file=file0,node-name=disk0",
            "-device", "virtio-blk-pci,drive=disk0",
        ]
    else:
        cmd += ["-drive", f"file={disk},if=virtio,format=qcow2"]
    return cmd + (extra or [])


def launch(disk, serial_log, qmp_sock, mode, extra=None):
    serial_log.unlink(missing_ok=True)
    start = time.monotonic()
    proc = subprocess.Popen(
        qemu_cmd(disk, serial_log, qmp_sock, mode, extra),
        stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
    )
    return proc, start


def vm_is_running(qmp):
    return qmp.command("query-status")["return"]["status"] == "running"


def wait_running(qmp, timeout_s=120.0):
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        if vm_is_running(qmp):
            return
        time.sleep(0.005)
    raise TimeoutError("VM did not reach 'running'")


def snapshot_create(qmp, mode):
    if mode == "proxy":
        qmp.command("snapshot-save", **{
            "job-id": "save0", "tag": SNAP_NAME,
            "vmstate": "disk0", "devices": ["disk0"]})
        qmp.wait_job_done("save0")
    else:
        qmp.command("human-monitor-command",
                    **{"command-line": f"savevm {SNAP_NAME}"})


def snapshot_restore(qmp, mode):
    if mode == "proxy":
        qmp.command("stop")
        qmp.command("snapshot-load", **{
            "job-id": "load0", "tag": SNAP_NAME,
            "vmstate": "disk0", "devices": ["disk0"]})
        qmp.wait_job_done("load0")
        qmp.command("cont")
    else:
        qmp.command("human-monitor-command",
                    **{"command-line": f"loadvm {SNAP_NAME}"})
        qmp.command("cont")


def run_iteration(mode, i):
    disk = WORK_DIR / f"qemu-{mode}-{i}.qcow2"
    serial_log = WORK_DIR / f"qemu-{mode}-{i}.serial"
    qmp_sock = WORK_DIR / f"qemu-{mode}-{i}.qmp"
    shutil.copyfile(BASE_IMAGE, disk)
    r = {}

    proc, start = launch(disk, serial_log, qmp_sock, mode)
    try:
        r["boot_s"] = wait_for_marker(serial_log, BOOT_MARKER, BOOT_TIMEOUT_S, start)
        qmp = QMPClient(qmp_sock)

        size_before = disk_usage(disk)
        t0 = time.monotonic()
        snapshot_create(qmp, mode)
        r["snapshot_create_s"] = time.monotonic() - t0
        r["storage_overhead_bytes"] = disk_usage(disk) - size_before

        t0 = time.monotonic()
        snapshot_restore(qmp, mode)
        wait_running(qmp)
        r["snapshot_restore_s"] = time.monotonic() - t0

        qmp.command("quit")
        qmp.close()
    finally:
        proc.wait(timeout=30)

    # Fresh process restoring the snapshot at launch (instant resume from disk).
    proc, start = launch(disk, serial_log, qmp_sock, mode,
                         extra=["-loadvm", SNAP_NAME])
    try:
        qmp = QMPClient(qmp_sock)
        wait_running(qmp, timeout_s=BOOT_TIMEOUT_S)
        r["resume_from_disk_s"] = time.monotonic() - start
        qmp.command("quit")
        qmp.close()
    finally:
        proc.wait(timeout=30)

    disk.unlink(missing_ok=True)
    serial_log.unlink(missing_ok=True)
    return r


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mode", choices=["proxy", "raw"], required=True)
    ap.add_argument("--iterations", type=int, default=5)
    args = ap.parse_args()

    ensure_dirs()
    if not BASE_IMAGE.exists():
        raise SystemExit(f"missing {BASE_IMAGE}; run fetch_assets.sh first")

    iters = []
    for i in range(args.iterations):
        print(f"[qemu-{args.mode}] iteration {i + 1}/{args.iterations}")
        iters.append(run_iteration(args.mode, i))
        print(f"  {iters[-1]}")

    label = ("vmforge-proxy (QEMU/KVM via QMP snapshot jobs)"
             if args.mode == "proxy"
             else "raw QEMU/KVM (HMP savevm/loadvm + qemu-img)")
    write_result(f"qemu-{args.mode}", {
        "stack": label,
        "guest": BASE_IMAGE.name,
        "mem_mb": MEM_MB,
        "iterations": iters,
        "machine": machine_specs(),
    })


if __name__ == "__main__":
    main()
