#!/usr/bin/env bash
# Console viewer spike: start a local QEMU guest exposing a VNC display.
# No disk image required — the SeaBIOS/iPXE boot screen is enough to prove
# the GUI -> VNC -> QEMU path end to end.
set -euo pipefail

DISPLAY_NUM="${1:-1}"   # -vnc :1  => TCP 127.0.0.1:5901

exec qemu-system-x86_64 \
  -name vmforge-spike \
  -m 512 \
  -smp 1 \
  -vnc "127.0.0.1:${DISPLAY_NUM}" \
  -monitor none \
  -serial none \
  -display none
