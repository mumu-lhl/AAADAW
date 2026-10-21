#!/usr/bin/env python3
"""Write the deterministic 10-second REAPER item-copy probe source WAV."""

from __future__ import annotations

import math
import struct
import sys
import wave
from pathlib import Path


def main() -> None:
    destination = Path(
        sys.argv[1] if len(sys.argv) > 1 else "target/reaper-dock-review/item-copy-loop-source.wav"
    )
    destination.parent.mkdir(parents=True, exist_ok=True)
    sample_rate = 48_000
    frame_count = sample_rate * 10
    with wave.open(str(destination), "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(sample_rate)
        frames = bytearray()
        for index in range(frame_count):
            sample = round(12_000 * math.sin(2 * math.pi * 440 * index / sample_rate))
            frames.extend(struct.pack("<h", sample))
        output.writeframes(frames)


if __name__ == "__main__":
    main()
