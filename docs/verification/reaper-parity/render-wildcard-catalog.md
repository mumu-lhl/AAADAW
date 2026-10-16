# REAPER Render wildcard catalog

This catalog comes from the actual Linux REAPER 7.82 Render dialog using the Default 7 theme and an isolated configuration. The [top-level Wildcards menu](fixtures/render-wildcards-menu-native.png) and its category menus were captured directly from the application. The [Wildcard help pages](fixtures/render-wildcards-help-1-native.png), [2](fixtures/render-wildcards-help-2-native.png), [3](fixtures/render-wildcards-help-3-native.png), [4](fixtures/render-wildcards-help-4-native.png), and [5](fixtures/render-wildcards-help-5-native.png) record expansion rules and value semantics.

## Native menu entries

The native menu exposes these entries, grouped as follows:

| Category | Tokens |
| --- | --- |
| Project Information | `$project`, `$title`, `$author`, `$projectnotes`, `$projectnotes(name)`, `$projectdirectory`, `$projectdirectoryN`, `$track`, `$trackslashes`, `$tracknumber`, `$tracknamenumber`, `$seltrack`, `$folders`, `$folders[X]`, `$parent`, `$tempo`, `$timesignature`, `$timesignum`, `$timesigdenom`, `$playrate`, `$fx`, `$fx[X]` |
| Regions/Markers | `$region`, `$region{lane}`, `$regionname`, `$region(name)`, `$region(name)[s]`, `$regionnumber`, `$regionnumber[N]`, `$marker`, `$marker{lane}`, `$markername`, `$marker(name)`, `$marker(name)[s]`, `$markernumber`, `$markernumber[N]` |
| Media Items | `$item`, `$itemnumber`, `$itemnotes`, `$itemnotes(name)`, `$takemarker`, `$takemarker(name)`, `$takemarker(name)[s]`, `$lane` |
| Position/Length | `$start`, `$end`, `$length`, `$startbeats`, `$endbeats`, `$lengthbeats`, `$starttc`, `$endtc`, `$startframes`, `$endframes`, `$lengthframes`, `$startseconds`, `$endseconds`, `$lengthseconds`, `$lenhh`, `$lenmm`, `$lenss`, `$lentt` |
| Recording | `$filenumber`, `$filenumber[N]`, `$filecount`, `$note`, `$note[X]`, `$natural`, `$namenumber`, `$timelineorder`, `$timelineorder[N]`, `$timelineorder_track`, `$timelineorder_track[N]` |
| Output Format | `$format`, `$samplerate`, `$sampleratek`, `$bitdepth` |
| Channels | `$channels`, `$chid` |
| Date/Time/Unique ID | `$date`, `$time`, `$datetime`, `$year`, `$year2`, `$month`, `$monthname`, `$day`, `$dayname`, `$hour`, `$hour12`, `$ampm`, `$minute`, `$second`, `$localtime`, `$uniqueid[N]` |
| Computer Information | `$user`, `$computer` |

The category captures are [Project Information](fixtures/render-wildcards-project-info-native.png), [Regions/Markers](fixtures/render-wildcards-regions-native.png), [Media Items](fixtures/render-wildcards-media-native.png), [Position/Length](fixtures/render-wildcards-position-native.png), [Recording](fixtures/render-wildcards-recording-native.png), [Output Format](fixtures/render-wildcards-format-native.png), [Channels](fixtures/render-wildcards-channels-native.png), [Date/Time/Unique ID](fixtures/render-wildcards-date-native.png), and [Computer Information](fixtures/render-wildcards-computer-native.png). The adjacent [master-track substitution](fixtures/render-wildcards-master-track-native.png) and [directory substitution option](fixtures/render-wildcards-options-native.png) are settings rather than wildcard tokens.

## Expansion rules observed in native help

- Capitalizing the first character of a wildcard capitalizes the first character of its substitution. Capitalizing its first two characters uppercases the substitution; a lowercase first character and uppercase second character lowercases it.
- `<X>` removes the character `X` from a substitution. `<abcdeX>` replaces each character in `abcde` with `X`; for example, `$track< >` removes spaces and `$track< _>` replaces spaces with underscores.
- A `[N]` suffix numbers repeated outputs with zero padding, as in `$filenumber[01]` or `$regionnumber[001]`. `[<N]` keeps the first N characters of a value, as in `$region[<4]`.
- If a wildcard expands to an empty value, an adjacent space, hyphen, underscore, or dot is omitted. The Render Options menu can allow `/` in substitutions and create directories; `$folders` and `$trackslashes` override that setting.
- `{lane}` on region or marker tokens limits expansion to objects on that ruler lane. Named values use `name=value` text: `$projectnotes(name)`, `$region(name)`, `$marker(name)`, `$itemnotes(name)`, and `$takemarker(name)` select matching values. A `[s]` suffix joins multiple matches with `s`. REAPER's help illustrates named project notes, marker values, and applying the same rules to item notes.
- The Position/Length help defines clock, beats, timecode, absolute-frame, total-seconds, and split length fields. Recording tokens depend on output-file order, timeline order, item/region name collisions, or the MIDI note sequence, depending on the token.

The captured Output Format previews confirm `wav-44100-44.1-24-2` for 44.1 kHz / 24-bit / stereo, `wav-44100-44.1-24-1` for mono, and `wav-22050-22.05-2-1` for 22.05 kHz / 2-bit cADPCM / mono. Native previews also show `$bitdepth` as `16`, `4`, and `2` for PCM16, IMA ADPCM, and cADPCM respectively. See the [format](fixtures/render-wildcards-format-preview-native.png), [mono](fixtures/render-wildcards-mono-native.png), [22.05 kHz](fixtures/render-wildcards-samplerate-22050-native.png), [PCM16](fixtures/render-wildcards-bitdepth-16-native.png), [IMA ADPCM](fixtures/render-wildcards-bitdepth-ima-native.png), and [cADPCM](fixtures/render-wildcards-bitdepth-cadpcm-native.png) captures.

## AAADAW status

AAADAW implements 42 tokens: `$project`, `$title`, `$author`, `$projectnotes`, `$projectnotes(name)`, `$bpm`, `$tempo`, `$timesignature`, `$timesignum`, `$timesigdenom`, `$track`, `$tracknumber`, `$tracknameornumber`, `$region`, `$regionnumber`, `$marker`, `$markernumber`, `$item`, `$itemnumber`, the 18 Position/Length entries above, and `$format`, `$samplerate`, `$sampleratek`, `$bitdepth`, and `$channels`. `$bpm` and `$tracknameornumber` are accepted compatibility forms outside the captured native menu labels. The native menu groups items hierarchically; AAADAW currently presents its flat grouped picker in a scrollable panel. Its lower rows and a real `$sampleratek` insertion are shown in the [scrolled AAADAW picker](fixtures/render-filename-pattern-wildcards-scrolled-aaadaw.png), [sample-rate insertion](fixtures/render-filename-pattern-wildcard-sampleratek-aaadaw.png), [named project-note insertion](fixtures/render-filename-pattern-wildcard-insert-aaadaw.png), and [item-number insertion](fixtures/render-filename-pattern-wildcard-itemnumber-aaadaw.png).

AAADAW now applies the help-documented capitalization modes, `<X>` character removal, `<abcdeX>` character replacement, `[<N]` prefix truncation, and removal of one adjacent space, hyphen, underscore, or dot when a resolved value is empty. Regression cases cover the help examples. Project-note named lookup `$projectnotes(name)` is now implemented from the captured help example. The rest of the native catalog and syntax remain open: region/marker/item/take-marker named notes and expressions, directory and folder hierarchy, parent and FX names, marker identity and ordering, item notes and take markers, recording counters, date/time and unique IDs, user/computer values, `[N]` render-order counters, `/` directory creation, `{lane}`, presets, `Master $Track` substitution, non-WAV formats, and per-channel `$chid` need implementation or additional source-specific REAPER comparisons. `RENDER-WILDCARDS-001` remains `in_progress`.
