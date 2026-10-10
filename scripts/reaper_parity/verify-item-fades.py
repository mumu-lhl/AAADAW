#!/usr/bin/env python3
"""Verify owned constant-signal REAPER probes; never copy native assets to the repo."""
import argparse
import hashlib
import json
from pathlib import Path
import wave


def gain(shape, x):
    return (
        x,
        x * (2 - x),
        x * x,
        1 - (1 - x) ** 4,
        x ** 4,
        x * x * (3 - 2 * x),
        8 * x ** 4 if x < .5 else 1 - 8 * (1 - x) ** 4,
    )[shape]


def verify(path, shape, requested_in, requested_out):
    with wave.open(str(path)) as wav:
        assert (wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getnframes()) == (2, 3, 48000, 48000)
        pcm = wav.readframes(wav.getnframes())
    fade_in = min(requested_in * 48000, 48000)
    fade_out = min(requested_out * 48000, 48000 - fade_in)
    error = 0.0
    for frame in range(48000):
        if fade_in and frame < fade_in:
            expected = gain(shape, frame / fade_in)
        elif fade_out and 48000 - frame < fade_out:
            expected = gain(shape, (48000 - frame) / fade_out)
        else:
            expected = 1.0
        for channel in range(2):
            start = frame * 6 + channel * 3
            sample = int.from_bytes(pcm[start:start + 3], 'little', signed=True) / (2 ** 23) / .125
            error = max(error, abs(sample - expected))
    assert error <= 1.01 / (2 ** 23) / .125, (path.name, error)
    return {'file': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(), 'max_normalized_error': error}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('probe_directory', type=Path)
    args = parser.parse_args()
    cases = [(f'item-fade-shape-{shape}', shape, .25, .25) for shape in range(7)]
    cases += [
        ('item-fade-overlap-linear', 0, .75, .75),
        ('item-fade-overlap-asymmetric', 0, .5, .75),
        ('item-fade-overlap-full', 0, 1, 1),
        ('item-fade-long-in', 0, 2, 0),
        ('item-fade-long-out', 0, 0, 2),
    ]
    print(json.dumps([verify(args.probe_directory / (name + '.wav'), shape, fade_in, fade_out)
                      for name, shape, fade_in, fade_out in cases], indent=2))


if __name__ == '__main__':
    main()
