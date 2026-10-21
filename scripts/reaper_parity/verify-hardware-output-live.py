#!/usr/bin/env python3
"""Verify captured REAPER Hardware Output samples against the probe matrix."""

from __future__ import annotations

import hashlib
import struct
import sys
import wave
from pathlib import Path


def read_stereo(path: Path) -> tuple[list[tuple[int, int]], str]:
    with wave.open(str(path), "rb") as capture:
        assert capture.getnchannels() == 2, f"{path}: expected stereo audio"
        assert capture.getframerate() == 44_100, f"{path}: expected 44100 Hz"
        assert capture.getsampwidth() == 2, f"{path}: expected 16-bit PCM"
        raw = capture.readframes(capture.getnframes())
    samples = [sample[0] for sample in struct.iter_unpack("<h", raw)]
    frames = list(zip(samples[::2], samples[1::2], strict=True))
    return frames, hashlib.sha256(path.read_bytes()).hexdigest()


def active(frames: list[tuple[int, int]]) -> list[tuple[int, int]]:
    return [frame for frame in frames if frame != (0, 0)]


def main() -> None:
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(
        "docs/verification/reaper-parity/fixtures/hardware-output-live"
    )
    manifest = dict(
        line.rstrip("\n").split("\t", 1)
        for line in (root / "live-evidence.tsv").read_text().splitlines()
    )
    for key, expected in manifest.items():
        if not key.endswith("_sha256"):
            continue
        artifact = root / key.removesuffix("_sha256")
        if artifact.is_file():
            actual = hashlib.sha256(artifact.read_bytes()).hexdigest()
            assert actual == expected, f"{artifact}: SHA-256 {actual} != manifest {expected}"
    for key, filename in (
        ("probe_script_sha256", "probe-hardware-output-live.lua"),
        ("verification_script_sha256", "verify-hardware-output-live.py"),
        ("capture_script_sha256", "capture-hardware-output-live.sh"),
    ):
        actual = hashlib.sha256((root / filename).read_bytes()).hexdigest()
        assert actual == manifest[key], f"{filename}: hash does not match {key}"

    captures = {
        name: read_stereo(root / f"{name}.wav")
        for name in (
            "source",
            "main",
            "routed",
            "muted",
            "half",
            "phase",
            "pan-right",
            "mono-1-stereo",
            "mono-1-output-2",
            "track-half",
            "pre-fx",
            "post-fx-2",
            "post-fx-3",
            "track-muted",
            "source-solo",
            "other-solo",
            "master-bypass",
        )
    }
    frames = {name: result[0] for name, result in captures.items()}
    source = frames["source"]
    source_left_peak = max(abs(left) for left, _ in source)
    source_right_peak = max(abs(right) for _, right in source)
    expected = {
        "main": (source_left_peak, source_right_peak),
        "routed": (source_left_peak, source_right_peak),
        "half": (source_left_peak // 2, source_right_peak // 2),
        "track-half": (source_left_peak // 2, source_right_peak // 2),
        "pre-fx": (source_left_peak, source_right_peak),
        "post-fx-2": (source_left_peak // 2, source_right_peak // 2),
        "post-fx-3": (source_left_peak // 2, source_right_peak // 2),
        "source-solo": (source_left_peak, source_right_peak),
        "master-bypass": (source_left_peak, source_right_peak),
        "pan-right": (0, source_right_peak),
        "mono-1-stereo": (source_left_peak, source_left_peak),
        "mono-1-output-2": (0, source_left_peak),
    }
    for name, (left_peak, right_peak) in expected.items():
        actual = (
            max(abs(left) for left, _ in frames[name]),
            max(abs(right) for _, right in frames[name]),
        )
        assert actual == (left_peak, right_peak), f"{name}: peaks {actual} != {(left_peak, right_peak)}"
        assert active(frames[name]), f"{name}: expected a non-silent output"

    for name in ("muted", "track-muted", "other-solo"):
        assert not any(left or right for left, right in frames[name]), f"{name} output was not silent"
    source_active = active(source)
    phase_active = active(frames["phase"])
    assert len(phase_active) == len(source_active), "phase output did not capture the full source"
    assert phase_active == [(-left, -right) for left, right in source_active], (
        "phase-inverted Hardware Output samples differ from the inverted source"
    )
    assert all(left == right for left, right in frames["mono-1-stereo"]), (
        "mono source channel 1 was not duplicated to both device channels"
    )
    assert all(left == 0 for left, _ in frames["pan-right"]), "hard-right pan leaked into device channel 1"
    assert all(left == 0 for left, _ in frames["mono-1-output-2"]), (
        "mono source sent to device channel 2 leaked into device channel 1"
    )
    for name, mode in (("pre-fx", 1), ("post-fx-2", 2), ("post-fx-3", 3)):
        state = (root / f"{name}.state").read_text()
        assert f"send_mode={mode}" in state, f"{name}: REAPER did not retain I_SENDMODE={mode}"
    assert "source_solo=true" in (root / "source-solo.state").read_text()
    assert "master_fx_and_fader=true" in (root / "master-bypass.state").read_text()
    for name in captures.keys() - {"source"}:
        project = (root / f"{name}.rpp").read_text()
        assert 'FILE "source.wav"' in project, f"{name}.rpp does not use the fixture-local source"
        assert "/tmp/" not in project, f"{name}.rpp contains a temporary path"

    for name, (audio, digest) in captures.items():
        left_peak = max(abs(left) for left, _ in audio)
        right_peak = max(abs(right) for _, right in audio)
        print(
            f"{name}\tframes={len(audio)}\tactive={len(active(audio))}"
            f"\tpeak={left_peak}/{right_peak}\tsha256={digest}"
        )
    print(
        "verified\tMain Send baseline; independent stereo Hardware Output; mute; gain; phase; pan;"
        " channel maps; fader; pre-FX; deprecated/current post-FX; track mute/solo; Master bypass"
    )
    print("verified\tfixture-local RPP media paths and all available SHA-256 manifest entries")


if __name__ == "__main__":
    main()
