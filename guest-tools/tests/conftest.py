import importlib.util
import sys
from pathlib import Path

GUEST_TOOLS = Path(__file__).resolve().parent.parent


def _load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    spec.loader.exec_module(mod)
    return mod


def load_agent():
    return _load("vmforge_ga", GUEST_TOOLS / "agent" / "vmforge-ga.py")


def load_client():
    return _load("vmforgectl", GUEST_TOOLS / "host" / "vmforgectl.py")
