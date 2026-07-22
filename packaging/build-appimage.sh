#!/usr/bin/env bash
# Build the vmforge .AppImage from the release binary using a pinned
# appimagetool. Wave 1 does not bundle QEMU (host QEMU/KVM required) —
# see docs/release-pipeline.md.
set -euo pipefail

VERSION="$1"
OUTDIR="$2"
BIN="target/release/vmforge"
ARCH="x86_64"

# Pinned appimagetool release (checksum-verified).
APPIMAGETOOL_URL="https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage"
APPIMAGETOOL_SHA256="ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0"

mkdir -p "$OUTDIR"
WORK="$(mktemp -d)"
APPDIR="$WORK/VMForge.AppDir"
mkdir -p "$APPDIR/usr/bin"

install -m 0755 "$BIN" "$APPDIR/usr/bin/vmforge"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/vmforge" "$@"
EOF
chmod +x "$APPDIR/AppRun"

cat > "$APPDIR/vmforge.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=VMForge
Exec=vmforge
Icon=vmforge
Categories=System;Emulator;
Terminal=true
EOF

python3 packaging/gen_icon.py "$APPDIR/vmforge.png"

curl -fsSL -o "$WORK/appimagetool" "$APPIMAGETOOL_URL"
echo "$APPIMAGETOOL_SHA256  $WORK/appimagetool" | sha256sum -c -
chmod +x "$WORK/appimagetool"

# --appimage-extract-and-run avoids needing FUSE on CI runners.
ARCH="$ARCH" "$WORK/appimagetool" --appimage-extract-and-run \
  "$APPDIR" "$OUTDIR/vmforge-${VERSION}-${ARCH}.AppImage"
rm -rf "$WORK"
