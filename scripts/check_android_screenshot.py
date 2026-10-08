#!/usr/bin/env python3
"""Reject blank Android screenshots and missing small-screen UI labels."""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
import zlib
from pathlib import Path


def paeth(left: int, above: int, upper_left: int) -> int:
    estimate = left + above - upper_left
    distances = (
        abs(estimate - left),
        abs(estimate - above),
        abs(estimate - upper_left),
    )
    return (left, above, upper_left)[distances.index(min(distances))]


def decode_rgba_png(path: Path) -> tuple[int, int, list[bytes]]:
    content = path.read_bytes()
    if content[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("screenshot is not a PNG")

    width = height = bit_depth = color_type = interlace = None
    compressed = bytearray()
    offset = 8
    while offset < len(content):
        length = int.from_bytes(content[offset : offset + 4], "big")
        kind = content[offset + 4 : offset + 8]
        data = content[offset + 8 : offset + 8 + length]
        offset += length + 12
        if kind == b"IHDR":
            width = int.from_bytes(data[0:4], "big")
            height = int.from_bytes(data[4:8], "big")
            bit_depth, color_type, _, _, interlace = data[8:13]
        elif kind == b"IDAT":
            compressed.extend(data)
        elif kind == b"IEND":
            break

    if width is None or height is None or (bit_depth, color_type, interlace) != (8, 6, 0):
        raise ValueError("expected a non-interlaced 8-bit RGBA Android screenshot")

    raw = zlib.decompress(compressed)
    stride = width * 4
    rows: list[bytes] = []
    previous = bytearray(stride)
    cursor = 0
    for _ in range(height):
        filter_kind = raw[cursor]
        cursor += 1
        scanline = bytearray(raw[cursor : cursor + stride])
        cursor += stride
        for index in range(stride):
            left = scanline[index - 4] if index >= 4 else 0
            above = previous[index]
            upper_left = previous[index - 4] if index >= 4 else 0
            if filter_kind == 1:
                scanline[index] = (scanline[index] + left) & 0xFF
            elif filter_kind == 2:
                scanline[index] = (scanline[index] + above) & 0xFF
            elif filter_kind == 3:
                scanline[index] = (scanline[index] + (left + above) // 2) & 0xFF
            elif filter_kind == 4:
                scanline[index] = (scanline[index] + paeth(left, above, upper_left)) & 0xFF
            elif filter_kind != 0:
                raise ValueError(f"unsupported PNG filter {filter_kind}")
        rows.append(bytes(scanline))
        previous = scanline
    return width, height, rows


def recognize_text(path: Path) -> str:
    if shutil.which("tesseract") is None:
        raise RuntimeError("tesseract is required to verify Android screen text")
    result = subprocess.run(
        ["tesseract", str(path), "stdout", "--psm", "11"],
        check=True,
        capture_output=True,
        text=True,
        timeout=30,
    )
    return result.stdout


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: check_android_screenshot.py <screenshot.png>", file=sys.stderr)
        return 2
    try:
        width, height, rows = decode_rgba_png(Path(sys.argv[1]))
    except (OSError, ValueError, zlib.error) as error:
        print(f"Android screenshot check failed: {error}", file=sys.stderr)
        return 1

    # Ignore the status and navigation bars; require visible UI content in the
    # central app surface instead of accepting an all-black Activity window.
    first_row = height // 10
    last_row = height - height // 10
    visible_pixels = 0
    for row in rows[first_row:last_row]:
        for index in range(0, width * 4, 4):
            red, green, blue, alpha = row[index : index + 4]
            if alpha > 0 and max(red, green, blue) > 24:
                visible_pixels += 1
    if visible_pixels < 100:
        print(
            f"Android app surface appears blank ({visible_pixels} visible pixels in {width}x{height})",
            file=sys.stderr,
        )
        return 1
    try:
        recognized_text = recognize_text(Path(sys.argv[1]))
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Android screenshot text check failed: {error}", file=sys.stderr)
        return 1

    if re.search(r"system\s+ui\s+isn't\s+responding", recognized_text, re.IGNORECASE):
        print("Android screenshot shows the System UI not-responding dialog", file=sys.stderr)
        return 1

    missing_labels = [
        label
        for label in ("Arrange", "Mixer")
        if re.search(rf"\b{label}\b", recognized_text, re.IGNORECASE) is None
    ]
    if missing_labels:
        print(
            "Android screenshot is missing visible app labels: " + ", ".join(missing_labels),
            file=sys.stderr,
        )
        if recognized_text.strip():
            print(f"Recognized screenshot text: {recognized_text.strip()}", file=sys.stderr)
        return 1

    print(
        f"Android app surface rendered with Arrange and Mixer labels "
        f"({visible_pixels} visible pixels in {width}x{height})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
