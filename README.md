# AAADAW

AAADAW is a Rust digital audio workstation project. The initial implementation is being built as a desktop-first, real-time-safe audio application with a reusable Rust core.

## Project status

The core project model includes sample-clock `AudioItem`s with undoable edits and SQLite persistence. Audio assets are immutable embedded snapshots by default, with seekable readers, cancellable background import/scan/pack workers, and persisted decoder metadata. `aaadaw-app` resolves embedded and external items into worker-fed render graphs and prepares seek-positioned refills. The Iced 0.14 desktop shell supports track and Audio/MIDI item editing, native file dialogs, audio import and asset maintenance, project actions, and optional Linux JACK or PipeWire playback. MIDI note playback is not yet wired to an instrument; replacing changed embedded snapshots from the UI, recording, and plugin hosting remain future work. See [Desktop shell](#desktop-shell) for the current workspace layout. The system architecture and phased implementation scope are documented in:

- [System architecture](docs/design/AAADAW_System_Architecture_Design.md)
- [UI/UX design contract](DESIGN.md)
- [Implementation roadmap](ROADMAP.md)
- [Project action-history decision](docs/adr/0001-project-action-history.md)

## Desktop shell

The shell has **File**, **Edit**, and **Track** menus, with **Arrangement** selected by default. Arrangement now shows track-aligned Audio/MIDI items on a meter-aware musical ruler, with horizontal zoom/pan, an edit cursor, track/item selection, and a resizable track-control panel. The Inspector retains precise sample/tick and MIDI-note editing. Item dragging, time selection, splitting, waveforms, and a piano roll remain future work.

**Media** contains audio import, source scanning, external-asset packing, and missing-link repair. **Project** contains action search and shortcut help. A transport strip stays at the bottom in every workspace; JACK and PipeWire builds expose backend selection, playback, and seek controls. Native system file pickers handle project open/save, audio import, and relinking; cancelling a picker leaves the existing path field unchanged.

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

- `crates/aaadaw`: Iced desktop shell with REAPER-inspired File/Edit/Track menus, Arrangement by default, separate Media/Project workspaces, a persistent bottom transport, and native file dialogs; undoable track and Audio/MIDI item editing; keyboard shortcuts; and optional JACK/PipeWire transport. Project changes use `aaadaw-core` actions.
- `crates/aaadaw-core`: platform-independent project actions, state, and domain logic, including MIDI and sample-clock audio items. GUI, audio drivers, and persistence adapters must call through this crate's public `Project` interface rather than mutating project state directly.
- `crates/aaadaw-app`: control-layer orchestration for media playback, audio/MIDI timeline-editing intents, and cancellable embedded imports; it prepares seek-positioned graphs, owns background feeders, and returns project `DawAction`s. Optional `jack-backend` and `pipewire-backend` support playback and safe graph replacement.
- `crates/aaadaw-storage`: `.aaadaw` SQLite project persistence and chunked embedded audio assets through `rusqlite` with its `bundled` SQLite library; a system SQLite installation is not required.
- `crates/aaadaw-engine`: fixed-topology streaming mixer, transport, MIDI event scheduler, PCM playback, and SPSC queue; optional `jack-backend` and `pipewire-backend` features add Linux device outputs.
- `crates/aaadaw-media`: packet-based Symphonia decoding plus background workers that accept files or seekable embedded-asset readers, then downmix/resample into the engine's PCM queue; decoder work stays off the audio callback.

To build desktop JACK controls on Debian/Ubuntu, install `libjack-jackd2-dev` and run `cargo check --workspace --all-targets --features aaadaw/jack-backend`. Launch them with `cargo run -p aaadaw --features aaadaw/jack-backend`. Opening a live client also requires a running JACK server; route its stereo output ports to hardware with a JACK patchbay. JACK graph replacement preserves play state and retires old graphs off the audio callback; call `RunningJackPlayback::collect_retired_graphs` to reclaim old feeders. Seek refill currently re-decodes from the item start, so long-item seeks can be slow. MIDI-device output is not implemented.

To build PipeWire playback on Linux, install the PipeWire development package that provides `libpipewire-0.3` and use `cargo check --workspace --all-targets --features aaadaw/pipewire-backend`. Compile both outputs with `--features "aaadaw/jack-backend aaadaw/pipewire-backend"`; the transport lets you choose either backend, with JACK selected initially. PipeWire output negotiates stereo F32LE at the project rate and reports stream/callback errors in the transport status.
