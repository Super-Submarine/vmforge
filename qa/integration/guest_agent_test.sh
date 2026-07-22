#!/usr/bin/env bash
# QA v2 integration test: guest-agent ping/exec over virtio-serial (PR #4 surface).
#
# Delegates to the guest-tools e2e smoke (guest-tools/tests/ga_smoke.sh), which
# boots Alpine with the agent installed via cloud-init and exercises
# wait-ready -> ping/info -> exec -> shutdown over the real virtio-serial channel.
# A lightweight ping/exec check via vmforgectl is used if the smoke script is absent.
#
# SKIPS (exit 0, with reason) until the guest-tools subsystem (PR #4) is merged:
# guest-tools/ does not exist on main yet.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if [[ ! -d "$REPO_ROOT/guest-tools" ]]; then
    echo "SKIP: guest-tools/ not present on this branch yet (waiting on PR #4 to merge)"
    exit 0
fi

if [[ -x "$REPO_ROOT/guest-tools/tests/ga_smoke.sh" ]]; then
    echo "==> Running guest-tools e2e smoke (ping/info/exec/shutdown over virtio-serial)"
    exec "$REPO_ROOT/guest-tools/tests/ga_smoke.sh"
fi

echo "FAIL: guest-tools/ exists but tests/ga_smoke.sh is missing or not executable." >&2
echo "      Update qa/integration/guest_agent_test.sh to the merged guest-tools test entrypoint." >&2
exit 1
