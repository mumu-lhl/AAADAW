# Native installers

The **Native installers** workflow builds Ubuntu 24.04 x86_64 `.deb` and Windows x86_64 MSI artifacts for pull requests that change installer inputs, or on demand. Artifacts include SHA-256 files and expire after 14 days. The workflow does not publish releases or sign binaries.

## Ubuntu 24.04 x86_64

Install the `.deb` with `sudo apt install ./aaadaw-linux-x86_64.deb`. The package installs `/usr/bin/aaadaw`, a desktop launcher, and the project license texts. APT installs the shared libraries required by the JACK and PipeWire build. AAADAW does not install or start an audio server. Connect the machine to a running JACK or PipeWire service and select its device in **Settings → Audio**.

Remove the application with `sudo apt remove aaadaw`. Projects, preferences, plug-in scan data, and logs stay in the user's home directories. Projects remain at paths chosen by the user. The package does not add a maintainer script or delete user data.

## Windows x86_64

Run the MSI to install AAADAW for the current user under `%LOCALAPPDATA%\Programs\AAADAW`. The installer adds an AAADAW shortcut to the Start Menu and registers the package in Windows installed apps. It uses WASAPI and does not install drivers or audio services.

Uninstall AAADAW from Windows **Settings → Apps → Installed apps**. The MSI removes its program files and shortcut. Projects, settings, and logs remain in per-user data folders.

## Headless checks

`aaadaw --version` and `aaadaw --help` run without opening the desktop window or initializing logging. Installers use this path to check the installed executable before audio devices and a display session are available.

## Scope

These are unsigned validation packages, not a public release channel. CI checks package creation, installation, executable startup, launcher registration, MSI upgrade and downgrade handling, and uninstall data preservation. Physical audio devices and a clean end-user desktop still need platform testing; see [portable builds](portable-builds.md) and [platform notes](platforms.md).
