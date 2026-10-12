# Linux REAPER 7.82 Render channels

Reference: Linux REAPER `7.82/linux-x86_64`, Default 7 theme, fresh isolated configuration, empty unsaved project. The probe used the Master mix source, custom 0–1 second bounds, 48 kHz output, no tail, and no render normalization. It created one centered track from a 48 kHz, 24-bit stereo WAV whose 48,000 frames contain constant L=0.125 and R=0.375.

The [probe script](../../../scripts/reaper_parity/probe-render-channels.lua) rendered the same Master mix once with two channels and once with one channel. Both output files contained 48,000 frames:

| Requested channels | WAV header | First output frame |
| ---: | --- | --- |
| 2 | stereo, 24-bit, 48 kHz | L=0.125, R=0.375 |
| 1 | mono, 24-bit, 48 kHz | 0.25 |

For this centered Master mix, REAPER's mono output is the average of its left and right mix channels. The official [REAPER 7.81 User Guide, section 21.3](https://www.reaper.fm/userguide/ReaperUserGuide781c.pdf) describes Channels as mono, stereo, or a selected multichannel count. The 7.82 probe confirms the mono average for the controlled case above; it does not establish behavior for non-center pan laws, stems, or multichannel track mappings.

AAADAW now exposes Mono and Stereo for project WAV renders. Mono averages the two rendered Master channels after optional sample-rate conversion; all three supported WAV encodings write the matching one-channel header and frame data. Focused regression coverage checks the mono average in PCM16, PCM24, and Float32, plus the project-render range integration. Source modes and sample-peak postprocessing are partially implemented in [Render Source](render-source.md) and [Render Normalize](render-normalize.md). Multichannel output and complete native dialog parity remain open; `UI-028` and `RENDER-CHANNELS-001` stay `in_progress`.
