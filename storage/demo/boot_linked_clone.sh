#!/usr/bin/env bash
# Demo: boot a VM from a linked clone created by vmforge-storage.
#
# Downloads a Cirros cloud image, imports it as a shared base image,
# creates a linked clone, snapshots it, and boots the clone with QEMU
# using TCG (no KVM required). Succeeds when the guest reaches its
# login prompt on the serial console, then proves the base image was
# never written to.
set -euo pipefail

CIRROS_URL="https://download.cirros-cloud.net/0.6.2/cirros-0.6.2-x86_64-disk.img"
WORKDIR="$(mktemp -d)"
export VMFORGE_HOME="$WORKDIR/vmforge-home"
trap 'rm -rf "$WORKDIR"' EXIT

echo "==> Downloading Cirros image"
curl -fsSL -o "$WORKDIR/cirros.img" "$CIRROS_URL"

echo "==> Importing as shared base image"
vmforge-storage import "$WORKDIR/cirros.img" --name cirros
BASE="$VMFORGE_HOME/images/cirros.qcow2"
BASE_SHA_BEFORE="$(sha256sum "$BASE" | cut -d' ' -f1)"

echo "==> Creating linked clone vm 'demo' disk 'root'"
vmforge-storage clone cirros demo root
vmforge-storage snapshot create demo root pristine
vmforge-storage info demo root
echo "==> Snapshot tree:"
vmforge-storage snapshot list demo root

DISK="$VMFORGE_HOME/vms/demo/disks/root.qcow2"
LOG="$WORKDIR/serial.log"

echo "==> Booting linked clone with QEMU (TCG, no KVM)"
timeout 600 qemu-system-x86_64 \
    -accel tcg -m 512 -nographic -serial "file:$LOG" -monitor none \
    -drive "file=$DISK,format=qcow2,if=virtio" &
QEMU_PID=$!

echo "==> Waiting for guest login prompt"
for _ in $(seq 1 120); do
    if grep -q "login:" "$LOG" 2>/dev/null; then
        echo "==> SUCCESS: guest reached login prompt"
        kill "$QEMU_PID" 2>/dev/null || true
        wait "$QEMU_PID" 2>/dev/null || true
        echo "==> Last serial console lines:"
        tail -n 5 "$LOG"
        BASE_SHA_AFTER="$(sha256sum "$BASE" | cut -d' ' -f1)"
        if [ "$BASE_SHA_BEFORE" = "$BASE_SHA_AFTER" ]; then
            echo "==> Base image untouched (sha256 unchanged) — true linked clone"
        else
            echo "==> ERROR: base image was modified!" >&2
            exit 1
        fi
        vmforge-storage check demo root
        exit 0
    fi
    sleep 5
done

echo "==> FAILED: guest did not reach login prompt in time" >&2
kill "$QEMU_PID" 2>/dev/null || true
exit 1
