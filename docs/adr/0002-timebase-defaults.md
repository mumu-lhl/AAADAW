# Initial Timebase Defaults

**Status: accepted.** New projects start at 48 kHz, 960 PPQ, and 120 BPM. The first tempo implementation is a piecewise-constant tempo map with cached sample anchors; conversions round to the nearest integer sample or tick, and tempo changes are ordinary atomic `DawAction`s, so undo/redo uses the same event history. Meter maps, tempo ramps, and higher-precision long-range conversion remain follow-up work rather than being implied by this initial model.
