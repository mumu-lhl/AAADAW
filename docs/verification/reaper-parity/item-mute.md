# Media Item mute (in progress)

Linux REAPER 7.82 / Default 7’s Action List catalog records Main action **40175**, `Item properties: Toggle mute`. The existing [native Media Item Properties screenshot](images/reaper-item-properties-volume.png) shows the `Mute` checkbox unchecked; the action catalog is [here](linux-7.82/actions.tsv). These are reference artifacts; this work did not capture a new native or AAADAW screenshot or rerun a native behavior probe.

AAADAW stores mute state on both Audio and MIDI items. Schema 44 adds Audio mute and schema 45 adds MIDI mute; older projects migrate both states to off. Action 40175 now applies one undoable transaction to Audio, MIDI or mixed selections. The existing Item Properties checkbox covers Audio items. MIDI Item Properties is still unsupported.

For Audio, the realtime control publishes volume and mute together through one atomic value; the render callback applies zero gain without adding locks, allocations or I/O. Offline rendering uses the same mute state. MIDI mute filters that item’s events out of the compiled realtime and offline event plans. The command marks the playback graph dirty, and MIDI-containing selections are disabled while playback is active so an existing plan cannot continue emitting the muted item. Snapshot restoration, SQLite migration, Undo/Redo, duplicate-state preservation and realtime/offline scheduling have regression coverage.

Parity is still incomplete. Audio loop copy/paste does not preserve mute; MIDI Properties UI, native mixed-selection/runtime semantics, exact menu placement and enabled states, complete property-window geometry and AAADAW screenshot alignment remain open.

Validation: Nightly + Cranelift workspace nextest ran 1063 tests successfully; two CLAP floating-editor tests were skipped because this environment has no X11 display (`X11 connect failed`). Stable workspace all-target Clippy with `-D warnings`, rustfmt and `git diff --check` passed.
