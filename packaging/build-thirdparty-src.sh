#!/usr/bin/env bash
# Build vmforge-thirdparty-src-<version>.tar: the corresponding-source
# bundle for third-party components redistributed in this release, built
# in the SAME workflow run as the binaries so binary and source cannot
# drift (GPL §3(a) plan input for the gated download page — see
# docs/release-pipeline.md).
set -euo pipefail

VERSION="$1"
OUTDIR="$2"

mkdir -p "$OUTDIR"
WORK="$(mktemp -d)"
SRCROOT="$WORK/vmforge-thirdparty-src-${VERSION}"
mkdir -p "$SRCROOT"

# Vendored sources of all Rust crate dependencies compiled into the
# shipped binaries (exact versions from Cargo.lock of this commit).
cargo vendor --locked "$SRCROOT/cargo-vendor" > "$SRCROOT/cargo-vendor-config.toml"
cp Cargo.lock "$SRCROOT/"

cat > "$SRCROOT/README" <<EOF
VMForge third-party corresponding source bundle — version ${VERSION}

Contents:
  cargo-vendor/            Exact sources of every Rust crate dependency
                           statically linked into the vmforge binaries in
                           this release (versions pinned by Cargo.lock).
  Cargo.lock               Dependency lockfile of the released commit.
  QEMU-NOT-REDISTRIBUTED   Wave-1 QEMU dependency statement (see below).

QEMU (GPL-2.0): wave-1 Linux artifacts do NOT redistribute QEMU. The .deb
declares a dependency on the distro package qemu-system-x86 and the
.AppImage requires host QEMU/KVM; because we do not distribute QEMU
binaries, GPL §3 source obligations for QEMU do not attach to these
artifacts. The moment any artifact bundles QEMU, its exact pinned source
tree + our patches + build scripts MUST be added to this bundle in the
same workflow run (GPL §3(a): source alongside binaries behind the same
gate; https://www.gnu.org/licenses/gpl-faq.html#SourceAndBinaryOnDifferentSites).

This bundle is produced by .github/workflows/release.yml in the same run
that builds the binaries, and is published next to them on the gated
download page (GAP-4).
EOF

cat > "$SRCROOT/QEMU-NOT-REDISTRIBUTED" <<EOF
Wave 1 uses distro-provided QEMU (Debian/Ubuntu: qemu-system-x86).
No QEMU binaries are redistributed in vmforge ${VERSION} artifacts.
EOF

tar -C "$WORK" -cf "$OUTDIR/vmforge-thirdparty-src-${VERSION}.tar" \
  "vmforge-thirdparty-src-${VERSION}"
rm -rf "$WORK"
