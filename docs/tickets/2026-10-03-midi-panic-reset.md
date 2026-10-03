# MIDI Panic and Controller Reset

## Goal

Give musicians a reliable MIDI Panic command and leave CLAP instruments in a known note/sustain state on stop, seek, and graph replacement.

## Scope

- Send tracked CLAP note-offs, CC64-off, CC123 All Notes Off, and CC120 All Sound Off during reset when a MIDI 1.0 input port is available.
- Add a transport Panic button and a searchable Actions command that leaves the playhead and transport state unchanged.
- Keep reset event capacity preallocated on the audio callback; note-only CLAP ports still receive host-tracked note-offs.

## Out of scope

External hardware MIDI output, per-channel projects, pitch-bend/program chase, and configurable reset policy.

## Acceptance

Stop and graph replacement release current notes and reset MIDI-capable CLAP instruments before playback resumes. Panic sends the same resets without changing the playhead or transport state. Note-only plugin ports receive CLAP note-offs. The callback uses bounded queues, preallocated event storage, and no blocking lock.

## Verification

Passed locally: `cargo xtest` (242 tests), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, and `git diff --check`. Focused CLAP controller scheduling and Panic-preserves-transport tests also pass.

Feature-gated JACK/PipeWire backend tests are left to CI because JACK development libraries are unavailable in this environment. Native Iced GUI rendering has not been visually reviewed here.

## Status

Implementation complete; awaiting CI and review.
