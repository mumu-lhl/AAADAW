# Desktop Runtime and Dependency Policy

**Status: accepted.** The desktop MVP targets Linux and Windows. Linux audio is
provided by optional JACK and PipeWire backends; Windows audio is provided by
the optional WASAPI backend. Building the portable model and UI does not require
an audio device. macOS and Android remain Phase 2 targets and are not implied by
the current cross-platform core.

The desktop shell uses Iced 0.14 with its `wgpu` renderer. The application keeps
Iced's retained widget tree for controls and uses `iced::widget::shader` for the
large, clipped Arrangement canvas. Upgrade Iced and its renderer as a compatible
pair after reviewing upstream release notes and running the Linux and Windows
build matrix; do not pin a second, independently selected `wgpu` version in the
application.

Audio callbacks are bounded render workers, not application service threads.
The callback may read bounded SPSC queues and atomics, render a preallocated
graph, copy/encode the supplied output buffer, and publish bounded status. It
must not allocate, block, perform filesystem or network I/O, log, or destroy
plugin/render-graph owners. Graph construction, plugin lifecycle, media reads,
recording file writes, error formatting, and retired graph reclamation remain on
control or background threads. A change that moves work across this boundary
requires an explicit real-time safety review and regression coverage (see
[`realtime-rendering.md`](../design/realtime-rendering.md)).

CLAP is the first supported plugin format and is hosted in process. Projects
remain open when a saved plugin is unavailable or fails to restore. Plugin
isolation is a later Phase 2 capability; until then, the UI warns before
in-process execution. VST3 is not enabled by implication: its SDK, bridge, and
redistribution conditions require a separate review before implementation or
distribution.

All resolved Rust dependencies, including optional platform features and
development dependencies, are checked from `Cargo.lock` with `cargo-deny`.
The checked-in `deny.toml` is the executable policy; CI checks every feature
and Linux, Windows, macOS, and Android target alongside the RustSec advisory
audit. MIT, Apache-2.0, BSD, ISC, Unicode, zlib, Boost, and Apache/LLVM
expressions are allowed. The audit found MPL-2.0 in `option-ext` and the
Symphonia decoder family; these named crates are the only copyleft exception.
MPL permits use in a larger work under other terms, but distribution of covered
source files and modified versions carries source and notice obligations. Any
binary release must provide the corresponding MPL source and notices; see the
[MPL 2.0 text](https://www.mozilla.org/en-US/MPL/2.0/) and
[official FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/). Unknown, other
copyleft, or source-unavailable terms remain rejected pending individual
review. This Rust crate check does not cover system libraries, bundled plugin
binaries, fonts, trademarks, or SDK distribution; those need separate packaging
review.

Project history and timebase semantics are defined by
[`0001-project-action-history.md`](0001-project-action-history.md) and
[`0002-timebase-defaults.md`](0002-timebase-defaults.md). The action history is
an in-memory editing session, not a durable event log: saving persists the
current project snapshot, and reopening starts a fresh undo history. Audio
positions use the project sample clock; musical positions use PPQ ticks and the
tempo map. Conversion is explicit and rounded to the nearest representable
integer unit. Neither representation replaces the other for audio placement or
musical editing.

## Consequences

- The CI build matrix proves compilation and deterministic tests on Linux and
  Windows, but does not prove physical-device latency, driver stability, or
  successful packaging.
- Optional audio backends remain opt-in so portable model/storage tests can run
  without native audio development headers or live devices.
- Dependency license metadata must be kept current when adding or upgrading
  crates. Non-Rust runtime payloads remain subject to their own distribution
  review.
