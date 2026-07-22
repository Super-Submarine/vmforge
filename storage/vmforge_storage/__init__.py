"""VMForge storage v0: qcow2 disk & snapshot-tree management."""

from .qemu_img import CheckResult, ImageInfo, QemuImgError
from .store import DiskStore, Snapshot, StorageError

__version__ = "0.1.0"

__all__ = [
    "CheckResult",
    "DiskStore",
    "ImageInfo",
    "QemuImgError",
    "Snapshot",
    "StorageError",
    "__version__",
]
