"""Thin, typed wrapper around the qemu-img binary.

All VMForge storage operations ultimately shell out to qemu-img. This module
centralizes invocation, JSON parsing, and error handling so higher layers
never build command lines themselves.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional


class QemuImgError(RuntimeError):
    def __init__(self, cmd: list[str], returncode: int, stderr: str) -> None:
        self.cmd = cmd
        self.returncode = returncode
        self.stderr = stderr
        super().__init__(f"qemu-img failed ({returncode}): {' '.join(cmd)}\n{stderr}")


def _qemu_img_bin() -> str:
    binary = os.environ.get("VMFORGE_QEMU_IMG", "qemu-img")
    resolved = shutil.which(binary)
    if resolved is None:
        raise QemuImgError([binary], 127, f"{binary} not found on PATH")
    return resolved


def run_qemu_img(args: list[str]) -> str:
    cmd = [_qemu_img_bin()] + args
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        raise QemuImgError(cmd, proc.returncode, proc.stderr.strip())
    return proc.stdout


@dataclass
class ImageInfo:
    path: Path
    format: str
    virtual_size: int
    actual_size: int
    backing_file: Optional[Path]
    backing_format: Optional[str]
    cluster_size: Optional[int]
    dirty: bool
    raw: dict = field(repr=False, default_factory=dict)


def info(path: Path | str, backing_chain: bool = False) -> list[ImageInfo]:
    """Return image info; first element is the image itself, followed by its
    backing chain when backing_chain=True."""
    args = ["info", "--output=json", "--force-share"]
    if backing_chain:
        args.append("--backing-chain")
    args.append(str(path))
    data = json.loads(run_qemu_img(args))
    entries = data if isinstance(data, list) else [data]
    result = []
    for entry in entries:
        backing = entry.get("full-backing-filename") or entry.get("backing-filename")
        result.append(
            ImageInfo(
                path=Path(entry["filename"]).resolve(),
                format=entry["format"],
                virtual_size=entry["virtual-size"],
                actual_size=entry.get("actual-size", 0),
                backing_file=Path(backing).resolve() if backing else None,
                backing_format=entry.get("backing-filename-format"),
                cluster_size=entry.get("cluster-size"),
                dirty=bool(entry.get("dirty-flag", False)),
                raw=entry,
            )
        )
    return result


PREALLOCATION_MODES = ("off", "metadata", "falloc", "full")


def create_qcow2(
    path: Path | str,
    size: Optional[int | str] = None,
    *,
    backing_file: Optional[Path | str] = None,
    backing_format: str = "qcow2",
    preallocation: str = "off",
    cluster_size: Optional[str] = None,
) -> None:
    if preallocation not in PREALLOCATION_MODES:
        raise ValueError(
            f"preallocation must be one of {PREALLOCATION_MODES}, got {preallocation!r}"
        )
    opts = [f"preallocation={preallocation}"]
    if cluster_size:
        opts.append(f"cluster_size={cluster_size}")
    args = ["create", "-f", "qcow2", "-o", ",".join(opts)]
    if backing_file is not None:
        args += ["-b", str(backing_file), "-F", backing_format]
    args.append(str(path))
    if size is not None:
        args.append(str(size))
    elif backing_file is None:
        raise ValueError("size is required when no backing file is given")
    run_qemu_img(args)


def resize(path: Path | str, new_size: int | str, *, shrink: bool = False) -> None:
    args = ["resize"]
    if shrink:
        args.append("--shrink")
    args += [str(path), str(new_size)]
    run_qemu_img(args)


def convert(
    src: Path | str,
    dst: Path | str,
    *,
    src_format: Optional[str] = None,
    dst_format: str = "qcow2",
    compress: bool = False,
) -> None:
    args = ["convert", "-O", dst_format]
    if compress:
        args.append("-c")
    if src_format:
        args += ["-f", src_format]
    args += [str(src), str(dst)]
    run_qemu_img(args)


def rebase(
    path: Path | str,
    new_backing: Optional[Path | str],
    *,
    backing_format: str = "qcow2",
    unsafe: bool = False,
) -> None:
    args = ["rebase"]
    if unsafe:
        args.append("-u")
    if new_backing is None:
        args += ["-b", ""]
    else:
        args += ["-b", str(new_backing), "-F", backing_format]
    args.append(str(path))
    run_qemu_img(args)


def commit(path: Path | str) -> None:
    run_qemu_img(["commit", str(path)])


@dataclass
class CheckResult:
    ok: bool
    corruptions: int
    leaks: int
    errors_fixed: int
    leaks_fixed: int
    raw: dict = field(repr=False, default_factory=dict)


def check(path: Path | str, *, repair: bool = False) -> CheckResult:
    args = ["check", "--output=json", "--force-share"]
    if repair:
        args.remove("--force-share")
        args += ["-r", "all"]
    args.append(str(path))
    cmd = [_qemu_img_bin()] + args
    proc = subprocess.run(cmd, capture_output=True, text=True)
    # qemu-img check exit codes: 0 clean, 1 error, 2 corruption, 3 leaks
    if proc.returncode in (1,) or not proc.stdout.strip():
        raise QemuImgError(cmd, proc.returncode, proc.stderr.strip())
    data = json.loads(proc.stdout)
    return CheckResult(
        ok=proc.returncode == 0,
        corruptions=data.get("corruptions", 0),
        leaks=data.get("leaks", 0),
        errors_fixed=data.get("corruptions-fixed", 0),
        leaks_fixed=data.get("leaks-fixed", 0),
        raw=data,
    )
