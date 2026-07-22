"""VMForge networking v0: user-mode NAT backend for QEMU.

Generates -netdev user / -device virtio-net-pci arguments from a simple
config, and hot-adds/removes host->guest port forwards over QMP.
"""

from .config import NatConfig, PortForward
from .natgen import build_qemu_args
from .qmp import QMPClient, QMPError

__all__ = [
    "NatConfig",
    "PortForward",
    "build_qemu_args",
    "QMPClient",
    "QMPError",
]

__version__ = "0.1.0"
