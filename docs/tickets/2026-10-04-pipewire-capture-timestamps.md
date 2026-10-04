# Preserve PipeWire capture gaps from buffer timestamps

## User scenario

A musician records through PipeWire and expects the take to retain its duration across delayed or
missing input buffers, matching the JACK capture path.

## Expected behavior

- Request `SPA_META_Header` buffer metadata when connecting the input stream.
- Use each buffer's PTS in nanoseconds to compute its first frame relative to the first captured
  buffer, then carry that position and its frame count through the bounded capture queue.
- Preserve forward PTS gaps as silence in WAV output using the existing worker-thread limit.
- Reject missing PTS, corrupted metadata, timestamp regression, invalid ranges, and zero-frame
  buffers whose duration cannot be represented. Never fall back to callback arrival order or
  `Stream::time().ticks`.
- Keep the `RT_PROCESS` callback allocation-free, nonblocking, lock-free, and free of I/O/logging.

## Validation

- Hardware-free tests cover checked PTS-to-frame conversion at 44.1 and 48 kHz and invalid timing.
- Existing capture/recovery/import tests verify the timestamped queue and WAV gap path.
- Linux PipeWire CI compiles the metadata request and buffer inspection, and runs the
  hardware-free PTS conversion tests with the PipeWire feature enabled.
- Physical PipeWire metadata availability and loopback timing require a real PipeWire graph and are
  not claimed by the hardware-free tests.

## API limitation

The pinned PipeWire Rust API does not guarantee every input node supplies the requested header.
See [PipeWire buffer timestamp research](../research/2026-10-04-pipewire-capture-buffer-timestamps.md).
If a node omits usable timing metadata, AAADAW fails the take clearly rather than creating a
shortened recording.
