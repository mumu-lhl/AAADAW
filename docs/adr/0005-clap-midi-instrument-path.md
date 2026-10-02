# Use CLAP instruments for MVP MIDI playback

## Decision

MVP MIDI playback will use an in-process CLAP instrument host built with `clack`. The smallest usable host and MIDI-to-audio route are pulled into Phase 4 so that the piano roll can produce sound. Plugin browsing, full plugin state persistence, effect chains, and crash isolation remain later work.

The first vertical slice will support one instrument on a MIDI track. The DAW will not ship a bundled instrument until its license and redistribution terms are explicitly reviewed; initial use can load an instrument installed by the user.

## Alternatives considered

- A built-in synthesizer would avoid plugin-host work, but would introduce an instrument sound engine and its own sound quality and licensing decisions.
- A bundled third-party instrument would make first run easier but requires a specific instrument and redistribution grant.

## Consequences

- MIDI playback depends on a compatible CLAP instrument being installed by the user for the first slice.
- The host must keep plugin discovery/loading and all non-real-time work off the audio callback. Plugin processing and MIDI delivery must obey the real-time audio-thread constraints.
- In-process plugins can crash or stall the DAW. The MVP must report load/processing failures accurately and must not claim crash isolation; process isolation remains a later milestone.
- This decision follows the existing architecture’s CLAP-first direction and does not authorize VST3 support or a bundled instrument.
