# Multiple FX automation lanes in Arrangement

## User scenario

A producer is balancing several effect parameters over the same section and wants to compare and edit their automation without repeatedly hiding one lane to show another.

## Acceptance

- Showing or hiding one parameter leaves other visible lanes intact.
- Each FX lane has its own label, compact graph band, and point hit area.
- Each lane can be resized from its lower divider, with a bounded compact height.
- Track row height, TCP, timeline, item labels, scrolling, and hit testing share the same expanded geometry.
- FX lanes remain attached to the matching plugin through chain reorder/removal; volume automation remains available.
- Editing continues to use sample-clock points, snapping, selection, undo, and redo.

## Completion

Arrangement now assigns each visible `(track, chain slot, parameter)` lane a stable sorted band below its track row. Track row geometry expands with visible lanes and is shared by the TCP, shader renderer, item labels, scrolling, drag targets, and pointer hit tests. Dragging a lane's lower divider resizes it from 20 to 192 px; lane heights follow their plugin parameters through chain reordering and are discarded when the plugin is removed. Showing/hiding a lane only changes that lane. FX point hit, add, drag, and delete behavior uses the lane's own value range and stepped metadata.

Verified with `cargo check -p aaadaw --no-default-features`, `cargo check -p aaadaw --features audio-device`, strict app Clippy, the lane geometry/resize tests, the app FX automation undo/redo test, formatting, and `git diff --check`.

Visual review used the installed Xorg dummy display with a temporary three-lane project: the arrangement was rendered at 1280×800 and 960×680, a divider drag enlarged its lane and track row while keeping lane labels aligned, the zoom control changed timeline scale, and middle-drag panned the timeline. The TCP remains aligned with the expanded row. A real plugin is unavailable in this environment, so plugin-backed playback was not visually exercised; lane hit-testing and bounded resize state are covered by targeted tests.
