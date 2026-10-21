#!/usr/bin/env python3
"""Create a deterministic Type 1 SMF fixture for native REAPER import probes."""

import argparse
from pathlib import Path


def vlq(value: int) -> bytes:
    if value < 0 or value > 0x0FFFFFFF:
        raise ValueError("SMF variable-length values must be within 0..0x0fffffff")
    result = [value & 0x7F]
    value >>= 7
    while value:
        result.insert(0, 0x80 | (value & 0x7F))
        value >>= 7
    return bytes(result)


def meta(kind: int, payload: bytes) -> bytes:
    return bytes((0xFF, kind)) + vlq(len(payload)) + payload


def track(events: list[tuple[int, bytes]]) -> bytes:
    events = sorted(enumerate(events), key=lambda entry: (entry[1][0], entry[0]))
    output = bytearray()
    previous_tick = 0
    for _, (tick, data) in events:
        if tick < previous_tick:
            raise ValueError("events must have nondecreasing ticks")
        output.extend(vlq(tick - previous_tick))
        output.extend(data)
        previous_tick = tick
    chunk = b"MTrk" + len(output).to_bytes(4, "big") + output
    return chunk


def make_file() -> bytes:
    conductor = track(
        [
            (0, meta(0x03, b"Conductor")),
            (0, meta(0x51, (500_000).to_bytes(3, "big"))),
            (0, meta(0x58, bytes((4, 2, 24, 8)))),
            (960, meta(0x51, (400_000).to_bytes(3, "big"))),
            (1_920, meta(0x58, bytes((3, 2, 24, 8)))),
            (2_880, meta(0x2F, b"")),
        ]
    )
    lead = track(
        [
            (0, meta(0x03, b"Lead")),
            (0, bytes((0x92, 60, 100))),
            (240, meta(0x01, b"Verse text")),
            (480, bytes((0x82, 60, 37))),
            (480, bytes((0xB2, 0, 11))),
            (480, bytes((0xB2, 32, 22))),
            (480, bytes((0xC2, 41))),
            (960, bytes((0xF0, 3, 0x43, 0x12, 0xF7))),
            (1_200, meta(0x06, b"Cue A")),
            (1_440, meta(0x2F, b"")),
        ]
    )
    meta_only = track(
        [
            (0, meta(0x03, b"Meta only")),
            (0, meta(0x01, b"Unattached text")),
            (480, meta(0x06, b"Unattached marker")),
            (960, meta(0x2F, b"")),
        ]
    )
    chunks = conductor + lead + meta_only
    header = b"MThd" + (6).to_bytes(4, "big") + (1).to_bytes(2, "big")
    header += (3).to_bytes(2, "big") + (960).to_bytes(2, "big")
    return header + chunks


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(make_file())
    print(f"Wrote {args.output} ({args.output.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
