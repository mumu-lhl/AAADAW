# Live track volume and pan controls

## User scenario

A musician balances tracks and places them in the stereo field while listening to the arrangement.

## Current behavior

The Arrangement TCP changes volume by 1 dB and pan by 10% for each button press. Every press
creates a separate project action. The current playback graph compiles track coefficients at
prepare time and does not receive later volume or pan edits, so a musician cannot audition a mix
change without rebuilding playback.

## Expected behavior

- Use compact continuous controls with readable dB and pan values.
- Allow precise numeric entry and an obvious reset to 0 dB / center.
- Support Shift fine adjustment and double-click reset on both sliders.
- Apply each completed drag or numeric edit through one `DawAction`, making one gesture one undo
  step.
- Treat a double-click reset as one undo step, including coalescing the click that precedes the
  second press.
- Make a value audible in active playback within the next block without allocation, locks, or I/O
  on the audio callback.
- Preserve track ordering, mute/solo, project serialization, and safe render-graph replacement.

## Design constraints

- Keep the seam between the control-thread mixer update and audio callback small and realtime-safe.
- Calculate gain/pan coefficients off the audio thread; the callback should consume precomputed
  values through atomics or an already bounded command path.
- Do not rebuild/decode media or recompile plugin processors for a fader change.
- A canceled or unchanged gesture must not create an undo record.
- Live audition updates immediately; only the project-history commit waits through the platform's
  double-click interval so a reset stays one undo step.

## Validation

- Engine tests prove a running render graph uses updated volume/pan values on its next block without
  changing transport or stream position.
- Application tests prove drag previews do not write project history until release, and commit to
  one undoable action; numeric entry also commits once.
- Review TCP density and narrow widths through the actual Iced view when a display is available.
- Run the focused engine/app tests, formatting, and Clippy. CI checks Windows and Linux builds.
