# AAADAW

AAADAW is a Rust digital audio workstation project. The initial implementation is being built as a desktop-first, real-time-safe audio application with a reusable Rust core.

## Project status

The core project model, SQLite storage, MIDI editing actions, tempo mapping, an allocation-free streaming mixer, transport and MIDI event scheduling primitives, packet-based Symphonia decoding, and a background decode/downmix/resample feeder into the engine's bounded PCM queue are implemented. An optional Linux JACK stereo-output backend is available; project audio-item integration, native PipeWire/WASAPI output, UI, recording, and plugin hosting remain future work. The system architecture and phased implementation scope are documented in:

- [System architecture](docs/design/AAADAW_System_Architecture_Design.md)
- [Implementation roadmap](ROADMAP.md)
- [Project action-history decision](docs/adr/0001-project-action-history.md)

## Development

Requires the stable Rust toolchain with `rustfmt` and `clippy`, plus [`cargo-nextest`](https://nexte.st/) for running tests.

```sh
cargo check --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo xtest
```

`cargo xtest` is a workspace alias for `cargo nextest run --workspace`.

## Workspace

- `crates/aaadaw-core`: platform-independent project actions, state, and domain logic. GUI, audio drivers, and persistence adapters must call through this crate's public `Project` interface rather than mutating project state directly.
- `crates/aaadaw-storage`: `.aaadaw` SQLite persistence through `rusqlite` with its `bundled` SQLite library; a system SQLite installation is not required.
- `crates/aaadaw-engine`: fixed-topology streaming mixer, transport, MIDI event scheduler, PCM playback, and SPSC queue; optional `jack-backend` feature adds Linux JACK output.
- `crates/aaadaw-media`: packet-based Symphonia decoding plus a background worker that downmixes and resamples into the engine's PCM queue; decoder work stays off the audio callback.

To build the JACK backend on Debian/Ubuntu, install `libjack-jackd2-dev` and run `cargo check --workspace --all-targets --features aaadaw-engine/jack-backend`. Opening a live client also requires a running JACK server; route its stereo output ports to hardware with a JACK patchbay. The current backend provides audio output and queued play/stop controls, not MIDI-device output or seeking.
