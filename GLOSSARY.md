# AAADAW Project Model

Terms for arranging and referencing audio and MIDI content in an AAADAW project.

## Timeline Content

**Audio Item**:
A placement of a media source on a track, anchored to the project's absolute sample clock. Its source offset is measured in source sample frames, independently of the project's timeline duration.
_Avoid_: Audio clip, when distinguishing the placement from the media content itself.

**MIDI Item**:
A track placement containing MIDI notes, positioned and sized in PPQ ticks so it follows the musical timeline.
_Avoid_: MIDI asset.

**Media Reference**:
An opaque identifier used by the media layer to resolve source content; the project core stores the reference but does not resolve it or guarantee that the source exists.
_Avoid_: File path, unless it is specifically a filesystem path.

**Project Sample**:
A frame position on the project's audio clock, independent of tempo changes.
_Avoid_: Tick, when referring to audio arrangement time.
