#!/usr/bin/env python3
"""Generate a minimal solid-color placeholder PNG icon, no external deps.

DBnest doesn't have real brand artwork yet (see DESIGN.md's note that the
project name/logo aren't finalized). This produces a plain square icon
purely so `tauri_build::build()` has a real file to point at; it should be
swapped for real artwork before release.
"""
import struct
import sys
import zlib


def make_png(path: str, size: int, rgba: tuple[int, int, int, int]) -> None:
    width = height = size
    raw = bytearray()
    row = bytes(rgba) * width
    for _ in range(height):
        raw += b"\x00" + row  # filter type 0 (none) per scanline

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    sig = b"\x89PNG\r\n\x1a\n"
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    idat = zlib.compress(bytes(raw), 9)
    png = sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


if __name__ == "__main__":
    out, size = sys.argv[1], int(sys.argv[2])
    # Teal square, DBnest's placeholder brand color.
    make_png(out, size, (13, 148, 136, 255))
