# AAADAW

AAADAW is a Rust digital audio workstation project. The initial implementation is being built as a desktop-first, real-time-safe audio application with a reusable Rust core.

## Project status

The core project model now includes sample-clock `AudioItem`s with undoable edits and SQLite persistence. Project audio assets are immutable snapshots embedded as bounded SQLite BLOB chunks and decoded through seekable readers without loading the whole asset into memory. Background import, pack, and source-scan workers report progress and support cancellation; imports stage in short transactions. Symphonia probes and SQLite persists decoder header metadata for embedded assets. The `aaadaw-app` crate resolves embedded and external `AudioItem`s into worker-fed render graphs and prepares seek-positioned refills. An initial Iced 0.14 desktop shell supports track actions, project files, action search, and a basic Audio/MIDI item overview with undoable sample-clock audio nudges (10 ms–1 s) and deletion. Build with the optional Linux JACK feature to enable desktop play/stop/restart controls, playhead display, and arbitrary-sample seek refills. The desktop shell now embeds imported audio into the first track with progress and cancellation; external-link repair/scan/pack UI, native PipeWire/WASAPI output, recording, and plugin hosting remain future work. The system architecture and phased implementation scope are documented in:

- [System architecture](docs/design/AAADAW_System_Architecture_Design.md)
- [Implementation roadmap](ROADMAP.md)
- [Project action-history decision](docs/adr/0001-project-action-history.md)

## Development

Requires the stable Rust toolchain with `rustfmt` and `clippy`, plus [`cargo-nextest`](https://nexte.st/) for running tests.

```sh
cargo run -p aaadaw
cargo run -p aaadaw -- /path/to/project.aaadaw
cargo check --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo xtest
```

`cargo xtest` is a workspace alias for `cargo nextest run --workspace`.

## Workspace

- `crates/aaadaw`: Iced desktop shell with undoable track creation/deletion/rename/reorder and controls, an Audio/MIDI item overview, undoable 10 ms–1 s sample-clock audio-item nudges/deletion, action search, background project open/save, and cancellable audio-file import; optional `jack-backend` adds live transport and item seek. Project changes use `aaadaw-core` actions.
- `crates/aaadaw-core`: platform-independent project actions, state, and domain logic, including MIDI and sample-clock audio items. GUI, audio drivers, and persistence adapters must call through this crate's public `Project` interface rather than mutating project state directly.
- `crates/aaadaw-app`: control-layer orchestration for media playback and cancellable embedded imports; it prepares seek-positioned graphs, owns background feeders, and returns placement `DawAction`s. Optional `jack-backend` supports playback and safe graph replacement.
- `crates/aaadaw-storage`: `.aaadaw` SQLite project persistence and chunked embedded audio assets through `rusqlite` with its `bundled` SQLite library; a system SQLite installation is not required.
- `crates/aaadaw-engine`: fixed-topology streaming mixer, transport, MIDI event scheduler, PCM playback, and SPSC queue; optional `jack-backend` feature adds Linux JACK output.
- `crates/aaadaw-media`: packet-based Symphonia decoding plus background workers that accept files or seekable embedded-asset readers, then downmix/resample into the engine's PCM queue; decoder work stays off the audio callback.

To build desktop JACK controls on Debian/Ubuntu, install `libjack-jackd2-dev` and run `cargo check --workspace --all-targets --features aaadaw/jack-backend`. Launch them with `cargo run -p aaadaw --features aaadaw/jack-backend`. Opening a live client also requires a running JACK server; route its stereo output ports to hardware with a JACK patchbay. JACK graph replacement preserves play state and retires old graphs off the audio callback; call `RunningJackPlayback::collect_retired_graphs` to reclaim old feeders. Seek refill currently re-decodes from the item start, so long-item seeks can be slow. MIDI-device output is not implemented.
