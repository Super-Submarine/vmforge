#!/usr/bin/env bash
# Build the vmforge .deb from the release binary.
# Wave 1 depends on the distro QEMU package (qemu-system-x86) instead of
# redistributing QEMU — see docs/release-pipeline.md for the GPL §3(a) plan.
set -euo pipefail

VERSION="$1"
OUTDIR="$2"
BIN="target/release/vmforge"
ARCH="amd64"
PKGROOT="$(mktemp -d)"

mkdir -p "$OUTDIR" \
  "$PKGROOT/DEBIAN" \
  "$PKGROOT/usr/bin" \
  "$PKGROOT/usr/share/doc/vmforge"

install -m 0755 "$BIN" "$PKGROOT/usr/bin/vmforge"
cp README.md "$PKGROOT/usr/share/doc/vmforge/"

cat > "$PKGROOT/DEBIAN/control" <<EOF
Package: vmforge
Version: ${VERSION}
Section: otherosfs
Priority: optional
Architecture: ${ARCH}
Depends: qemu-system-x86 (>= 1:6.2)
Maintainer: VMForge Release Engineering <release@vmforge.invalid>
Homepage: https://github.com/Super-Submarine/vmforge
Description: Desktop virtualization with git-like VM snapshots
 VMForge CLI: hypervisor abstraction layer with instant-resume VMs and
 git-like snapshot/branching, driving QEMU/KVM as a separate process.
 QEMU is consumed via the distro qemu-system-x86 package (not
 redistributed in this package).
EOF

dpkg-deb --build --root-owner-group "$PKGROOT" \
  "$OUTDIR/vmforge_${VERSION}_${ARCH}.deb"
rm -rf "$PKGROOT"
