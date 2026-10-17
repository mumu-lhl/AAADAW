# MIDI Editor Humanize Notes

This is an in-progress implementation of REAPER 7.82 action 40457, “Humanize
notes”. It adds the native dialog controls and an undoable note-edit path. The
random distribution is not yet considered parity-complete.

## Native observations

The Linux REAPER 7.82 / Default 7 probe uses a 960-PPQ project with a 1/32
MIDI grid (120 ticks), zero swing, three selected notes, and one unselected
note. The native dialog defaults to Selected notes with Timing, Velocity, and
Timing bias at 0%. The dialog exposes a New random seed button, Timing and
Velocity percentages from 0–100, and Timing bias from −100–100. It has no note
length control.

At Timing 100%, Velocity 100%, and bias 0%, the three selected notes changed
from ticks/velocities `(170, 70)`, `(350, 80)`, `(550, 90)` to
`(131, 68)`, `(288, 96)`, `(510, 92)`. Their durations stayed at 90, 110, and
150 ticks. The unselected note stayed at `(730, 920, 100)`. This sample
establishes that note starts and velocity change while note length and
unselected note data remain fixed; it is one random draw, not an expected
output fixture.

After explicitly clicking New random seed with the same 100% / 100% / 0%
settings, REAPER produced starts/velocities `(170, 41)`, `(381, 63)`, and
`(557, 93)`. The largest observed velocity change in these two seeds is 29
points from a starting velocity of 70. This supports a variation scale near
half the original velocity, but more seeds and low/high velocity edge cases
are still needed to confirm the exact rule.

A separate Timing 100%, Velocity 0%, bias 100% run produced selected starts
`60: 170→537`, `64: 350→264`, and `67: 550→816`; durations and velocities were
unchanged, and the unselected note remained at ticks 730–920. Thus the bias
setting can produce offsets larger than half the 1/32 grid. The native random
stream and the bias distribution have not been characterized across enough
seeds to derive an exact model.

## AAADAW behavior in this slice

The MIDI Editor now exposes Humanize Notes with All notes and Selected notes
scope, Timing, Velocity, Timing bias, New random seed, OK, and Cancel. Defaults
match the observed dialog. The Core edit stores the resolved note changes in
one project-history event, so Undo and Redo replay the same positions and
velocities rather than drawing new random values. Timing moves only starts;
velocity changes remain in the MIDI velocity range; note durations and all
unselected notes are preserved.

AAADAW uses a local seeded generator. The current model uses a maximum velocity
perturbation of half the original velocity at 100%, and expands timing range
with bias using project PPQ; those curves are provisional. The native
distribution, edge behavior, and New random seed sequence are not yet matched.
Treat this row as `in_progress`; it does not count as completed parity coverage.

## Remembered dialog settings

On Linux REAPER 7.82 / Default 7, accepting All notes with Timing 37%, Velocity
61%, and Timing bias −42% wrote `humanall=1`, `humantiming=37`, `humanvel=61`,
and `humanbias=-42` to `reaper.ini`. A new REAPER process using that same
profile reopened the dialog with those values. Changing the scope and values to
Selected notes / 12% / 23% / 0% and clicking Cancel left the last accepted
settings intact.

AAADAW now loads the same four values from a versioned global
`midi-humanize-preferences.json`. It atomically saves only after OK, flushes the
debounced write before application exit, and discards changed dialog values on
Cancel or Escape. Invalid versions and out-of-range saved values are rejected
without replacing the existing file. The native settings readback, UI captures,
and the current AAADAW restart-load screenshot are included in the fixtures.

## Verification artifacts

- `fixtures/midi-humanize-reaper-native-dialog.png`: native default dialog.
- `fixtures/midi-humanize-reaper-native-configured.png`: native 100% Timing,
  100% Velocity, 0% bias dialog.
- `fixtures/midi-humanize-native.tsv`: raw native note readbacks and grid data.
- `fixtures/midi-humanize-probe-output.tsv`: direct output from the checked-in
  ReaScript probe, including the explicitly re-seeded sample.
- `fixtures/midi-humanize-reaper-native-bias100.png`: native 100% Timing,
  0% Velocity, 100% Timing bias dialog during the boundary probe.
- `fixtures/midi-humanize-reaper-native-applied.png`: native MIDI editor after
  the explicitly re-seeded action.
- `fixtures/midi-humanize-settings-native.tsv` and the three
  `fixtures/midi-humanize-settings-native-*.png` captures: native OK, restart,
  and Cancel persistence behavior.
- `fixtures/midi-humanize-before.rpp` and `fixtures/midi-humanize-after.rpp`:
  native project before and after the confirmed dialog action.
- `fixtures/midi-humanize-aaadaw-dialog.png` and
  `fixtures/midi-humanize-aaadaw-configured.png`: live Cranelift desktop
  captures of the new dialog from Build ID
  `9c3ab6f1bcbb98ccaa375fda51ad4aa5fbc5880f`.
- `fixtures/midi-humanize-aaadaw-applied.png`: live editor after the All notes
  action, captured from the same current build.
- `fixtures/midi-humanize-aaadaw-settings-reopened.png`: current desktop build
  reopening with the stored All notes / 37% / 61% / −42% settings.
- `scripts/reaper_parity/probe-midi-humanize-notes.lua`: creates the native
  fixture and waits for a manual dialog confirmation before recording output.

Core tests cover deterministic replay, selected-note isolation, timing and
velocity bounds, unchanged durations, invalid settings, and Undo/Redo. App
tests cover dialog defaults, selected/all scope, seed changes, apply, and
Undo/Redo. Preference tests cover persisted values, rejected invalid files,
OK-only updates, reopening the dialog, and Cancel preserving the prior values.
Random output is deliberately checked by invariants rather than exact values.

## Evidence hashes

The first three AAADAW screenshots were captured from Cranelift build
`9c3ab6f1bcbb98ccaa375fda51ad4aa5fbc5880f` in the live Xorg dummy desktop
(`:289`):

| Artifact | SHA-256 |
| --- | --- |
| `fixtures/midi-humanize-aaadaw-dialog.png` | `2661100f5d06d9060de15b417e664eeacf25faa621ef5e5b5ac3e3d2c10e7cb9` |
| `fixtures/midi-humanize-aaadaw-configured.png` | `7c00115b9d54d76f76c9f846a901d2649e9c6670e42f999e12ec4402dfa34bca` |
| `fixtures/midi-humanize-aaadaw-applied.png` | `665fef4335a75c95d61567c12d60461fac9162f67e9022735700ed9111f76ae1` |

The new persistence screenshots were captured from Cranelift build
`d8ab03b30c7e342bbe69412b51f092b31cf42ccd`:

| Artifact | SHA-256 |
| --- | --- |
| `fixtures/midi-humanize-settings-native-configured.png` | `7f9218af2f36bc00da6cf612492b12f047538a3be07c8513400efbcc78e27cc0` |
| `fixtures/midi-humanize-settings-native-reopened.png` | `b84faaa07fe041b268aa6af29da64e87467d34ad18fb13584ac30c144365860d` |
| `fixtures/midi-humanize-settings-native-cancelled.png` | `2917cbdb846d3c6d166e295596f37871869187ba7a5176754ae56ff4cc7c7520` |
| `fixtures/midi-humanize-aaadaw-settings-reopened.png` | `07ef32b4ca4eed80afd55272dfc4f9aa0d1be40118b3e802beeb9d004668e585` |

The settings TSV SHA-256 is
`72c164efd091103c373aa72bde6b39bc36db296ff2c1bd46af5fe29222ea622c`.
The persistence-check desktop executable SHA-256 is
`71f667150d23eec0363788308b72aa38c74bd80cb30e63c206bd06c1274f323c`.
The original action-slice executable SHA-256 was
`64b9aadc2e7ea42bf9f133c19072f9c649ab85023dc5d9f7bb5fe70ef70d0f4d`.
The two RPP fixtures use LF line endings in the repository; only line endings
were normalized from REAPER's saved files.
