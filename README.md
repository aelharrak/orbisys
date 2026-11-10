# OrbiSys

**Linux maintenance and system administration in one application.** OrbiSys brings together a graphical interface and a CLI built on the same Rust core.

[![Version](https://img.shields.io/badge/version-1.0.0-2f6f62)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange)](https://www.rust-lang.org/)
[![MIT License](https://img.shields.io/badge/license-MIT-blue)](Cargo.toml)

<p align="center">
  <img src="packaging/orbisys.svg" alt="OrbiSys logo" width="112">
</p>

**Website:** [OrbiSys project site](https://aelharrak.github.io/orbisys/) · [Source code](https://github.com/aelharrak/orbisys)

**Languages:** English · [Français](README.fr.md) · [Deutsch](README.de.md) · [Español](README.es.md) · [Português](README.pt.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [中文](README.zh.md) · [العربية](README.ar.md)

## Contents

- [Features](#features)
- [Installation](#installation)
- [Quick start](#quick-start)
- [CLI commands](#cli-commands)
- [Security and permissions](#security-and-permissions)
- [Building packages](#building-packages)
- [Languages](#languages)
- [Project structure](#project-structure)
- [License](#license)

## Features

OrbiSys provides system inspection and maintenance tools grouped by area:

- **System and monitoring:** machine health, processes, services, logs, startup items, and scheduled tasks.
- **Software:** inventory of native packages, Snap, Flatpak, and manually installed applications; updates, repositories, and runtimes.
- **Storage and backups:** disks, mounts, cleanup, archives, Btrfs, snapshots, and backups.
- **Networking and hardware:** local network, traffic, Wi-Fi, VPN, Bluetooth, USB, power, sensors, and firmware.
- **Security:** audits, firewall, Secure Boot, AppArmor, certificates, SSH/GPG keys, and integrity checks.
- **Exports:** inventory in TXT, CSV, or JSON; system reports in TXT, CSV, JSON, or PDF.

Some features depend on the tools available on your Linux distribution. The interface also lets you configure its theme and language.

## Installation

Prebuilt packages are included in [`dist/`](dist/) for the architectures shown in their filenames. From the repository root, use the package format for your distribution.

| Distribution | Install command |
| --- | --- |
| Fedora, RHEL, and compatible distributions | `sudo dnf install ./dist/orbisys-*.rpm` |
| openSUSE | `sudo zypper install ./dist/orbisys-*.rpm` |
| Debian and Ubuntu | `sudo apt install ./dist/orbisys_*_*.deb` |
| Arch Linux and derivatives | `sudo pacman -U ./dist/orbisys-*.pkg.tar.zst` |
| Alpine Linux | `sudo apk add --allow-untrusted ./dist/orbisys-*.apk` |

The included Alpine package is a local file that is not signed by a configured repository, so `--allow-untrusted` is required. The other package managers install the local file and resolve declared dependencies. Polkit is required for privileged operations; it is recommended by the Debian package and declared as a dependency by the other package formats.

After installation, run `orbisys` in a terminal or launch **OrbiSys** from your applications menu.

## Uninstall

Use your distribution's package manager to remove OrbiSys:

| Distribution | Command |
| --- | --- |
| Fedora and RHEL | `sudo dnf remove orbisys` |
| openSUSE | `sudo zypper remove orbisys` |
| Debian and Ubuntu | `sudo apt remove orbisys` |
| Arch Linux | `sudo pacman -R orbisys` |
| Alpine Linux | `sudo apk del orbisys` |

If you installed OrbiSys from the PPA, you can remove the PPA separately with `sudo add-apt-repository --remove ppa:amine-elharrak/ppa`. If you built it from source, close the app and delete the cloned repository folder.

### Build from source

Requirements: Rust and Cargo 1.80 or later. From the project root, build the release binary:

```sh
cargo build --release
```

Launch the graphical interface:

```sh
./target/release/orbisys
```

With no arguments, the binary opens the graphical interface. To use the CLI:

```sh
./target/release/orbisys help
```

For development, you can run the same commands with Cargo, for example `cargo run -- help`.

## Quick start

```sh
# Show help and version
orbisys help
orbisys version

# View the software inventory and export it
orbisys inventory
orbisys inventory --csv software.csv

# Check for available updates
orbisys updates

# Review cleanup targets without deleting anything
orbisys clean

# Write reports to a directory
orbisys report ./reports

# List CLI panels, then view one panel
orbisys panel
orbisys panel monitor
```

## CLI commands

```text
orbisys [command] [options]
```

| Command | Description |
| --- | --- |
| `help`, `--help`, `-h` | Show help. |
| `version`, `--version` | Show the version. |
| `inventory [--txt FILE] [--csv FILE] [--json FILE]` | Display the software inventory and write any requested exports. |
| `updates` | Check for available updates using the detected package manager. |
| `clean` | Measure and show cleanup targets without deleting anything. |
| `clean --apply` | Clean supported user-level targets. Targets requiring root privileges are skipped in the CLI. |
| `report [DIRECTORY]` | Write TXT, CSV, JSON, and PDF reports to the specified directory, or to the default Documents directory. |
| `panel` | List available panel names. |
| `panel NAME` | Display information for a panel in read-only mode. |

Run `orbisys panel` to see the accepted names. Examples include `monitor`, `health`, `mounts`, `lan`, and `certs`.

## Security and permissions

Inspection screens and diagnostic commands do not modify the system. Operations that require elevated privileges are delegated to Polkit through `pkexec` and request authorization. In the CLI, `clean --apply` only processes user-level targets; system-level targets are skipped.

OrbiSys invokes programs with separate arguments and validates package names, unit names, and paths before use. The availability and configuration of system tools may vary between distributions.

## Building packages

The scripts in [`packaging/`](packaging/) build DEB, RPM, Arch, and Alpine packages from the release binary. Build the binary first, then run the scripts from the repository root:

```sh
cargo build --release
./packaging/build-deb.sh
./packaging/build-rpm.sh
./packaging/build-arch.sh
./packaging/build-apk.sh
```

To attempt all package builds:

```sh
./packaging/build-all.sh
```

Tool requirements vary by format: `dpkg-deb` for DEB, `rpmbuild` for RPM, `tar` and `zstd` for Arch, and `apk-tools` 3 or Docker for Alpine. The scripts write packages to `dist/`. On some Debian/Ubuntu systems, the RPM build script can fetch its tools locally.

### Preparing packages for distribution

The local packages in `dist/` are useful for testing and direct downloads. The Arch, Alpine, and RPM recipes fetch a versioned source archive from the GitHub release, so create and attach that archive before publishing the recipe:

```sh
./packaging/create-vendor-source.sh
```

This creates `dist/orbisys-1.0.0-vendor.tar.gz`. Attach it to the GitHub release tagged `v1.0.0`; then publish the corresponding recipe to the relevant community repository (AUR, Alpine aports, or an RPM build service such as OBS). Update version numbers and checksums for each release.

For an Ubuntu PPA source upload, create the source package from the same archive:

```sh
./packaging/create-ubuntu-source.sh
```

The script writes the unsigned source upload files to `dist/ubuntu-source/`. Sign the generated `.changes` file with your Launchpad key using `debsign`, then upload it to your PPA with `dput`. Launchpad builds the binary packages for users. The current Debian metadata targets Ubuntu 26.04 (`resolute`); change `debian/changelog` to the intended Ubuntu series before preparing an upload for another release.

## Languages

The interface is available in French, English, German, Spanish, Portuguese, Italian, Dutch, Japanese, Korean, Chinese, and Arabic. French, English, and Spanish have complete translation tables. Other translations may be partial; missing labels fall back to English.

## Project structure

- `src/main.rs`: entry point and GUI/CLI selection.
- `src/lib.rs`: shared core and system modules.
- `src/cli.rs`: command-line interface.
- `src/gui.rs`, `src/gui/`: graphical interface.
- `src/p_*.rs`, `src/panels.rs`: panels and specialized views.
- `src/lang/`: translation files.
- `packaging/`: Linux package scripts and metadata.
- `assets/`: graphical resources.
- `dist/`: included prebuilt packages.

## License

The project declares the MIT license in [`Cargo.toml`](Cargo.toml). The included Lucide icons have a separate license in [`assets/icons/LICENSE`](assets/icons/LICENSE).
