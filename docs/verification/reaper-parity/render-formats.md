# Linux REAPER 7.82 Render WAV formats

Reference: Linux REAPER `7.82/linux-x86_64`, Default 7 theme, isolated factory configuration. The [probe script](../../../scripts/reaper_parity/probe-render-formats-ui.lua) opens the native Render to File dialog. Its standard source is a 48 kHz stereo PCM24 file with one second of constant L=0.125/R=0.375 on each of two tracks; the Master mix renders L=0.25/R=0.75. The [WAV bit-depth menu](images/render-wav-bit-depth-native.png) lists 8/16/24/32-bit PCM, 32/64-bit floating point, 4-bit IMA ADPCM, 2-bit cADPCM, and 8-bit u-Law. The [u-Law dialog](images/render-wav-ulaw-native.png) records the selected encoding. For codec vectors, [the fixture generator](../../../scripts/reaper_parity/probe-render-ulaw-vectors.py) creates sixteen 3,000-frame level blocks in a one-second stereo source.

Three native renders verify the menu's unsigned/integer/float data paths:

| Native format | Header and duration | First stereo frame |
| --- | --- | --- |
| 8-bit PCM | RIFF/WAVE PCM, 2 channels, 48 kHz, 48,000 frames | unsigned bytes `[160, 224]`, representing `[0.25, 0.75]` |
| 32-bit PCM | RIFF/WAVE PCM, 2 channels, 48 kHz, 48,000 frames | signed integers `[536870912, 1610612736]`, representing `[0.25, 0.75]` |
| 64-bit FP | RIFF/WAVE IEEE float, 2 channels, 48 kHz, 48,000 frames | `[0.25, 0.75]` |
| 8-bit u-Law | RIFF/WAVE format tag 7, 2 channels, 48 kHz, 48,000 frames | constant `[0.25, 0.75]` encodes as `[159, 135]`; level vectors encode as `[0, 7, 15, 31, 47, 53, 68, 99, 255, 227, 196, 181, 175, 159, 143, 135]` |

The captured output hashes are `7380e49bbd3fb54fe60401cb8e922445db2462db0bd53b99aba3e883e9032fef` (PCM8), `9530b0af314c74d1066657db4a32428f882b7919f58a83420cb829ae6840eb1e` (PCM32), `a86c8fae1a643d6e22101c26c869490f4337dd5f373205d196c8cf1e63e67e56` (Float64), `513dab654074300353a63dd27af5f571becd0a265d18c922fd8a92d70fb2e110` (constant-source u-Law), and `c33fb05ece5fbad748b6b1f6a7e96b40eee0c4ce536b14ec85c203ee85c7943e` (segmented u-Law vector source). All outputs used the same two-track Master mix path.

AAADAW's Render format picker now exports six uncompressed linear formats—PCM 8/16/24/32-bit and IEEE float 32/64-bit—plus 8-bit G.711 u-Law. The queue freezes the selected format with the other render settings. Linear integer PCM uses signed little-endian samples except for the required unsigned 8-bit representation; optional TPDF dither is scoped to these linear PCM formats. u-Law uses the native WAVE format tag 7, companding table, and stereo/mono channel mapping; odd-length mono data receives the required RIFF pad byte. IEEE float formats preserve samples above full scale and ignore integer dither. Float64 stores the f32 samples produced by the current render graph after promotion to f64, so the container is 64-bit IEEE float but the engine's processing precision remains f32. Writers stream frames to private sibling files and publish only after completion; cancellation removes partial files, and publication never replaces a destination that appeared during rendering.

Unit tests verify bit depth and format tags, unsigned PCM8 values, PCM32 values, float64 headroom, G.711 u-Law vectors and decoding, Mono/Stereo channel mapping, dither, RIFF limits, cancellation, and no-clobber publication for every offered format. An offline graph integration test renders all seven formats from the same project and decodes each result.

The native 4-bit IMA ADPCM and 2-bit cADPCM encodings remain unsupported. Native dither behavior for u-Law, noise-shaping modes, exact Render UI state persistence, large-file RF64 behavior, and full format-menu equivalence remain open; this format slice does not close `RENDER-FORMATS-001` or `UI-028`.
