# AAADAW Project Model

Terms for arranging and referencing audio and MIDI content in an AAADAW project.

## Timeline Content

**Audio Item**:
A placement of a media source on a track, anchored to the project's absolute sample clock. Its source offset is measured in source sample frames, independently of the project's timeline duration.
_Avoid_: Audio clip, when distinguishing the placement from the media content itself.

**MIDI Item**:
A track placement containing MIDI notes, positioned and sized in PPQ ticks so it follows the musical timeline.
_Avoid_: MIDI asset.

**Audio Asset**:
Immutable source-media bytes embedded in a project and stored as bounded SQLite BLOB chunks; a seekable reader exposes the content to background decoders.
_Avoid_: Audio Item; an asset is source content, while an item is its timeline placement.

**Media Reference**:
An opaque identifier that the storage adapter resolves to an Audio Asset; the project core stores and validates only the reference string, not asset existence or contents.
_Avoid_: File path, unless it is specifically a filesystem path.

**Project Sample**:
A frame position on the project's audio clock, independent of tempo changes.
_Avoid_: Tick, when referring to audio arrangement time.
