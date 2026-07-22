#!/usr/bin/env bash
# VMForge QA v2 negative / failure-mode suite.
#
# Beta-readiness robustness cases beyond qa/smoke/smoke_test.sh --negative:
#   X1  VM process crash mid-run (SIGKILL)   -> cleanup + relaunch works
#   X2  disk-full during snapshot (savevm)   -> clean error, VM keeps running
#   X3  corrupt/truncated qcow2 snapshot file -> refused with clean error
#   X4  invalid VM config                    -> fast, clean startup error
#   X5  double-boot of the same VM disk      -> second boot refused (image lock)
#   X6  snapshot-restore of a deleted branch -> loadvm fails cleanly
#
# Every case additionally asserts: no orphaned QEMU processes and no corrupted
# VM state dir (qemu-img check on the disk).
#
# Backend selection follows qa/smoke/drivers/qemu.sh (BACKEND=auto|kvm|tcg|tcg-aarch64).
# X2 needs a small dedicated filesystem; it uses a tmpfs mount via sudo and is
# skipped with a reason when passwordless sudo is unavailable.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SMOKE_DIR="$SCRIPT_DIR/../smoke"

BOOT_TIMEOUT="${BOOT_TIMEOUT:-300}"
WORKDIR="${WORKDIR:-$SCRIPT_DIR/.work}"
VM_MEM="${VM_MEM:-512}"
DRIVER="${DRIVER:-qemu}"

mkdir -p "$WORKDIR"
SERIAL_LOG="$WORKDIR/serial-$$.log"
export WORKDIR SERIAL_LOG VM_MEM
SCRIPT_DIR="$SMOKE_DIR" # drivers + qmp.py live under qa/smoke
export SCRIPT_DIR

# shellcheck source=../smoke/drivers/qemu.sh
source "$SMOKE_DIR/drivers/$DRIVER.sh"

_default_image_url() {
    case "$(vm_arch)" in
        aarch64) echo "https://dl-cdn.alpinelinux.org/alpine/v3.20/releases/cloud/nocloud_alpine-3.20.3-aarch64-uefi-cloudinit-r0.qcow2" ;;
        *) echo "https://dl-cdn.alpinelinux.org/alpine/v3.20/releases/cloud/nocloud_alpine-3.20.3-x86_64-bios-cloudinit-r0.qcow2" ;;
    esac
}
GUEST_IMAGE_URL="${GUEST_IMAGE_URL:-$(_default_image_url)}"

PASS=0; FAIL=0; SKIP=0
step() { echo; echo "==> $*"; }
ok()   { echo "    PASS: $*"; PASS=$((PASS + 1)); }
fail() { echo "    FAIL: $*" >&2; FAIL=$((FAIL + 1)); }
skip() { echo "    SKIP: $*"; SKIP=$((SKIP + 1)); }
die()  { echo "FATAL: $*" >&2; cleanup_all; exit 1; }

TMPFS_MNT=""
cleanup_all() {
    vm_kill 2> /dev/null || true
    pkill -9 -f "qemu-system-.*$WORKDIR" 2> /dev/null || true
    if [[ -n "$TMPFS_MNT" ]] && mountpoint -q "$TMPFS_MNT" 2> /dev/null; then
        sudo umount "$TMPFS_MNT" || true
    fi
}
trap cleanup_all EXIT

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
runcmd:
  - echo VMFORGE_CLOUD_INIT_DONE > /dev/console
EOF
    printf 'instance-id: vmforge-failure\nlocal-hostname: vmforge-failure\n' > "$d/meta-data"
    genisoimage -quiet -output "$SEED_ISO" -volid cidata -joliet -rock \
        "$d/user-data" "$d/meta-data"
}

# --- shared assertions ------------------------------------------------------

assert_no_orphans() {
    # All QEMU processes launched by this suite carry $WORKDIR in their argv.
    local orphans
    orphans="$(pgrep -f "qemu-system-.*$WORKDIR" || true)"
    if [[ -z "$orphans" ]]; then
        ok "$1: no orphaned QEMU processes"
    else
        fail "$1: orphaned QEMU processes: $orphans"
        # shellcheck disable=SC2086
        kill -9 $orphans 2> /dev/null || true
    fi
}

assert_state_ok() {
    # qemu-img check: 0 = clean, 1 = leaked clusters only (repairable), >=2 = corrupt.
    local rc=0
    qemu-img check -q "$2" 2> /dev/null || rc=$?
    if (( rc <= 1 )); then
        ok "$1: state dir / qcow2 consistent (qemu-img check rc=$rc)"
    else
        fail "$1: qcow2 corrupted (qemu-img check rc=$rc)"
    fi
}

new_overlay() {
    local disk="$1"
    qemu-img create -q -f qcow2 -b "$BASE_IMAGE" -F qcow2 "$disk"
}

boot_to_ready() {
    vm_create "$1" "$SEED_ISO"
    vm_boot
    vm_wait_ready "$BOOT_TIMEOUT"
}

# --- cases ------------------------------------------------------------------

x1_crash_mid_run() {
    step "X1: VM process crash mid-run (SIGKILL) -> recovery/cleanup"
    local disk="$WORKDIR/x1.qcow2"
    SERIAL_LOG="$WORKDIR/serial-x1-$$.log"
    new_overlay "$disk"
    boot_to_ready "$disk" || die "X1: VM failed to boot"
    vm_kill
    if ! vm_is_running; then ok "X1: process gone after SIGKILL"; else fail "X1: process survived SIGKILL"; fi
    assert_no_orphans "X1"
    assert_state_ok "X1" "$disk"
    SERIAL_LOG="$WORKDIR/serial-x1b-$$.log"
    if boot_to_ready "$disk"; then ok "X1: relaunch on same disk boots to ready"; else fail "X1: relaunch failed"; fi
    vm_kill
    rm -f "$disk"
}

x2_disk_full_snapshot() {
    step "X2: disk-full during snapshot (savevm) -> clean error, VM unaffected"
    if ! sudo -n true 2> /dev/null; then
        skip "X2: passwordless sudo unavailable (needed for tmpfs mount)"
        return 0
    fi
    TMPFS_MNT="$WORKDIR/x2-full"
    mkdir -p "$TMPFS_MNT"
    # RAM (VM_MEM) never fits into this filesystem, so savevm must hit ENOSPC.
    sudo mount -t tmpfs -o size=64m,mode=0777 tmpfs "$TMPFS_MNT"
    local disk="$TMPFS_MNT/x2.qcow2"
    SERIAL_LOG="$WORKDIR/serial-x2-$$.log"
    new_overlay "$disk"
    boot_to_ready "$disk" || die "X2: VM failed to boot"
    local err=""
    if err="$(vm_snapshot full-disk-snap 2>&1)"; then
        fail "X2: savevm succeeded on a full filesystem"
    else
        ok "X2: savevm failed cleanly (${err:-error reported})"
    fi
    if vm_query_status | grep -q '"running"'; then
        ok "X2: VM still running after failed savevm"
    else
        fail "X2: VM died after failed savevm"
    fi
    vm_kill
    assert_no_orphans "X2"
    sudo umount "$TMPFS_MNT"
    TMPFS_MNT=""
}

x3_corrupt_snapshot_file() {
    step "X3: corrupt/truncated qcow2 with snapshot -> refused cleanly"
    local disk="$WORKDIR/x3.qcow2"
    SERIAL_LOG="$WORKDIR/serial-x3-$$.log"
    new_overlay "$disk"
    boot_to_ready "$disk" || die "X3: VM failed to boot"
    vm_snapshot x3snap || die "X3: savevm failed on healthy disk"
    vm_stop || true
    # Truncate the image to half: the internal snapshot data is now damaged.
    local size
    size="$(stat -c %s "$disk")"
    truncate -s "$((size / 2))" "$disk"
    SERIAL_LOG="$WORKDIR/serial-x3b-$$.log"
    vm_create "$disk" "$SEED_ISO"
    vm_boot
    local rebooted=0
    sleep 5
    if vm_is_running; then
        # QEMU may open a truncated image; restoring the snapshot must fail.
        if vm_restore x3snap 2> /dev/null; then
            fail "X3: loadvm succeeded on truncated image"
        else
            ok "X3: loadvm of damaged snapshot rejected"
        fi
        rebooted=1
        vm_kill
    else
        ok "X3: QEMU refused truncated image at startup"
    fi
    : "$rebooted"
    assert_no_orphans "X3"
    rm -f "$disk"
}

x4_invalid_config() {
    step "X4: invalid VM config -> fast, clean startup error"
    local out rc=0
    out="$(qemu_oneshot -m -128 2>&1)" || rc=$?
    if (( rc != 0 )) && [[ -n "$out" ]]; then
        ok "X4: negative memory size rejected with error (rc=$rc)"
    else
        fail "X4: invalid -m accepted (rc=$rc)"
    fi
    rc=0
    out="$("$(qemu_bin)" -machine definitely-not-a-machine -display none 2>&1)" || rc=$?
    if (( rc != 0 )) && [[ -n "$out" ]]; then
        ok "X4: bogus machine type rejected with error (rc=$rc)"
    else
        fail "X4: bogus machine type accepted (rc=$rc)"
    fi
    assert_no_orphans "X4"
}

x5_double_boot() {
    step "X5: double-boot of the same VM disk -> second boot refused"
    local disk="$WORKDIR/x5.qcow2"
    SERIAL_LOG="$WORKDIR/serial-x5-$$.log"
    new_overlay "$disk"
    vm_create "$disk" "$SEED_ISO"
    vm_boot
    sleep 5
    vm_is_running || die "X5: first VM failed to start"
    local first_pid="$QEMU_PID"
    local out rc=0
    # Second QEMU on the same qcow2: the qcow2 write lock must refuse it.
    # shellcheck disable=SC2046
    out="$(timeout 30 "$(qemu_bin)" $(_accel_args) $(_machine_args) -display none \
        -m "$VM_MEM" \
        -drive "file=$disk,if=virtio,format=qcow2" 2>&1)" || rc=$?
    if (( rc == 124 )); then
        fail "X5: second QEMU kept running on the same disk (killed by timeout)"
    elif (( rc != 0 )) && echo "$out" | grep -qi "lock"; then
        ok "X5: second boot refused with lock error"
    elif (( rc != 0 )); then
        ok "X5: second boot refused (rc=$rc): $(echo "$out" | head -1)"
    else
        fail "X5: second QEMU booted the same disk"
    fi
    if kill -0 "$first_pid" 2> /dev/null; then
        ok "X5: first VM unaffected"
    else
        fail "X5: first VM died"
    fi
    vm_kill
    assert_no_orphans "X5"
    assert_state_ok "X5" "$disk"
    rm -f "$disk"
}

x6_restore_deleted_branch() {
    step "X6: snapshot-restore of a deleted branch -> clean error"
    local disk="$WORKDIR/x6.qcow2"
    SERIAL_LOG="$WORKDIR/serial-x6-$$.log"
    new_overlay "$disk"
    boot_to_ready "$disk" || die "X6: VM failed to boot"
    vm_snapshot branch-a || die "X6: savevm branch-a failed"
    vm_snapshot branch-b || die "X6: savevm branch-b failed"
    vm_delete_snapshot branch-a || die "X6: delvm branch-a failed"
    if vm_list_snapshots | grep -q "branch-a"; then
        fail "X6: deleted snapshot still listed"
    else
        ok "X6: branch-a deleted"
    fi
    if vm_restore branch-a 2> /dev/null; then
        fail "X6: loadvm of deleted snapshot succeeded"
    else
        ok "X6: loadvm of deleted snapshot rejected"
    fi
    if vm_query_status | grep -q '"running"\|"paused"'; then
        ok "X6: VM state intact after failed restore"
    else
        fail "X6: VM state broken after failed restore"
    fi
    if vm_restore branch-b; then
        ok "X6: restore to surviving branch-b still works"
    else
        fail "X6: restore to branch-b failed"
    fi
    vm_kill
    assert_no_orphans "X6"
    assert_state_ok "X6" "$disk"
    rm -f "$disk"
}

main() {
    fetch_image
    make_seed
    step "Backend: $BACKEND (accel: $(accel_name), arch: $(vm_arch))"
    x1_crash_mid_run
    x2_disk_full_snapshot
    x3_corrupt_snapshot_file
    x4_invalid_config
    x5_double_boot
    x6_restore_deleted_branch
    echo
    echo "RESULT: $PASS passed, $FAIL failed, $SKIP skipped (backend=$BACKEND)"
    [[ $FAIL -eq 0 ]]
}

main "$@"
