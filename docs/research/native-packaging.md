# Linux `.deb` and Windows MSI packaging research

Scope: Issue #186, Ubuntu 24.04 x86_64 and Windows x86_64 native packages. This is a packaging recommendation, not evidence that either installer has been validated on a clean machine.

## Ubuntu 24.04 `.deb`

Use [`cargo-deb`](https://github.com/kornelski/cargo-deb) for the first package. It consumes Cargo metadata and supports package metadata, explicit assets, cargo feature selection, and automatically generated runtime `Depends` (`$auto`). Keep the existing Linux backend feature set (`jack-backend`, `pipewire-backend`) and add desktop integration assets such as a `.desktop` launcher and icon under standard `/usr/share` paths. Include both project license texts and a useful package description. List JACK and PipeWire runtime package names explicitly because JACK may load dynamically and remain invisible to ELF dependency scanning. Ubuntu 24.04 uses `libpipewire-0.3-0t64` for PipeWire; see the [Ubuntu package record](https://packages.ubuntu.com/noble/libs/libpipewire-0.3-0t64).

Do not list build headers as runtime dependencies. Inspect the resulting control metadata (`dpkg-deb -I`) and ELF dependencies (`dpkg-shlibdeps`/`ldd`) to confirm the exact JACK/PipeWire shared-library package dependencies on Ubuntu 24.04; keep `$auto` and add explicit relationships only where ELF scanning cannot express a genuine requirement. Test installation with `apt install ./...deb` on a fresh Ubuntu 24.04 image, launch the app, and test upgrade and purge. The app has no system service: avoid maintainer scripts and preserve projects and per-user settings/logs on uninstall.

Debian Policy defines binary package contents and dependency relationships; Ubuntu's packaging guide is the distro-specific workflow reference. See [binary packages](https://www.debian.org/doc/debian-policy/ch-binary.html), [relationships](https://www.debian.org/doc/debian-policy/ch-relationships.html), [maintainer scripts](https://www.debian.org/doc/debian-policy/ch-maintainerscripts.html), and [Ubuntu packaging documentation](https://documentation.ubuntu.com/packaging/en/latest/).

## Windows MSI

Use WiX Toolset to author the MSI. This project invokes the WiX CLI directly and pins its major version in CI; [`cargo-wix`](https://github.com/volks73/cargo-wix) is an alternative for projects that want Cargo-driven MSI generation. Install WiX explicitly instead of relying on the runner's preinstalled legacy toolset. Before adopting current WiX releases, review its [official licensing/maintenance-fee terms](https://github.com/wixtoolset/wix#open-source-maintenance-fee) for the project's distribution model.

Give the product a stable `UpgradeCode`; use a new `ProductCode` for each major upgrade and a monotonically increasing MSI `Version`. Author WiX `MajorUpgrade` so a newer MSI replaces an older version and downgrades are blocked with a clear message. Include the app executable and license/readme, register Add/Remove Programs metadata and shortcuts, and ensure uninstall removes only installed product files. Store user projects and preferences in per-user data locations, outside the install directory. Choose per-machine installation only if the release workflow is prepared for elevation; otherwise make the MSI explicitly per-user. Exercise fresh install → upgrade → downgrade refusal → uninstall, while verifying project data survives.

Primary references: [WiX `MajorUpgrade` schema](https://docs.firegiant.com/wix/schema/wxs/majorupgrade/), [WiX `Package` schema](https://docs.firegiant.com/wix/schema/wxs/package/), [Microsoft major upgrades guidance](https://learn.microsoft.com/en-us/windows/win32/msi/major-upgrades), and the [cargo-wix setup and modern-toolset documentation](https://github.com/volks73/cargo-wix#quick-start).

## Repo-specific recommendation

Keep portable archives as a separate artifact from installers. Add `.deb` and `.msi` generation as separate CI jobs from tagged/reviewed source, each with its own install/upgrade/uninstall validation. Do not make package installation start audio servers, grant device permissions globally, or remove user data. The Linux package must declare the runtime libraries actually linked by the selected backend build; the Windows MSI should rely on Windows system audio APIs and report any non-system DLLs during clean-VM validation.
