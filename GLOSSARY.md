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
Source media associated with a project by an opaque reference. It may be embedded or linked to an external file.
_Avoid_: Audio Item; an asset is source content, while an item is its timeline placement.

**Embedded Audio Asset**:
An immutable snapshot of source-media bytes stored in the project; edits to the original file do not alter it. Refreshing creates a new asset rather than mutating this one.
_Avoid_: Live link.

**External Audio Link**:
A project reference to a file outside the project; playback follows that file's current contents, and moving or removing it may break the reference.
_Avoid_: Embedded asset.

**Media Reference**:
An opaque identifier that storage resolves to an embedded asset or external audio link; the project core stores and validates only the reference string, not the source's existence or contents.
_Avoid_: File path, unless it is specifically a filesystem path.

**Project Sample**:
A frame position on the project's audio clock, independent of tempo changes.
_Avoid_: Tick, when referring to audio arrangement time.
