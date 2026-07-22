import shutil

import pytest

from vmforge_storage import DiskStore

pytestmark = pytest.mark.skipif(
    shutil.which("qemu-img") is None, reason="qemu-img not installed"
)


def pytest_collection_modifyitems(config, items):
    if shutil.which("qemu-img") is None:
        skip = pytest.mark.skip(reason="qemu-img not installed")
        for item in items:
            item.add_marker(skip)


@pytest.fixture()
def store(tmp_path):
    return DiskStore(home=tmp_path / "vmforge-home")
