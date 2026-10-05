# Multiple FX automation lanes in Arrangement

## User scenario

A producer is balancing several effect parameters over the same section and wants to compare and edit their automation without repeatedly hiding one lane to show another.

## Acceptance

- Showing or hiding one parameter leaves other visible lanes intact.
- Each FX lane has its own label, compact graph band, and point hit area.
- Track row height, TCP, timeline, item labels, scrolling, and hit testing share the same expanded geometry.
- FX lanes remain attached to the matching plugin through chain reorder/removal; volume automation remains available.
- Editing continues to use sample-clock points, snapping, selection, undo, and redo.

## Completion

Arrangement now assigns each visible `(track, chain slot, parameter)` lane a stable sorted band below its track row. Track row geometry expands with visible lanes and is shared by the TCP, shader renderer, item labels, scrolling, drag targets, and pointer hit tests. Showing/hiding a lane only changes that lane; chain reconciliation remaps all lanes to their plugin instances and removes lanes when the plugin is removed. FX point hit, add, drag, and delete behavior uses the lane's own value range and stepped metadata.

Verified with `cargo check -p aaadaw --no-default-features`, `cargo check -p aaadaw --features audio-device`, strict app Clippy, all 18 timeline tests, the app FX automation undo/redo test, and `git diff --check`.

Native visual review at narrow/wide window sizes and with GUI zoom/scroll could not be run in this environment: neither `Xvfb` nor `xvfb-run` is installed. The row mapping and interaction boundaries are covered by targeted tests; the GUI rendering itself remains unverified here.
