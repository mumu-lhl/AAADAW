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
- In-process runtime plugins can crash or stall the DAW. The MVP must report load/processing failures accurately and must not claim runtime crash isolation; runtime process isolation remains a later milestone.
- This decision follows the existing architecture’s CLAP-first direction and does not authorize VST3 support or a bundled instrument.

## Phase 2 follow-up: scanner process boundary

CLAP entry inspection now runs in a short-lived child process launched from the AAADAW executable. The parent passes the entry path and a private response-file path through the versioned `__aaadaw_scan_clap_entry_v1` command. The child returns bounded JSON; the parent enforces a 30-second timeout and a 1 MiB response limit. Child exit, timeout, or invalid protocol output becomes a per-entry scan error, while scanning continues for other entries.

The existing atomic scan cache persists those errors. Startup skips entries with cached scanner-process failures; the explicit Rescan command retries them and replaces the cache. This boundary contains scanner crashes and hangs only. The child has the same user privileges and filesystem access as AAADAW, and loaded plugins still run in-process without a sandbox. Real-time plugin hosting isolation remains separate work.
