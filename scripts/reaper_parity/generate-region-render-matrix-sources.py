#!/usr/bin/env python3
"""Create deterministic 48 kHz PCM24 sources for the REAPER matrix render probe."""

from pathlib import Path
import sys
import wave


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} OUTPUT_DIRECTORY")

    output = Path(sys.argv[1])
    output.mkdir(parents=True, exist_ok=True)
    frame_count = 4 * 48_000
    for name, level in (("drums", 0.125), ("bass", 0.25), ("guitar", 0.375)):
        sample = round(level * (1 << 23))
        packed = sample.to_bytes(3, byteorder="little", signed=True)
        frame = packed + packed
        with wave.open(str(output / f"{name}.wav"), "wb") as destination:
            destination.setnchannels(2)
            destination.setsampwidth(3)
            destination.setframerate(48_000)
            destination.writeframes(frame * frame_count)


if __name__ == "__main__":
    main()
