# AAADAW

AAADAW is a Rust digital audio workstation project. The initial implementation is being built as a desktop-first, real-time-safe audio application with a reusable Rust core.

## Project status

The core project model, SQLite storage, MIDI editing actions, tempo mapping, an allocation-free streaming mixer, transport and MIDI event scheduling primitives, in-memory PCM playback, and packet-based Symphonia decoding are implemented. Device I/O, audio-item integration, UI, recording, and plugin hosting remain future work. The system architecture and phased implementation scope are documented in:

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
- `crates/aaadaw-engine`: fixed-topology streaming mixer, transport, MIDI event scheduler, PCM playback, and SPSC queue; device integration is not yet implemented.
- `crates/aaadaw-media`: packet-based Symphonia decoding for background media import; decoded chunks are owned and must stay off the audio callback.
