# Desktop development and build notes

AAADAW currently verifies Linux and Windows builds in GitHub Actions. These
steps build development binaries; the project does not yet produce signed
installers or a self-contained release package. Device-driver behavior and
audio latency still need validation on real hardware (see Issue #7 for Linux
capture).

## Linux

Install the stable Rust toolchain with `rustfmt` and `clippy`. The default build
does not enable a device backend:

```sh
cargo run -p aaadaw
cargo build --release -p aaadaw
```

For JACK controls and playback, install the JACK development headers
(`libjack-jackd2-dev` on Debian/Ubuntu) and enable the backend:

```sh
cargo run -p aaadaw --features aaadaw/jack-backend
cargo build --release -p aaadaw --features aaadaw/jack-backend
```

For PipeWire, install the package providing `libpipewire-0.3` development files
and use `aaadaw/pipewire-backend`. To expose both backends, enable both feature
names. A running server and a working audio route are required for playback.
The Linux build enables X11 and Wayland Iced shells; the host needs a compatible
display server and graphics stack to launch the desktop application.

## Windows

Install the stable Rust MSVC toolchain and Visual Studio Build Tools with the
Desktop development with C++ workload. The portable build and the WASAPI build
are checked in CI:

```powershell
cargo run -p aaadaw
cargo check --workspace --all-targets --features aaadaw/wasapi-backend
cargo build --release -p aaadaw --features aaadaw/wasapi-backend
```

The WASAPI feature enables device playback and recording when a Windows audio
device is available. CI tests policy and device-enumeration failure behavior;
it cannot validate a physical device or its driver.

## Packaging status

Release builds above are unpackaged executables. There is no installer,
code-signing, automatic update, or bundle of native audio services. Keep the
chosen backend feature and its required system libraries explicit when
distributing a binary. Installer generation and clean-machine installation
checks are Phase 1 release-candidate work in [the roadmap](../ROADMAP.md).
