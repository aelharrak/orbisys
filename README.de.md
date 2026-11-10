# OrbiSys

**Linux-Wartung und Systemverwaltung in einer Anwendung.** OrbiSys vereint eine grafische Oberfläche und eine CLI auf Basis desselben Rust-Kerns.

[![Version](https://img.shields.io/badge/version-1.0.0-2f6f62)](Cargo.toml) [![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange)](https://www.rust-lang.org/) [![MIT-Lizenz](https://img.shields.io/badge/license-MIT-blue)](Cargo.toml)

<p align="center"><img src="packaging/orbisys.svg" alt="OrbiSys-Logo" width="112"></p>

**Sprachen:** [English](README.md) · [Français](README.fr.md) · Deutsch · [Español](README.es.md) · [Português](README.pt.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [中文](README.zh.md) · [العربية](README.ar.md)

## Funktionen

OrbiSys bündelt Werkzeuge zur Systemübersicht und Wartung:

- **System und Überwachung:** Systemzustand, Prozesse, Dienste, Protokolle, Autostart und geplante Aufgaben.
- **Software:** Inventar nativer Pakete, Snap, Flatpak und manuell installierter Anwendungen; Updates, Paketquellen und Laufzeitumgebungen.
- **Speicher und Sicherungen:** Laufwerke, Einhängungen, Bereinigung, Archive, Btrfs, Snapshots und Backups.
- **Netzwerk und Hardware:** lokales Netzwerk, Datenverkehr, WLAN, VPN, Bluetooth, USB, Energie, Sensoren und Firmware.
- **Sicherheit:** Audits, Firewall, Secure Boot, AppArmor, Zertifikate, SSH/GPG-Schlüssel und Integritätsprüfungen.
- **Exporte:** Inventar als TXT, CSV oder JSON; Systemberichte als TXT, CSV, JSON oder PDF.

Einige Funktionen benötigen distributionsabhängige Systemwerkzeuge. Design und Sprache der Oberfläche lassen sich einstellen.

## Installation

Vorkompilierte Pakete liegen in [`dist/`](dist/). Wählen Sie das zur Distribution und zur Architektur passende Paket und führen Sie den Befehl im Projektverzeichnis aus.

| Distribution | Installationsbefehl |
| --- | --- |
| Fedora, RHEL und kompatible Systeme | `sudo dnf install ./dist/orbisys-*.rpm` |
| openSUSE | `sudo zypper install ./dist/orbisys-*.rpm` |
| Debian und Ubuntu | `sudo apt install ./dist/orbisys_*_*.deb` |
| Arch Linux und Derivate | `sudo pacman -U ./dist/orbisys-*.pkg.tar.zst` |
| Alpine Linux | `sudo apk add --allow-untrusted ./dist/orbisys-*.apk` |

Das Alpine-Paket ist eine lokale Datei, die nicht von einem konfigurierten Repository signiert wurde; deshalb ist `--allow-untrusted` erforderlich. Die anderen Paketmanager installieren die lokale Datei und lösen deklarierte Abhängigkeiten auf. Polkit wird für privilegierte Aktionen benötigt: Das Debian-Paket empfiehlt es, die übrigen Formate deklarieren es als Abhängigkeit.

Starten Sie nach der Installation `orbisys` im Terminal oder öffnen Sie **OrbiSys** über das Anwendungsmenü.

### Aus dem Quellcode erstellen

Voraussetzung: Rust und Cargo 1.80 oder neuer. Im Projektverzeichnis:

```sh
cargo build --release
./target/release/orbisys
```

Ohne Argument startet die grafische Oberfläche. Für die CLI: `./target/release/orbisys help`. Während der Entwicklung zum Beispiel: `cargo run -- help`.

## Schnellstart

```sh
orbisys help
orbisys version
orbisys inventory
orbisys inventory --csv software.csv
orbisys updates
orbisys clean                 # analysiert, löscht nichts
orbisys report ./berichte
orbisys panel                 # Rubriken auflisten
orbisys panel monitor
```

## CLI-Befehle

| Befehl | Beschreibung |
| --- | --- |
| `help`, `--help`, `-h` | Hilfe anzeigen. |
| `version`, `--version` | Version anzeigen. |
| `inventory [--txt DATEI] [--csv DATEI] [--json DATEI]` | Softwareinventar anzeigen und angeforderte Exporte schreiben. |
| `updates` | Verfügbare Updates über den erkannten Paketmanager abrufen. |
| `clean` | Bereinigungsziele anzeigen, ohne etwas zu löschen. |
| `clean --apply` | Unterstützte benutzerspezifische Ziele bereinigen; Systemziele mit Root-Bedarf werden übersprungen. |
| `report [VERZEICHNIS]` | TXT-, CSV-, JSON- und PDF-Berichte im angegebenen oder standardmäßig im Dokumente-Verzeichnis schreiben. |
| `panel` / `panel NAME` | Rubriken auflisten / eine Rubrik schreibgeschützt anzeigen. |

`orbisys panel` zeigt gültige Namen, zum Beispiel `monitor`, `health`, `mounts`, `lan` und `certs`.

## Sicherheit und Berechtigungen

Ansichten und Diagnosebefehle verändern das System nicht. Aktionen mit erhöhten Rechten laufen über Polkit (`pkexec`) und erfordern eine Autorisierung. In der CLI bearbeitet `clean --apply` nur benutzerspezifische Ziele.

OrbiSys startet Programme mit getrennten Argumenten und validiert Paketnamen, Unit-Namen und Pfade vor der Verwendung. Verfügbare Systemwerkzeuge unterscheiden sich je nach Distribution.

## Pakete erstellen

Die Skripte in [`packaging/`](packaging/) erstellen DEB-, RPM-, Arch- und Alpine-Pakete aus dem Release-Binary:

```sh
cargo build --release
./packaging/build-deb.sh
./packaging/build-rpm.sh
./packaging/build-arch.sh
./packaging/build-apk.sh
# Alle Formate versuchen
./packaging/build-all.sh
```

Benötigte Werkzeuge: `dpkg-deb` (DEB), `rpmbuild` (RPM), `tar` und `zstd` (Arch), `apk-tools` 3 oder Docker (Alpine). Die Pakete landen in `dist/`. Auf manchen Debian-/Ubuntu-Systemen kann das RPM-Skript seine Werkzeuge lokal herunterladen.

## Sprachen

Die Oberfläche ist auf Französisch, Englisch, Deutsch, Spanisch, Portugiesisch, Italienisch, Niederländisch, Japanisch, Koreanisch, Chinesisch und Arabisch verfügbar. Französisch, Englisch und Spanisch sind vollständig übersetzt; andere Übersetzungen können teilweise fehlen. Fehlende Texte verwenden Englisch.

## Projektstruktur

- `src/main.rs`: Einstiegspunkt und Auswahl GUI/CLI.
- `src/lib.rs`: gemeinsamer Kern und Systemmodule.
- `src/cli.rs`: CLI-Befehle.
- `src/gui.rs`, `src/gui/`: grafische Oberfläche.
- `src/p_*.rs`, `src/panels.rs`: spezialisierte Ansichten.
- `src/lang/`: Anwendungsübersetzungen.
- `packaging/`: Paketskripte und Metadaten.
- `assets/`: Grafiken; `dist/`: vorkompilierte Pakete.

## Lizenz

Das Projekt führt die MIT-Lizenz in [`Cargo.toml`](Cargo.toml). Die enthaltenen Lucide-Symbole haben eine separate Lizenz unter [`assets/icons/LICENSE`](assets/icons/LICENSE).
