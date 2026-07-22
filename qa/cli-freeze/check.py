#!/usr/bin/env python3
"""CLI-freeze hygiene check (v1.0-beta).

Verifies the live command surface against qa/cli-freeze/frozen-surface.json:

* ``vmforge`` (path via $VMFORGE_BIN, default target/debug/vmforge): probes
  the built binary — ``info`` exits 0 or 1 (host-dependent backend), unknown
  verbs exit 2.
* ``vmforge-storage``: introspects ``vmforge_storage.cli.build_parser()`` and
  diffs verbs, positional order, flags, global flags and the contract version
  against the manifest.

Exit 0 = surface matches the freeze; exit 1 = frozen surface changed (the PR
must update the manifest and docs/cli-freeze-v1.0-beta.md together).
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
MANIFEST = Path(__file__).with_name("frozen-surface.json")

errors: list[str] = []


def err(msg: str) -> None:
    errors.append(msg)
    print(f"FREEZE VIOLATION: {msg}", file=sys.stderr)


def check_vmforge(manifest: dict) -> None:
    bin_path = os.environ.get("VMFORGE_BIN", str(REPO / "target/debug/vmforge"))
    if not Path(bin_path).exists():
        err(f"vmforge binary not found at {bin_path} (build it or set VMFORGE_BIN)")
        return
    codes = manifest["exit_codes"]

    r = subprocess.run([bin_path, "info"], capture_output=True, text=True)
    if r.returncode not in (codes["success"], codes["no_backend"]):
        err(f"`vmforge info` exited {r.returncode}; frozen codes are "
            f"{codes['success']} (success) / {codes['no_backend']} (no backend)")

    r = subprocess.run([bin_path, "definitely-not-a-frozen-verb"],
                       capture_output=True, text=True)
    if r.returncode != codes["unknown_command"]:
        err(f"unknown verb exited {r.returncode}; frozen unknown-command code is "
            f"{codes['unknown_command']}")

    for verb in manifest["verbs"]:
        r = subprocess.run([bin_path, verb], capture_output=True, text=True)
        if r.returncode == codes["unknown_command"]:
            err(f"frozen verb `vmforge {verb}` is no longer recognized")


def _surface(parser: argparse.ArgumentParser, prefix: str = "") -> dict[str, dict]:
    out: dict[str, dict] = {}
    subactions = [a for a in parser._actions
                  if isinstance(a, argparse._SubParsersAction)]
    positionals = [a.dest for a in parser._actions
                   if not a.option_strings
                   and not isinstance(a, argparse._SubParsersAction)]
    flags = sorted(s for a in parser._actions for s in a.option_strings
                   if s.startswith("--") and s != "--help")
    if prefix:
        out[prefix] = {"positionals": positionals, "flags": flags}
    for sub in subactions:
        for name, sp in sub.choices.items():
            full = f"{prefix} {name}".strip()
            nested = _surface(sp, full)
            if any(isinstance(a, argparse._SubParsersAction) for a in sp._actions):
                nested.pop(full, None)  # group verb, not directly invocable
            out.update(nested)
    return out


def check_storage(manifest: dict) -> None:
    sys.path.insert(0, str(REPO / "storage"))
    from vmforge_storage import cli as storage_cli  # noqa: PLC0415

    parser = storage_cli.build_parser()

    global_flags = sorted(s for a in parser._actions for s in a.option_strings
                          if s.startswith("--") and s != "--help")
    frozen_globals = sorted(manifest["global_flags"])
    if global_flags != frozen_globals:
        err(f"vmforge-storage global flags changed: {global_flags} != {frozen_globals}")

    if str(storage_cli.CONTRACT_VERSION) != manifest["contract_version"]:
        err(f"vmforge-storage contract version changed: "
            f"{storage_cli.CONTRACT_VERSION} != {manifest['contract_version']}")

    live = _surface(parser)
    frozen = manifest["verbs"]

    for verb, spec in frozen.items():
        if verb not in live:
            err(f"frozen verb `vmforge-storage {verb}` was removed")
            continue
        if live[verb]["positionals"] != spec["positionals"]:
            err(f"`vmforge-storage {verb}` positionals changed: "
                f"{live[verb]['positionals']} != {spec['positionals']}")
        missing = sorted(set(spec["flags"]) - set(live[verb]["flags"]))
        if missing:
            err(f"`vmforge-storage {verb}` lost frozen flags: {missing}")


def main() -> int:
    manifest = json.loads(MANIFEST.read_text())
    check_vmforge(manifest["vmforge"])
    check_storage(manifest["vmforge_storage"])
    if errors:
        print(f"\n{len(errors)} freeze violation(s). If this change is intentional, "
              f"update {MANIFEST.relative_to(REPO)} and docs/cli-freeze-v1.0-beta.md "
              f"in the same PR.", file=sys.stderr)
        return 1
    print("CLI freeze check passed: surface matches v1.0-beta manifest.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
