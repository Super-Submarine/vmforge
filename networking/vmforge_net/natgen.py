"""Generate QEMU command-line arguments from a NatConfig."""

from __future__ import annotations

from .config import NatConfig


def build_netdev_arg(cfg: NatConfig) -> str:
    """Build the -netdev user,... option string."""
    parts = [f"user,id={cfg.netdev_id}"]
    if cfg.net:
        parts.append(f"net={cfg.net}")
    if cfg.host:
        parts.append(f"host={cfg.host}")
    if cfg.dns:
        parts.append(f"dns={cfg.dns}")
    if cfg.dhcp_start:
        parts.append(f"dhcpstart={cfg.dhcp_start}")
    if cfg.hostname:
        parts.append(f"hostname={cfg.hostname}")
    if cfg.restrict:
        parts.append("restrict=on")
    for fwd in cfg.forwards:
        parts.append(f"hostfwd={fwd.to_hostfwd()}")
    return ",".join(parts)


def build_device_arg(cfg: NatConfig) -> str:
    """Build the -device virtio-net-pci,... option string."""
    parts = [f"{cfg.model},netdev={cfg.netdev_id}"]
    if cfg.mac:
        parts.append(f"mac={cfg.mac}")
    return ",".join(parts)


def build_qemu_args(cfg: NatConfig) -> list[str]:
    """Return the full QEMU argument list for this NIC.

    Example output:
        ["-netdev", "user,id=net0,hostfwd=tcp:127.0.0.1:8080-:80",
         "-device", "virtio-net-pci,netdev=net0"]
    """
    return [
        "-netdev",
        build_netdev_arg(cfg),
        "-device",
        build_device_arg(cfg),
    ]
