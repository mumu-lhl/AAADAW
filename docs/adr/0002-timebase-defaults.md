# Initial Timebase Defaults

**Status: accepted.** New projects start at 48 kHz, 960 PPQ, 120 BPM, and 4/4. Tempo uses a piecewise-constant map with cached sample anchors; conversions round to the nearest integer sample or tick. Time signatures change only at bar lines, and both tempo and meter changes are atomic `DawAction`s with undo/redo. Tempo ramps and higher-precision long-range conversion remain follow-up work.
