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

### Expanded timing samples

Two exact Timing 100%, Velocity 0%, bias 0% captures use 960 PPQ, 120 BPM, a
1/32 MIDI grid, and a 1/32 project grid. Their configured dialogs are included
with the raw event readbacks: one sample has 64 selected notes, and the other
has 512 selected notes plus one unselected note. Across the 576 selected-note
offsets, the mean is −1.35 ticks, population standard deviation is 122.31
ticks, the range is −373 to +400, skew is −0.066, and excess kurtosis is
−0.101. This supports a symmetric distribution with a scale near PPQ/8 (120
ticks) at 100% Timing; it does not establish the exact native random sequence.
The 512-note capture also confirms unchanged duration, pitch, channel,
velocity, selection, and unselected-note data.

The bias samples do not support one simple signed offset model. A 64-note
capture at +100% had mean +122.8 ticks (SD 144.4); its −99% counterpart had
mean −131.4 ticks (SD 127.1). Four separately captured 512-note endpoint runs
at 120 BPM used screenshots and accepted `reaper.ini` settings to verify the
displayed bias and zero Velocity: the two +100% runs had means −126.8 and
+121.3 ticks, while the two −100% runs had means −5.9 and +111.3 ticks. Their
individual raw before/after event rows are retained below. The materially
different outcomes prevent treating the current linear bias curve as native
parity; repeatable seed control and bias semantics remain open.

## AAADAW behavior in this slice

The MIDI Editor now exposes Humanize Notes with All notes and Selected notes
scope, Timing, Velocity, Timing bias, New random seed, OK, and Cancel. Defaults
match the observed dialog. The Core edit stores the resolved note changes in
one project-history event, so Undo and Redo replay the same positions and
velocities rather than drawing new random values. Timing moves only starts;
velocity changes remain in the MIDI velocity range; note durations and all
unselected notes are preserved.

AAADAW uses a local seeded generator. At zero bias, its current timing model is
a normal draw with standard deviation `project PPQ / 8` at 100% Timing, scaled
linearly by the Timing percentage and clamped at four standard deviations.
The 576 verified native zero-bias offsets support that scale, but do not reveal
REAPER's exact generator. AAADAW currently applies bias as a signed shift of
the same standard deviation; the captured native endpoint runs do not
consistently support that rule. Velocity still uses a local uniform draw with
a maximum perturbation of half the original velocity at 100%. Exact native
bias semantics, velocity distribution and boundaries, and New random seed
behavior remain open. Treat this row as `in_progress`; it does not count as
completed parity coverage.

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
- `fixtures/midi-humanize-aaadaw-cranelift-current-startup.png`,
  `fixtures/midi-humanize-aaadaw-cranelift-current-dialog.png`,
  `fixtures/midi-humanize-aaadaw-cranelift-current-configured.png`, and
  `fixtures/midi-humanize-aaadaw-cranelift-current-applied.png`: screenshots
  from the rebuilt current binary, Build ID
  `9efbe45b29a0ff959422bd60affe6e86c084bf2f`. The live Xorg :290 run created
  a MIDI item and note, opened the actual Humanize dialog, applied 100% Timing
  with Velocity and bias at 0%, then captured the editor.
- `fixtures/midi-humanize-aaadaw-settings-reopened.png`: current desktop build
  reopening with the stored All notes / 37% / 61% / −42% settings.
- `fixtures/midi-humanize-rng-timing-bias0-64-native.tsv` and
  `fixtures/midi-humanize-rng-timing-bias0-64-dialog.png`: the verified
  64-note, 100% Timing / 0% Velocity / 0% bias capture.
- `fixtures/midi-humanize-rng-timing512-bias0-native.tsv`,
  `fixtures/midi-humanize-rng-timing512-settings-native.tsv`, and
  `fixtures/midi-humanize-rng-timing512-dialog.png`: the 512-note raw
  readback, accepted REAPER setting values, and configured native dialog.
- `fixtures/midi-humanize-rng-timing512-applied.png`: the native MIDI Editor
  after the 512-note zero-bias action.
- `fixtures/midi-humanize-rng-timing-biasplus100-native.tsv`,
  `fixtures/midi-humanize-rng-timing-biasminus99-native.tsv`, and their
  `fixtures/midi-humanize-rng-biasplus100-dialog.png` and
  `fixtures/midi-humanize-rng-biasminus99-dialog.png` captures: the earlier
  64-note bias samples. The minus sample was −99%, as shown in its dialog,
  despite the original probe filename.
- `fixtures/midi-humanize-rng-timing512-biasplus100-*.tsv`,
  `fixtures/midi-humanize-rng-timing512-biasminus100-*.tsv`, and corresponding
  `*-dialog.png` captures: four exact 512-note bias endpoint runs, including
  the inconsistent outcomes described above.
- `scripts/reaper_parity/probe-midi-humanize-notes.lua`: creates the native
  fixture and waits for a manual dialog confirmation before recording output.
- `scripts/reaper_parity/probe-midi-humanize-distribution.lua`: creates
  configurable fresh-project samples and records raw note readbacks, project
  tempo, PPQ, and MIDI/project grid values.

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

The expanded native timing fixtures were captured in Linux REAPER
`7.82/linux-x86_64` with the Default 7 theme on Xorg dummy display `:289`.
Their SHA-256 hashes are:

| Artifact | SHA-256 |
| --- | --- |
| `fixtures/midi-humanize-rng-timing-bias0-64-dialog.png` | `214feebc71fc97954862ed985e5bd4de943e05260de847e7334caa9cc281bd7d` |
| `fixtures/midi-humanize-rng-timing-bias0-64-native.tsv` | `e646bade5e3dd0ae65ca0db2bfdbb3a113706491535780ce496bfcc3fd40e084` |
| `fixtures/midi-humanize-rng-timing512-dialog.png` | `3a52a21bd97f87f2507422eb4eb2e9047d4fbf780beb52c182c3ee1e3c34904d` |
| `fixtures/midi-humanize-rng-timing512-applied.png` | `db32072d27893709872e4ffeeb44a7791cfe992e6d6b638f0b4abf99bea078e0` |
| `fixtures/midi-humanize-rng-timing512-bias0-native.tsv` | `80037a62070a089101526b19ada8e979dc712c427939ac7e400fdc182aad04a3` |
| `fixtures/midi-humanize-rng-timing512-settings-native.tsv` | `2ef0846b54800e63a055d35210b12247d07bcb495eee94484dcad2fcd1898c95` |
| `fixtures/midi-humanize-rng-timing-biasplus100-dialog.png` | `51b3ba5c73341f4fb95f05db2838c02a67fa6f46aeb52591ea330a1da2bbe718` |
| `fixtures/midi-humanize-rng-timing-biasplus100-native.tsv` | `4f0ef27473efcbef551b9df02b15fa38616424f890c7eea5fa7a0bae622f8111` |
| `fixtures/midi-humanize-rng-timing-biasminus99-dialog.png` | `e3609f1c7ca7fa5ea23b13587aac06463436e5836cf2f18b9f407dc30224d598` |
| `fixtures/midi-humanize-rng-timing-biasminus99-native.tsv` | `c12d72d51661fd590e2c5c71f50997e4519ca1a4ddae3890a9353e915d83efed` |
| `fixtures/midi-humanize-rng-timing512-bias-settings-native.tsv` | `b9548c423190e61743b31bc38a6fac491c40c361e820ab22e8cc31d935fbb8aa` |
| `fixtures/midi-humanize-rng-timing512-biasplus100-first-dialog.png` | `744bd530f39debb984cb4beeededb87a9b13354ca5197679df3f43f752cc66b7` |
| `fixtures/midi-humanize-rng-timing512-biasplus100-first-native.tsv` | `8b4181cc6fe2f8e1813ddfb2de379a15b1d060abbf9b353699add549c5086907` |
| `fixtures/midi-humanize-rng-timing512-biasplus100-repeat2-dialog.png` | `01ae52c9c35fd3752731f143986f2e0fdd7ec8dc3129eee2a2791846d6a13189` |
| `fixtures/midi-humanize-rng-timing512-biasplus100-repeat2-native.tsv` | `f76c32b2ce502faff3222267b808e97c1a81b6779bb9efeb198acfcee68aa572` |
| `fixtures/midi-humanize-rng-timing512-biasminus100-first-dialog.png` | `a87862dc525fcc085df05d2a580d664c9ebac693da0e1eda01cd56c6eea6378b` |
| `fixtures/midi-humanize-rng-timing512-biasminus100-first-native.tsv` | `c5e06f0817776029f7809b0a9de518de72880b995934d434b1fd399594eb94d5` |
| `fixtures/midi-humanize-rng-timing512-biasminus100-repeat2-dialog.png` | `635146f135b1a81e6021b6a5ccded9e0476c4ce33ea89f491d86e3e11a20f37b` |
| `fixtures/midi-humanize-rng-timing512-biasminus100-repeat2-native.tsv` | `dbdba8e4c80d0e233f9a3075f234d8aa21230a3a5d47945da6c13bf7bb4aaabf` |

The current AAADAW screenshot run used Cranelift Build ID
`9efbe45b29a0ff959422bd60affe6e86c084bf2f`, executable SHA-256
`1aa3f584b7916b1f52181c8f0cf39debe878b5ec2c04b264ce6edc9b48075fe8`:

| Artifact | SHA-256 |
| --- | --- |
| `fixtures/midi-humanize-aaadaw-cranelift-current-startup.png` | `5983232a7cbe2cccfc9b20feb1d36f805f879d847cf558bb74e838e2d255ad1d` |
| `fixtures/midi-humanize-aaadaw-cranelift-current-dialog.png` | `96d239a7246f46f49ea00bedb4004822d00373ae1518313e9c31d3637e8e8a46` |
| `fixtures/midi-humanize-aaadaw-cranelift-current-configured.png` | `8b63f722a7b8dbeb67ade9026c5d7f531b284b9f6e96b074c12a686f449290b4` |
| `fixtures/midi-humanize-aaadaw-cranelift-current-applied.png` | `cebdb2b79565aa0255b0d5b033e699b71b91cebcec5088ee46eee313f13545cd` |
