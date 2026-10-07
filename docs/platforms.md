# Desktop development and build notes

AAADAW currently verifies Linux and Windows builds in GitHub Actions. It also
builds unsigned Ubuntu 24.04 x86_64 `.deb` and Windows x86_64 MSI validation
packages, plus portable archives. These artifacts are not signed public
releases and do not bundle audio services or drivers. Device-driver behavior
and audio latency still need validation on real hardware (see Issue #7 for
Linux capture).

## Linux

Install the stable Rust toolchain with `rustfmt` and `clippy`. The default build
does not enable a device backend. Linux builds also need the GLib development
package for the isolated CLAP helper's GLib main-context servicing
(`libglib2.0-dev` on Debian/Ubuntu). This services GLib-integrated plug-in UI
events and CLAP main-thread callbacks; it is not a universal event pump for
every X11 toolkit:

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

Release builds above are unpackaged executables. CI also creates the validation
packages described in [native installer notes](native-installers.md). There is
no code-signing, automatic update, or bundle of native audio services. Keep the
chosen backend feature and its required system libraries explicit when
distributing a binary; see [the roadmap](../ROADMAP.md) for remaining release
gates.
