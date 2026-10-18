# MIDI file import

AAADAW has an end-to-end Standard MIDI File import path at `Insert > MIDI file…`. It reads and decodes the file off the UI thread, then creates project tracks/items in one undoable transaction. The imported items are placed at the edit cursor.

The decoder accepts PPQ-timed SMF format 0 and format 1. It validates chunk bounds, VLQs, status/data bytes, declared track counts, and End of Track placement. It expands running status and retains event order, SysEx payloads, and Meta payloads in `MidiFileImport`. Input is limited to 64 MiB, 1,024 tracks, and 1,000,000 events. SMPTE divisions and format 2 are rejected with an error.

Project conversion scales event positions from file PPQ to project PPQ using nearest-tick rounding. It maps Note On/Off, CC, Pitch Bend, Program Change, Poly Aftertouch, and Channel Pressure to the editable MIDI model. Repeated notes with the same channel and pitch pair FIFO. A Note Off without a Note On, an unmatched Note On, or a note whose converted duration is zero rejects the entire import. The current desktop path creates tracks for channel-bearing source tracks and reports the SysEx/Meta events it cannot store; meta-only tracks are omitted.

## Linux REAPER 7.82 comparison

The comparison used a deterministic 212-byte Type 1 file at 960 PPQ with three tracks: a conductor track (120 BPM, 4/4, then 150 BPM and 3/4), a lead track (note, text, CC0/CC32, program change, SysEx and cue), and a metadata-only track (text and marker). The edit cursor was at 2 seconds. The fixture generator, Lua probe, binary fixture, raw event dumps and screenshots are in [`fixtures/midi-file-import-native/`](fixtures/midi-file-import-native/).

REAPER presents `MIDI File Import` with these three options unchecked by default:

1. Expand the two MIDI tracks to new REAPER tracks.
2. Import the MIDI tempo map into the project tempo map at the edit cursor.
3. Import MIDI markers/cues as project markers.

With the defaults, REAPER creates one track named `Conductor` with one item at 2 seconds. That item's take contains events from all three SMF tracks, including both track-name events, ordinary text, marker/cue meta events, and SysEx. The project tempo map is unchanged and no project markers are added. The [default import screenshot](fixtures/midi-file-import-native/reaper-7.82-default-import.png) and [raw dump](fixtures/midi-file-import-native/default-import.tsv) record this behavior.

With all three options enabled, REAPER creates two tracks named `Lead` and `Meta only`; the conductor track becomes tempo/meter data instead of a MIDI track. It adds tempo markers at the edit cursor and at source tempo/meter change positions, and imports both marker/cue events as project markers. The MIDI items still retain text, marker/cue and SysEx events. The [expanded import screenshot](fixtures/midi-file-import-native/reaper-7.82-expanded-import.png) shows the Event List rows for the note, text, paired bank-select controllers, program change, SysEx and cue. The [raw dump](fixtures/midi-file-import-native/expanded-import.tsv) records the two tracks, events, tempo map and project markers. The [native options dialog](fixtures/midi-file-import-native/reaper-7.82-import-options-default.png) captures the default unchecked state.

The native probe establishes that SysEx and metadata are editable MIDI-item events even when project-marker or tempo-map import is disabled; they are not discarded. AAADAW does not yet match this behavior: it has no import-options dialog, merges no source tracks, omits metadata-only tracks and does not store SysEx/Meta events, source event order across event classes, conductor tempo/meter, or imported project markers. Key signatures and additional SMF metadata also remain unsupported. These gaps keep `MIDI-FILE-IMPORT-001`, UI-019 and P7 in progress.

Automated verification currently covers format 0/1 parsing, running status, track separation, PPQ scaling, supported event conversion, SysEx/Meta counting, malformed input rejection, note pairing, transaction rollback, Undo/Redo, project snapshot restoration, SQLite save/reopen, and the desktop result-message handler. The [current-build desktop capture](../verification/reaper-parity/fixtures/midi-file-import-aaadaw-action-list.png) confirms the Action List entry only; AAADAW's completed-import screen still needs a live desktop capture after implementation.
