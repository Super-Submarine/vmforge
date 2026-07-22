#!/usr/bin/env python3
"""Generate the placeholder VMForge app icon (solid 256x256 PNG)."""
import struct
import sys
import zlib


def png(width: int, height: int, rgb: tuple[int, int, int]) -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data))
        )

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    row = b"\x00" + bytes(rgb) * width
    idat = zlib.compress(row * height)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", idat)
        + chunk(b"IEND", b"")
    )


if __name__ == "__main__":
    with open(sys.argv[1], "wb") as f:
        f.write(png(256, 256, (0x2E, 0x5C, 0x8A)))
