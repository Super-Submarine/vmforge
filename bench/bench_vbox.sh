#!/usr/bin/env bash
# Benchmark VirtualBox: cold boot, live snapshot create, snapshot restore,
# savestate/resume (instant resume), and snapshot storage overhead, for the
# same Alpine guest used by the QEMU benchmarks.
#
# Requires: VirtualBox with a working vboxdrv kernel module (check with
# `VBoxManage --version` and `ls /dev/vboxdrv`). On hosts running a custom
# kernel without matching headers (e.g. this benchmark's original CI host,
# kernel 5.15.200), vboxdrv cannot be built and this script exits with a
# machine-readable "blocked" result instead of numbers.
#
# Usage: bench_vbox.sh [iterations]
set -euo pipefail
cd "$(dirname "$0")"
ITER="${1:-5}"
mkdir -p results work
VM=vmforge-bench
BASE=images/alpine-nocloud.qcow2
SERIAL=$PWD/work/vbox.serial

blocked() {
    cat > results/virtualbox.json <<EOF
{
  "stack": "VirtualBox",
  "status": "blocked",
  "reason": "$1",
  "iterations": []
}
EOF
    echo "VirtualBox benchmark BLOCKED: $1" >&2
    exit 0
}

command -v VBoxManage >/dev/null || blocked "VBoxManage not installed"
[ -e /dev/vboxdrv ] || blocked "vboxdrv kernel module not loaded (no matching kernel headers on this host)"
[ -f "$BASE" ] || { echo "missing $BASE; run fetch_assets.sh" >&2; exit 1; }

wait_marker() { # wait_marker <file> <timeout_s>; echoes elapsed seconds
    local f=$1 t=$2 start elapsed
    start=$(date +%s.%N)
    for _ in $(seq 1 $((t * 20))); do
        if grep -aq "login:" "$f" 2>/dev/null; then
            elapsed=$(echo "$(date +%s.%N) - $start" | bc)
            echo "$elapsed"; return 0
        fi
        sleep 0.05
    done
    echo "timeout waiting for login prompt in $f" >&2; return 1
}

cleanup_vm() {
    VBoxManage controlvm "$VM" poweroff >/dev/null 2>&1 || true
    sleep 1
    VBoxManage unregistervm "$VM" --delete >/dev/null 2>&1 || true
}
trap cleanup_vm EXIT

vm_dir_bytes() {
    du -sb "$(VBoxManage showvminfo "$VM" --machinereadable | grep -oP '(?<=CfgFile=").*(?=/)')" | cut -f1
}

echo '{"stack":"VirtualBox","status":"ok","iterations":[' > results/virtualbox.json.tmp
for i in $(seq 1 "$ITER"); do
    echo "[virtualbox] iteration $i/$ITER" >&2
    cleanup_vm
    rm -f work/vbox.vdi "$SERIAL"
    qemu-img convert -f qcow2 -O vdi "$BASE" work/vbox.vdi

    VBoxManage createvm --name "$VM" --ostype Linux26_64 --register >/dev/null
    VBoxManage modifyvm "$VM" --memory 512 --cpus 1 --nic1 none \
        --uart1 0x3F8 4 --uartmode1 file "$SERIAL" --graphicscontroller vmsvga >/dev/null
    VBoxManage storagectl "$VM" --name sata --add sata --controller IntelAhci >/dev/null
    VBoxManage storageattach "$VM" --storagectl sata --port 0 --device 0 \
        --type hdd --medium "$PWD/work/vbox.vdi" >/dev/null

    # Cold boot
    t0=$(date +%s.%N)
    VBoxManage startvm "$VM" --type headless >/dev/null
    boot_s=$(wait_marker "$SERIAL" 180)

    # Live snapshot create (includes RAM state when VM is running)
    size_before=$(vm_dir_bytes)
    t0=$(date +%s.%N)
    VBoxManage snapshot "$VM" take bench-snap --live >/dev/null
    snap_s=$(echo "$(date +%s.%N) - $t0" | bc)
    overhead=$(( $(vm_dir_bytes) - size_before ))

    # Instant resume path: savestate then start from saved state
    VBoxManage controlvm "$VM" savestate >/dev/null
    t0=$(date +%s.%N)
    VBoxManage startvm "$VM" --type headless >/dev/null
    # wait until VM reports running
    while ! VBoxManage showvminfo "$VM" --machinereadable | grep -q 'VMState="running"'; do sleep 0.05; done
    resume_s=$(echo "$(date +%s.%N) - $t0" | bc)

    # Snapshot restore (VirtualBox requires the VM to be powered off first;
    # the poweroff is excluded from the measured restore time)
    VBoxManage controlvm "$VM" poweroff >/dev/null 2>&1 || true
    sleep 1
    t0=$(date +%s.%N)
    VBoxManage snapshot "$VM" restore bench-snap >/dev/null
    VBoxManage startvm "$VM" --type headless >/dev/null
    while ! VBoxManage showvminfo "$VM" --machinereadable | grep -q 'VMState="running"'; do sleep 0.05; done
    restore_s=$(echo "$(date +%s.%N) - $t0" | bc)

    [ "$i" -gt 1 ] && echo ',' >> results/virtualbox.json.tmp
    printf '{"boot_s":%s,"snapshot_create_s":%s,"snapshot_restore_s":%s,"resume_from_disk_s":%s,"storage_overhead_bytes":%s}' \
        "$boot_s" "$snap_s" "$restore_s" "$resume_s" "$overhead" >> results/virtualbox.json.tmp
done
echo ']}' >> results/virtualbox.json.tmp
mv results/virtualbox.json.tmp results/virtualbox.json
echo "wrote results/virtualbox.json"
