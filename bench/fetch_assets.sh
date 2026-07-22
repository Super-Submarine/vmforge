#!/usr/bin/env bash
# Download all guest images and binaries needed by the benchmark harness.
# Everything lands in bench/images/ and bench/work/ (both gitignored).
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p images work

ALPINE_URL="https://dl-cdn.alpinelinux.org/alpine/v3.22/releases/cloud/nocloud_alpine-3.22.2-x86_64-bios-tiny-r0.qcow2"
FC_VERSION="v1.10.1"
FC_URL="https://github.com/firecracker-microvm/firecracker/releases/download/${FC_VERSION}/firecracker-${FC_VERSION}-x86_64.tgz"
FC_CI="https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.10/x86_64"

if [ ! -f images/alpine-nocloud.qcow2 ]; then
    echo "fetching Alpine nocloud qcow2..."
    curl -fL -o images/alpine-nocloud.qcow2 "$ALPINE_URL"
fi

if [ ! -x work/firecracker ]; then
    echo "fetching firecracker ${FC_VERSION}..."
    curl -fL -o work/fc.tgz "$FC_URL"
    tar -xzf work/fc.tgz -C work
    mv "work/release-${FC_VERSION}-x86_64/firecracker-${FC_VERSION}-x86_64" work/firecracker
    rm -rf work/fc.tgz "work/release-${FC_VERSION}-x86_64"
    chmod +x work/firecracker
fi

if [ ! -f images/fc-vmlinux ]; then
    echo "fetching firecracker CI kernel..."
    KERNEL=$(curl -fsSL "https://s3.amazonaws.com/spec.ccfc.min/?prefix=firecracker-ci/v1.10/x86_64/vmlinux-5.10&list-type=2" \
        | grep -oP '(?<=<Key>)[^<]+' | grep -v -e config -e no-acpi | sort -V | tail -1 || true)
    KERNEL=${KERNEL:-firecracker-ci/v1.10/x86_64/vmlinux-5.10.223}
    curl -fL -o images/fc-vmlinux "https://s3.amazonaws.com/spec.ccfc.min/${KERNEL}"
fi

if [ ! -f images/fc-rootfs.ext4 ]; then
    echo "fetching firecracker CI ubuntu rootfs..."
    curl -fL -o images/fc-rootfs.ext4 "${FC_CI}/ubuntu-22.04.ext4"
fi

echo "assets ready:"
ls -la images
