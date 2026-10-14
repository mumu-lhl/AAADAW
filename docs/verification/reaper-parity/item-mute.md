# Audio Item mute (in progress)

Linux REAPER 7.82 / Default 7’s Action List catalog records Main action **40175**, `Item properties: Toggle mute`. The existing [native Media Item Properties screenshot](images/reaper-item-properties-volume.png) shows the `Mute` checkbox unchecked; the action catalog is [here](linux-7.82/actions.tsv). These are reference artifacts; this change did not capture a new native or AAADAW screenshot or rerun a native behavior probe.

AAADAW stores mute state on audio items, migrates older projects to schema 44 with mute off, and exposes the checkbox in the existing Audio Item Properties window. The Action List command applies one undoable transaction to selected Audio items. Its live control publishes volume and mute together through one atomic value; the render callback applies zero gain for muted items without adding locks, allocations, or I/O. The shared render graph also keeps muted items silent in offline output. Snapshot restoration, SQLite migration, Undo/Redo, live publication and realtime/offline PCM are covered by regression tests.

The scope remains incomplete. MIDI items are not muted by this action, copying and pasting an Audio item does not currently preserve mute, and mixed-selection behavior has not been compared with REAPER. The AAADAW command’s Item-menu placement, native disabled states, exact properties-window geometry and screenshot alignment also need reference validation. Do not treat the current Audio-only path as full action parity.

Validation: Nightly + Cranelift workspace nextest ran 1057 tests successfully; two CLAP floating-editor tests were excluded because this environment has no X11 display (`X11 connect failed`). Stable workspace all-target Clippy with `-D warnings`, rustfmt and `git diff --check` passed.
