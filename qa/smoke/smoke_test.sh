#!/usr/bin/env bash
# VMForge QA smoke suite: create -> boot -> snapshot -> restore -> shutdown,
# plus negative cases with --negative. See qa/README.md and qa/TEST_PLAN.md.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

GUEST_IMAGE_URL="${GUEST_IMAGE_URL:-https://dl-cdn.alpinelinux.org/alpine/v3.20/releases/cloud/nocloud_alpine-3.20.3-x86_64-bios-cloudinit-r0.qcow2}"
BOOT_TIMEOUT="${BOOT_TIMEOUT:-300}"
WORKDIR="${WORKDIR:-$SCRIPT_DIR/.work}"
VM_MEM="${VM_MEM:-512}"
DRIVER="${DRIVER:-qemu}"
SNAP_NAME="smoke1"

mkdir -p "$WORKDIR"
SERIAL_LOG="$WORKDIR/serial-$$.log"
export WORKDIR SERIAL_LOG SCRIPT_DIR VM_MEM

# shellcheck source=drivers/qemu.sh
source "$SCRIPT_DIR/drivers/$DRIVER.sh"

PASS=0; FAIL=0
step()   { echo; echo "==> $*"; }
ok()     { echo "    PASS: $*"; PASS=$((PASS + 1)); }
fail()   { echo "    FAIL: $*" >&2; FAIL=$((FAIL + 1)); }
die()    { echo "FATAL: $*" >&2; vm_kill 2>/dev/null || true; exit 1; }
cleanup() { vm_kill 2>/dev/null || true; }
trap cleanup EXIT

fetch_image() {
    local cache
    cache="$WORKDIR/$(basename "$GUEST_IMAGE_URL")"
    if [[ ! -s "$cache" ]]; then
        step "Downloading guest image: $GUEST_IMAGE_URL"
        curl -fsSL --retry 3 --retry-delay 5 -o "$cache.tmp" "$GUEST_IMAGE_URL"
        mv "$cache.tmp" "$cache"
    fi
    BASE_IMAGE="$cache"
}

make_seed() {
    SEED_ISO="$WORKDIR/seed.iso"
    local d="$WORKDIR/seed"
    mkdir -p "$d"
    cat > "$d/user-data" << 'EOF'
#cloud-config
password: vmforge
chpasswd: { expire: false }
ssh_pwauth: true
runcmd:
  - echo VMFORGE_CLOUD_INIT_DONE > /dev/console
EOF
    printf 'instance-id: vmforge-smoke\nlocal-hostname: vmforge-smoke\n' > "$d/meta-data"
    genisoimage -quiet -output "$SEED_ISO" -volid cidata -joliet -rock \
        "$d/user-data" "$d/meta-data"
}

happy_path() {
    step "Accelerator: $(accel_name)"

    step "Create VM disk (overlay on cached base image)"
    DISK="$WORKDIR/vm-$$.qcow2"
    qemu-img create -q -f qcow2 -b "$BASE_IMAGE" -F qcow2 "$DISK"
    vm_create "$DISK" "$SEED_ISO"
    ok "disk + seed created"

    step "Boot to ready (timeout ${BOOT_TIMEOUT}s)"
    local t0=$SECONDS
    vm_boot
    vm_wait_ready "$BOOT_TIMEOUT" || die "boot did not reach ready state"
    ok "booted to ready in $((SECONDS - t0))s"

    step "Snapshot running VM ($SNAP_NAME)"
    t0=$SECONDS
    vm_snapshot "$SNAP_NAME" || die "savevm failed"
    vm_list_snapshots | grep -q "$SNAP_NAME" || die "snapshot not listed by monitor"
    ok "snapshot taken and listed in $((SECONDS - t0))s"

    step "Restore running VM to $SNAP_NAME"
    t0=$SECONDS
    vm_restore "$SNAP_NAME" || die "loadvm failed"
    vm_query_status | grep -q '"running"' || die "VM not running after restore"
    ok "restored, VM running in $((SECONDS - t0))s"

    step "Graceful shutdown"
    if vm_stop; then ok "clean shutdown"; else fail "shutdown timed out (killed)"; fi

    step "Offline snapshot verification (qemu-img)"
    if qemu-img snapshot -l "$DISK" | grep -q "$SNAP_NAME"; then
        ok "snapshot persisted in qcow2"
    else
        fail "snapshot missing from qcow2"
    fi
    rm -f "$DISK"
}

negative_cases() {
    step "F2: boot with missing disk image must fail fast"
    if qemu-system-x86_64 -accel tcg -display none \
        -drive "file=$WORKDIR/does-not-exist.qcow2,if=virtio,format=qcow2" \
        > /dev/null 2>&1; then
        fail "QEMU started with a missing image"
    else
        ok "QEMU refused missing image"
    fi

    step "F1: corrupt qcow2 header must be rejected"
    local bad="$WORKDIR/corrupt.qcow2"
    qemu-img create -q -f qcow2 "$bad" 64M
    printf 'GARBAGE!' | dd of="$bad" bs=1 seek=0 conv=notrunc status=none
    if qemu-system-x86_64 -accel tcg -display none \
        -drive "file=$bad,if=virtio,format=qcow2" > /dev/null 2>&1; then
        fail "QEMU booted a corrupt image"
    else
        ok "QEMU refused corrupt image"
    fi
    rm -f "$bad"

    step "F3: kill -9 mid-boot, then relaunch on same disk"
    DISK="$WORKDIR/vm-neg-$$.qcow2"
    qemu-img create -q -f qcow2 -b "$BASE_IMAGE" -F qcow2 "$DISK"
    vm_create "$DISK" "$SEED_ISO"
    vm_boot
    sleep 5
    vm_kill
    if qemu-img check -q "$DISK" 2> /dev/null || [[ $? -le 2 ]]; then
        ok "qcow2 consistent (or only leaked clusters) after SIGKILL"
    else
        fail "qcow2 corrupted after SIGKILL"
    fi
    SERIAL_LOG="$WORKDIR/serial-neg-$$.log"
    vm_create "$DISK" "$SEED_ISO"
    vm_boot
    sleep 5
    if vm_is_running; then ok "relaunch after SIGKILL works"; else fail "relaunch failed"; fi
    vm_kill
    rm -f "$DISK"

    step "F6: loadvm of nonexistent snapshot must fail cleanly"
    SERIAL_LOG="$WORKDIR/serial-f6-$$.log"
    DISK="$WORKDIR/vm-f6-$$.qcow2"
    qemu-img create -q -f qcow2 -b "$BASE_IMAGE" -F qcow2 "$DISK"
    vm_create "$DISK" "$SEED_ISO"
    vm_boot
    sleep 5
    vm_is_running || die "VM failed to start for F6"
    if vm_restore "definitely-not-a-snapshot" 2> /dev/null; then
        fail "loadvm of bogus snapshot succeeded"
    else
        ok "loadvm of bogus snapshot rejected"
    fi
    if vm_query_status | grep -q '"running"\|"paused"'; then
        ok "VM state intact after failed loadvm"
    else
        fail "VM state broken after failed loadvm"
    fi
    vm_kill
    rm -f "$DISK"
}

main() {
    fetch_image
    make_seed
    if [[ "${1:-}" == "--negative" ]]; then
        negative_cases
    else
        happy_path
    fi
    echo
    echo "RESULT: $PASS passed, $FAIL failed (accel=$(accel_name), serial log: $SERIAL_LOG)"
    [[ $FAIL -eq 0 ]]
}

main "$@"
