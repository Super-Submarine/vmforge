# shellcheck shell=bash
# Default driver: plain qemu-system-* controlled over QMP.
# Implements the driver interface documented in qa/README.md.
#
# Backend selection (cross-backend matrix): set BACKEND to one of
#   auto        (default) KVM if /dev/kvm is writable, else TCG — x86_64
#   kvm         require KVM — x86_64 (fails if /dev/kvm is unavailable)
#   tcg         force TCG — x86_64
#   tcg-aarch64 TCG-emulated aarch64 (the CI stand-in for the HVF/ARM path)
# FORCE_TCG=1 is kept as a back-compat alias for BACKEND=tcg.

BACKEND="${BACKEND:-auto}"
if [[ "${FORCE_TCG:-0}" == "1" && "$BACKEND" == "auto" ]]; then
    BACKEND=tcg
fi

QEMU_PID=""
QMP_SOCK=""
DISK=""

vm_arch() {
    case "$BACKEND" in
        tcg-aarch64) echo aarch64 ;;
        *) echo x86_64 ;;
    esac
}

qemu_bin() {
    if [[ -n "${QEMU_BIN:-}" ]]; then
        echo "$QEMU_BIN"
    else
        echo "qemu-system-$(vm_arch)"
    fi
}

_aarch64_fw() {
    local f
    for f in "${AARCH64_FW:-}" \
        /usr/share/qemu-efi-aarch64/QEMU_EFI.fd \
        /usr/share/AAVMF/AAVMF_CODE.fd; do
        [[ -n "$f" && -r "$f" ]] && { echo "$f"; return 0; }
    done
    echo "no aarch64 UEFI firmware found (install qemu-efi-aarch64)" >&2
    return 1
}

_machine_args() {
    case "$BACKEND" in
        tcg-aarch64)
            local fw
            fw="$(_aarch64_fw)" || return 1
            echo "-machine virt -cpu cortex-a57 -bios $fw"
            ;;
        *)
            echo "-machine q35"
            ;;
    esac
}

_accel_args() {
    case "$BACKEND" in
        kvm) echo "-accel kvm" ;;
        tcg | tcg-aarch64) echo "-accel tcg" ;;
        auto)
            if [[ -w /dev/kvm ]]; then echo "-accel kvm"; else echo "-accel tcg"; fi
            ;;
        *)
            echo "unknown BACKEND: $BACKEND" >&2
            return 1
            ;;
    esac
}

accel_name() {
    case "$BACKEND" in
        kvm) echo kvm ;;
        tcg) echo tcg ;;
        tcg-aarch64) echo "tcg (aarch64)" ;;
        auto) if [[ -w /dev/kvm ]]; then echo kvm; else echo tcg; fi ;;
    esac
}

vm_create() {
    DISK="$1"
    SEED_ISO="$2"
    QMP_SOCK="$WORKDIR/qmp-$$.sock"
    rm -f "$QMP_SOCK"
}

vm_boot() {
    # shellcheck disable=SC2046,SC2086
    "$(qemu_bin)" \
        $(_accel_args) $(_machine_args) \
        -m "${VM_MEM:-512}" -smp 1 \
        -drive "file=$DISK,if=virtio,format=qcow2" \
        -drive "file=$SEED_ISO,if=virtio,format=raw,media=cdrom,read-only=on" \
        -netdev user,id=n0 -device virtio-net-pci,netdev=n0 \
        -display none \
        -serial "file:$SERIAL_LOG" \
        -qmp "unix:$QMP_SOCK,server,nowait" \
        ${VM_EXTRA_ARGS:-} \
        &
    QEMU_PID=$!
}

# Run a one-shot QEMU invocation (no QMP, no lifecycle) with the backend's
# accel/machine args plus the given extra args. Used by negative tests.
qemu_oneshot() {
    # shellcheck disable=SC2046
    "$(qemu_bin)" $(_accel_args) $(_machine_args) -display none "$@"
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

vm_delete_snapshot() { _hmp_strict "delvm $1"; }

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
