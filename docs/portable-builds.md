# Portable desktop builds

The repository's **Actions → Portable desktop archives** workflow creates Linux and Windows archives for pull requests that change build inputs, or on demand. It uploads workflow artifacts for 14 days. It does not publish a GitHub Release, installer, signed binary, updater, or audio service.

Each archive contains the AAADAW executable, this guide, the MIT and Apache 2.0 project license texts, and a build manifest with the target, feature set, and source commit. The adjacent `.sha256` file lets you verify the archive after download.

## Linux x86_64

The archive targets the Ubuntu 24.04 x86_64 runtime baseline and enables both JACK and PipeWire. Extract it, then launch the `aaadaw` executable from a terminal:

```sh
tar -xzf aaadaw-linux-x86_64.tar.gz
cd aaadaw-linux-x86_64
./aaadaw
```

The host must provide a compatible display server and graphics driver, `libjack` and `libpipewire-0.3` runtime libraries, and a running JACK or PipeWire service with an audio route. AAADAW does not install or start these services. If launch fails, check `ldd ./aaadaw` for missing shared libraries and see [platform build notes](platforms.md).

## Windows x86_64

The archive targets Windows x86_64 and enables WASAPI. Extract the ZIP and run `aaadaw.exe`. On first launch, open **Settings → Audio** to select the WASAPI output and input devices available on the machine. Recording requires an armed track and an available input device.

## Projects, settings, and logs

Use **File → New Project** or **File → Open Project**. Save projects as `.aaadaw` files in a location included in your backup plan. The project stays at the path you choose.

AAADAW stores configuration and plug-in scan cache in the current user's configuration directory:

- Linux: `$XDG_CONFIG_HOME/aaadaw`, or `~/.config/aaadaw` when `XDG_CONFIG_HOME` is not set.
- Windows: `%APPDATA%\aaadaw`.

Logs use a separate local data directory:

- Linux: `$XDG_DATA_HOME/aaadaw/logs`, or `~/.local/share/aaadaw/logs` when `XDG_DATA_HOME` is not set.
- Windows: `%LOCALAPPDATA%\AAADAW\AAADAW\data\logs`.

Set `AAADAW_LOG_DIR` to override the log directory. See [logging and diagnostics](logging.md) for `RUST_LOG` filtering and more detail.

## Plug-ins and current limits

CLAP entry scanning uses a helper process. Loaded CLAP plug-ins still run inside AAADAW with the user's privileges and are not sandboxed; a plug-in can crash or stall the application. Do not open an untrusted project that loads plug-ins you do not trust.

The archives are portable builds, not installers. They do not bundle audio services, device drivers, CLAP plug-ins, or platform graphics drivers. Physical audio latency and device behavior still require testing on the target machine; see [Issue #7](https://github.com/mumu-lhl/AAADAW/issues/7).
