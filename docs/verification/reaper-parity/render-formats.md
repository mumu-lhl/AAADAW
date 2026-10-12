# Linux REAPER 7.82 Render WAV formats

Reference: Linux REAPER `7.82/linux-x86_64`, Default 7 theme, isolated factory configuration. The [probe script](../../../scripts/reaper_parity/probe-render-formats-ui.lua) opens the native Render to File dialog for a controlled 48 kHz stereo source: one second of constant L=0.125/R=0.375 on each of two tracks. The Master mix therefore renders L=0.25/R=0.75. The native [WAV bit-depth menu](images/render-wav-bit-depth-native.png) lists 8/16/24/32-bit PCM, 32/64-bit floating point, 4-bit IMA ADPCM, 2-bit cADPCM, and 8-bit u-Law.

Three native renders verify the menu's unsigned/integer/float data paths:

| Native format | Header and duration | First stereo frame |
| --- | --- | --- |
| 8-bit PCM | RIFF/WAVE PCM, 2 channels, 48 kHz, 48,000 frames | unsigned bytes `[160, 224]`, representing `[0.25, 0.75]` |
| 32-bit PCM | RIFF/WAVE PCM, 2 channels, 48 kHz, 48,000 frames | signed integers `[536870912, 1610612736]`, representing `[0.25, 0.75]` |
| 64-bit FP | RIFF/WAVE IEEE float, 2 channels, 48 kHz, 48,000 frames | `[0.25, 0.75]` |

The three captured output hashes are `7380e49bbd3fb54fe60401cb8e922445db2462db0bd53b99aba3e883e9032fef` (PCM8), `9530b0af314c74d1066657db4a32428f882b7919f58a83420cb829ae6840eb1e` (PCM32), and `a86c8fae1a643d6e22101c26c869490f4337dd5f373205d196c8cf1e63e67e56` (Float64). Each used the same two-track fixture and Master mix.

AAADAW's Render format picker now exports all six uncompressed linear formats: PCM 8/16/24/32-bit and IEEE float 32/64-bit. The queue freezes the selected format with the other render settings. Integer PCM uses signed little-endian samples except for the required unsigned 8-bit representation; optional TPDF dither remains available only for integer formats. IEEE float formats preserve samples above full scale and ignore integer dither. Float64 stores the f32 samples produced by the current render graph after promotion to f64, so the container is 64-bit IEEE float but the engine's processing precision remains f32. Writers stream frames to private sibling files and publish only after completion; cancellation removes partial files, and publication never replaces a destination that appeared during rendering.

Unit tests verify bit depth and format tags, unsigned PCM8 values, PCM32 values, float64 headroom and decoding, Mono/Stereo channel mapping, dither, RIFF limits, cancellation, and no-clobber publication for every offered format. An offline graph integration test renders all six formats from the same project and decodes each result.

The compressed native WAV encodings (IMA ADPCM, cADPCM, and u-Law) remain unsupported. Dither/noise-shaping details, exact native Render UI layout and state persistence, large-file RF64 behavior, and full format-menu equivalence remain open; this format slice does not close `RENDER-FORMATS-001` or `UI-028`.
