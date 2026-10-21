#!/usr/bin/env python3
"""Generate deterministic SMF Format 2 and 25 fps SMPTE import probes."""

from pathlib import Path
import struct


def vlq(value: int) -> bytes:
    result = bytearray([value & 0x7F])
    value >>= 7
    while value:
        result.insert(0, (value & 0x7F) | 0x80)
        value >>= 7
    return bytes(result)


def track(name: str, pitch: int, note_length: int) -> bytes:
    events = bytearray(b"\x00\xff\x03" + vlq(len(name)) + name.encode())
    events.extend(b"\x00\x90" + bytes([pitch, 100]))
    events.extend(vlq(note_length) + bytes([0x80, pitch, 24]))
    events.extend(b"\x00\xff\x2f\x00")
    return bytes(events)


def smf(format_number: int, division: int, tracks: list[bytes]) -> bytes:
    header = struct.pack(">IHHH", 6, format_number, len(tracks), division)
    chunks = b"".join(b"MTrk" + struct.pack(">I", len(data)) + data for data in tracks)
    return b"MThd" + header + chunks


output = Path(__file__).resolve().parents[2] / "target" / "midi-timebase-native"
output.mkdir(parents=True, exist_ok=True)
(output / "format-2-independent-sequences.mid").write_bytes(
    smf(2, 960, [track("Sequence A", 60, 480), track("Sequence B", 64, 960)])
)

smpte_track = bytearray(b"\x00\xff\x03\x05SMPTE" + vlq(250) + b"\x90\x3c\x64")
smpte_track.extend(vlq(500) + b"\x80\x3c\x18")
smpte_track.extend(b"\x00\xff\x2f\x00")
(output / "smpte-25fps-40ticks-per-frame.mid").write_bytes(
    smf(0, 0xE728, [bytes(smpte_track)])
)
