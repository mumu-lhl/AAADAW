# Offline WAV rendering

The File menu's **Render project to WAV…** command renders the saved project's full arrangement
from sample zero through the end of its placed audio and MIDI items. It appends a fixed two-second
tail for track and Master effects. The initial renderer exports interleaved stereo, integer PCM24
WAV at the project sample rate.

The render uses the same fixed audio graph as playback, including track mute/solo, track gain and
pan, enabled CLAP instruments and effects, and the configured Master output ceiling. Rendering
runs on a background worker, feeds media through bounded queues, writes output incrementally, and
does not change project history or the active playback graph. The UI reports frame progress and
can cancel a render.

The `Selected media items` source builds an in-memory project snapshot containing only the chosen
audio item, while keeping its original timeline position and its track, routing, and mix state. It
then uses the regular render graph, so modeled item/take volume and take pan, fades, track processing, and Master
processing follow the same path as other renders. Keeping the original sample position also keeps
track automation aligned to the project clock. Other media items and MIDI parts are removed from
the render snapshot, preventing them from leaking into the file. Output bounds are the selected
item's start and end, with no additional tail. The saved project and playback graph are unchanged.

The Linux REAPER 7.82 probe confirmed that selected-item output includes item gain and track-fader
gain: with both set to 0.5, a source at `[0.125, 0.375]` rendered as `[0.03125, 0.09375]`. Native
Item Properties inspection separately confirmed that its Volume/pan row edits the active take; the
take-volume render coefficient and native pan law still need waveform comparison.
Master-fader behavior and native item/take FX, automation, folder, and send behavior still need
direct comparison; the parity record tracks those gaps separately.

Each finite floating-point output sample is clamped to [-1, 1], multiplied by 2^23, rounded to the
nearest integer, then saturated to the signed 24-bit range and written little-endian. The renderer
does not normalize or dither. A temporary file is used until rendering and media workers finish;
only then is the result published. Cancellation, decode/render errors, and destination collisions
remove the temporary output and do not replace an existing file.

RIFF/WAVE limits an individual output file to less than 4 GiB. Larger projects will fail with a
clear size error; segmented WAV export is a separate future capability.
