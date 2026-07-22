"""Config model for the user-mode NAT backend."""

from __future__ import annotations

import ipaddress
import re
from dataclasses import dataclass, field

_MAC_RE = re.compile(r"^[0-9a-fA-F]{2}(:[0-9a-fA-F]{2}){5}$")
_ID_RE = re.compile(r"^[A-Za-z][A-Za-z0-9_.-]*$")

VALID_PROTOS = ("tcp", "udp")


@dataclass(frozen=True)
class PortForward:
    """A host->guest port forwarding rule (QEMU hostfwd)."""

    host_port: int
    guest_port: int
    proto: str = "tcp"
    host_ip: str = "127.0.0.1"
    guest_ip: str = ""  # empty = guest's DHCP address

    def __post_init__(self) -> None:
        if self.proto not in VALID_PROTOS:
            raise ValueError(f"proto must be one of {VALID_PROTOS}, got {self.proto!r}")
        for name, port in (("host_port", self.host_port), ("guest_port", self.guest_port)):
            if not isinstance(port, int) or not (1 <= port <= 65535):
                raise ValueError(f"{name} must be an integer in 1..65535, got {port!r}")
        if self.host_ip:
            ipaddress.ip_address(self.host_ip)
        if self.guest_ip:
            ipaddress.ip_address(self.guest_ip)

    def to_hostfwd(self) -> str:
        """Render as a QEMU hostfwd rule: proto:hostip:hostport-guestip:guestport."""
        return (
            f"{self.proto}:{self.host_ip}:{self.host_port}"
            f"-{self.guest_ip}:{self.guest_port}"
        )

    @classmethod
    def from_dict(cls, d: dict) -> "PortForward":
        return cls(
            host_port=d["host_port"],
            guest_port=d["guest_port"],
            proto=d.get("proto", "tcp"),
            host_ip=d.get("host_ip", "127.0.0.1"),
            guest_ip=d.get("guest_ip", ""),
        )

    @classmethod
    def parse(cls, spec: str) -> "PortForward":
        """Parse 'proto:hostip:hostport-guestip:guestport' or 'hostport:guestport'."""
        if "-" in spec:
            left, right = spec.split("-", 1)
            lparts = left.split(":")
            if len(lparts) != 3:
                raise ValueError(f"bad forward spec {spec!r}")
            proto, host_ip, host_port = lparts
            rparts = right.rsplit(":", 1)
            if len(rparts) != 2:
                raise ValueError(f"bad forward spec {spec!r}")
            guest_ip, guest_port = rparts
            return cls(
                host_port=int(host_port),
                guest_port=int(guest_port),
                proto=proto,
                host_ip=host_ip or "127.0.0.1",
                guest_ip=guest_ip,
            )
        parts = spec.split(":")
        if len(parts) == 2:
            return cls(host_port=int(parts[0]), guest_port=int(parts[1]))
        raise ValueError(f"bad forward spec {spec!r}")


@dataclass
class NatConfig:
    """Configuration for one user-mode NAT NIC."""

    netdev_id: str = "vmforge-nat0"
    model: str = "virtio-net-pci"
    mac: str | None = None
    net: str | None = None  # e.g. "10.0.2.0/24"
    host: str | None = None  # gateway IP inside guest network
    dns: str | None = None
    dhcp_start: str | None = None
    hostname: str | None = None
    restrict: bool = False
    forwards: list[PortForward] = field(default_factory=list)

    def __post_init__(self) -> None:
        if not _ID_RE.match(self.netdev_id):
            raise ValueError(f"invalid netdev id {self.netdev_id!r}")
        if self.mac is not None and not _MAC_RE.match(self.mac):
            raise ValueError(f"invalid MAC address {self.mac!r}")
        if self.net is not None:
            ipaddress.ip_network(self.net, strict=False)
        for name in ("host", "dns", "dhcp_start"):
            value = getattr(self, name)
            if value is not None:
                ipaddress.ip_address(value)

    @classmethod
    def from_dict(cls, d: dict) -> "NatConfig":
        return cls(
            netdev_id=d.get("netdev_id", "vmforge-nat0"),
            model=d.get("model", "virtio-net-pci"),
            mac=d.get("mac"),
            net=d.get("net"),
            host=d.get("host"),
            dns=d.get("dns"),
            dhcp_start=d.get("dhcp_start"),
            hostname=d.get("hostname"),
            restrict=d.get("restrict", False),
            forwards=[PortForward.from_dict(f) for f in d.get("forwards", [])],
        )
