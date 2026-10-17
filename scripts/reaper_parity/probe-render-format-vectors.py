#!/usr/bin/env python3
"""Generate the stereo PCM24 source used by REAPER 7.82 compressed-format probes."""

import argparse
import struct
from pathlib import Path

SAMPLE_RATE = 48_000
LEVELS = (
    -0.999,
    -0.75,
    -0.5,
    -0.25,
    -0.125,
    -0.1,
    -0.05,
    -0.01,
    0.0,
    0.01,
    0.05,
    0.1,
    0.125,
    0.25,
    0.5,
    0.75,
)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output_directory", type=Path)
    output_directory = parser.parse_args().output_directory
    output_directory.mkdir(parents=True, exist_ok=True)
    source_path = output_directory / "source.wav"
    if source_path.exists():
        parser.error(f"refusing to replace existing fixture: {source_path}")

    frames_per_level = SAMPLE_RATE // len(LEVELS)
    data = bytearray()
    for level in LEVELS:
        source_sample = round((level / 2.0) * (1 << 23))
        packed = (source_sample & 0xFF_FFFF).to_bytes(3, "little")
        stereo_frame = packed + packed
        data.extend(stereo_frame * frames_per_level)

    channels = 2
    bits_per_sample = 24
    block_align = channels * bits_per_sample // 8
    byte_rate = SAMPLE_RATE * block_align
    fmt = struct.pack(
        "<HHIIHH",
        1,
        channels,
        SAMPLE_RATE,
        byte_rate,
        block_align,
        bits_per_sample,
    )
    wav = (
        b"RIFF"
        + struct.pack("<I", 36 + len(data))
        + b"WAVEfmt "
        + struct.pack("<I", len(fmt))
        + fmt
        + b"data"
        + struct.pack("<I", len(data))
        + data
    )
    source_path.write_bytes(wav)
    print(f"created {source_path} ({len(data) // block_align} frames)")
    print("Master mix target levels: " + ", ".join(map(str, LEVELS)))


if __name__ == "__main__":
    main()
