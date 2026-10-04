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

Each finite floating-point output sample is clamped to [-1, 1], multiplied by 2^23, rounded to the
nearest integer, then saturated to the signed 24-bit range and written little-endian. The renderer
does not normalize or dither. A temporary file is used until rendering and media workers finish;
only then is the result published. Cancellation, decode/render errors, and destination collisions
remove the temporary output and do not replace an existing file.

RIFF/WAVE limits an individual output file to less than 4 GiB. Larger projects will fail with a
clear size error; segmented WAV export is a separate future capability.
