# shellcheck shell=bash
# Default driver: plain qemu-system-x86_64 controlled over QMP.
# Implements the driver interface documented in qa/README.md.

QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"
QEMU_PID=""
QMP_SOCK=""
DISK=""

_accel_args() {
    if [[ "${FORCE_TCG:-0}" != "1" && -w /dev/kvm ]]; then
        echo "-accel kvm"
    else
        echo "-accel tcg"
    fi
}

accel_name() {
    if [[ "${FORCE_TCG:-0}" != "1" && -w /dev/kvm ]]; then echo kvm; else echo tcg; fi
}

vm_create() {
    DISK="$1"
    SEED_ISO="$2"
    QMP_SOCK="$WORKDIR/qmp-$$.sock"
    rm -f "$QMP_SOCK"
}

vm_boot() {
    # shellcheck disable=SC2046
    "$QEMU_BIN" \
        $(_accel_args) \
        -m "${VM_MEM:-512}" -smp 1 \
        -machine q35 \
        -drive "file=$DISK,if=virtio,format=qcow2" \
        -drive "file=$SEED_ISO,if=virtio,format=raw,media=cdrom,read-only=on" \
        -netdev user,id=n0 -device virtio-net-pci,netdev=n0 \
        -display none \
        -serial "file:$SERIAL_LOG" \
        -qmp "unix:$QMP_SOCK,server,nowait" \
        &
    QEMU_PID=$!
}

vm_wait_ready() {
    local timeout="$1" waited=0
    local pattern="${GUEST_LOGIN_REGEX:-login:}|VMFORGE_CLOUD_INIT_DONE"
    while (( waited < timeout )); do
        if [[ -f "$SERIAL_LOG" ]] && grep -Eq "$pattern" "$SERIAL_LOG"; then
            return 0
        fi
        vm_is_running || { echo "QEMU exited during boot" >&2; return 1; }
        sleep 2; waited=$((waited + 2))
    done
    echo "timed out after ${timeout}s waiting for: $pattern" >&2
    return 1
}

_qmp() { python3 "$SCRIPT_DIR/qmp.py" "$QMP_SOCK" "$@"; }

# HMP failures arrive as a successful QMP reply whose payload is the error text,
# so treat any non-empty output from savevm/loadvm as a failure.
_hmp_strict() {
    local out
    out="$(_qmp human-monitor-command "$1")" || return 1
    out="${out//\\r/}"; out="${out//\\n/}"; out="${out//\"/}"
    if [[ -n "${out//[[:space:]]/}" ]]; then
        echo "$out" >&2
        return 1
    fi
}

vm_snapshot() { _hmp_strict "savevm $1"; }

vm_restore() { _hmp_strict "loadvm $1"; }

vm_query_status() { _qmp query-status; }

vm_list_snapshots() { _qmp human-monitor-command "info snapshots"; }

vm_stop() {
    _qmp system_powerdown > /dev/null || true
    local waited=0
    while (( waited < 60 )); do
        vm_is_running || return 0
        sleep 2; waited=$((waited + 2))
    done
    echo "graceful shutdown timed out; killing" >&2
    vm_kill
    return 1
}

vm_kill() {
    [[ -n "$QEMU_PID" ]] && kill -9 "$QEMU_PID" 2> /dev/null || true
    wait "$QEMU_PID" 2> /dev/null || true
}

vm_is_running() { [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2> /dev/null; }
