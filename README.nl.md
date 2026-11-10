# OrbiSys

**Linux-onderhoud en systeembeheer in één toepassing.** OrbiSys combineert een grafische interface en een CLI op basis van dezelfde Rust-kern.

[![Versie](https://img.shields.io/badge/version-1.0.0-2f6f62)](Cargo.toml) [![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange)](https://www.rust-lang.org/) [![MIT-licentie](https://img.shields.io/badge/license-MIT-blue)](Cargo.toml)

<p align="center"><img src="packaging/orbisys.svg" alt="OrbiSys-logo" width="112"></p>

**Talen:** [English](README.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Español](README.es.md) · [Português](README.pt.md) · [Italiano](README.it.md) · Nederlands · [日本語](README.ja.md) · [한국어](README.ko.md) · [中文](README.zh.md) · [العربية](README.ar.md)

## Functies

OrbiSys bundelt hulpmiddelen voor systeeminspectie en onderhoud:

- **Systeem en bewaking:** systeemstatus, processen, services, logboeken, opstartitems en geplande taken.
- **Software:** inventaris van native pakketten, Snap, Flatpak en handmatig geïnstalleerde apps; updates, pakketbronnen en runtimes.
- **Opslag en back-ups:** schijven, aankoppelingen, opschonen, archieven, Btrfs, snapshots en back-ups.
- **Netwerk en hardware:** lokaal netwerk, verkeer, wifi, VPN, Bluetooth, USB, energie, sensoren en firmware.
- **Beveiliging:** audits, firewall, Secure Boot, AppArmor, certificaten, SSH/GPG-sleutels en integriteitscontroles.
- **Exports:** inventaris als TXT, CSV of JSON; systeemrapporten als TXT, CSV, JSON of PDF.

Sommige functies zijn afhankelijk van de hulpmiddelen die op de distributie zijn geïnstalleerd. Thema en taal van de interface zijn instelbaar.

## Installatie

Voorgebouwde pakketten staan in [`dist/`](dist/) voor de architecturen die in de bestandsnamen worden vermeld. Kies vanuit de hoofdmap van de repository het formaat voor je distributie.

| Distributie | Installatieopdracht |
| --- | --- |
| Fedora, RHEL en compatibele systemen | `sudo dnf install ./dist/orbisys-*.rpm` |
| openSUSE | `sudo zypper install ./dist/orbisys-*.rpm` |
| Debian en Ubuntu | `sudo apt install ./dist/orbisys_*_*.deb` |
| Arch Linux en afgeleiden | `sudo pacman -U ./dist/orbisys-*.pkg.tar.zst` |
| Alpine Linux | `sudo apk add --allow-untrusted ./dist/orbisys-*.apk` |

Het meegeleverde Alpine-pakket is een lokaal bestand dat niet door een geconfigureerde pakketbron is ondertekend; daarom is `--allow-untrusted` nodig. De andere pakketbeheerders installeren het lokale bestand en lossen de opgegeven afhankelijkheden op. Polkit is nodig voor bewerkingen met verhoogde rechten: het Debian-pakket beveelt het aan, de andere formaten geven het op als afhankelijkheid.

Start na de installatie `orbisys` in een terminal of open **OrbiSys** via het toepassingenmenu.

## Verwijderen

Verwijder OrbiSys met de pakketbeheerder van uw distributie:

| Distributie | Opdracht |
| --- | --- |
| Fedora en RHEL | `sudo dnf remove orbisys` |
| openSUSE | `sudo zypper remove orbisys` |
| Debian en Ubuntu | `sudo apt remove orbisys` |
| Arch Linux | `sudo pacman -R orbisys` |
| Alpine Linux | `sudo apk del orbisys` |

Als u OrbiSys via de PPA hebt geïnstalleerd, verwijder dan ook de pakketbron met `sudo add-apt-repository --remove ppa:amine-elharrak/ppa`. Sluit de toepassing en verwijder de gekloonde repositorymap als u de broncode hebt gebouwd.

### Bouwen vanuit broncode

Vereist: Rust en Cargo 1.80 of hoger. Voer in de projectmap uit:

```sh
cargo build --release
./target/release/orbisys
```

Zonder argumenten start de grafische interface. Voor de CLI: `./target/release/orbisys help`. Tijdens ontwikkeling kun je bijvoorbeeld `cargo run -- help` gebruiken.

## Snel aan de slag

```sh
orbisys help
orbisys version
orbisys inventory
orbisys inventory --csv software.csv
orbisys updates
orbisys clean                 # analyseert zonder iets te verwijderen
orbisys report ./rapporten
orbisys panel                 # toont de rubrieken
orbisys panel monitor
```

## CLI-opdrachten

| Opdracht | Beschrijving |
| --- | --- |
| `help`, `--help`, `-h` | Toon de helpinformatie. |
| `version`, `--version` | Toon de versie. |
| `inventory [--txt BESTAND] [--csv BESTAND] [--json BESTAND]` | Toon de software-inventaris en schrijf gevraagde exports. |
| `updates` | Controleer updates via de gevonden pakketbeheerder. |
| `clean` | Toon opschoondoelen zonder iets te verwijderen. |
| `clean --apply` | Schoon ondersteunde gebruikersdoelen op; systeemdoelen waarvoor root nodig is, worden overgeslagen. |
| `report [MAP]` | Schrijf TXT-, CSV-, JSON- en PDF-rapporten naar de opgegeven map of standaard naar Documenten. |
| `panel` / `panel NAAM` | Toon de rubrieken / toon een rubriek alleen-lezen. |

`orbisys panel` toont de geaccepteerde namen, bijvoorbeeld `monitor`, `health`, `mounts`, `lan` en `certs`.

## Beveiliging en rechten

Inspectieschermen en diagnoseopdrachten wijzigen het systeem niet. Bewerkingen waarvoor verhoogde rechten nodig zijn, verlopen via Polkit (`pkexec`) en vereisen toestemming. In de CLI verwerkt `clean --apply` alleen gebruikersdoelen.

OrbiSys start programma's met afzonderlijke argumenten en valideert pakketnamen, unitnamen en paden vóór gebruik. Beschikbare systeemhulpmiddelen kunnen per distributie verschillen.

## Pakketten bouwen

De scripts in [`packaging/`](packaging/) maken DEB-, RPM-, Arch- en Alpine-pakketten van de release-binary:

```sh
cargo build --release
./packaging/build-deb.sh
./packaging/build-rpm.sh
./packaging/build-arch.sh
./packaging/build-apk.sh
# Probeer alle formaten
./packaging/build-all.sh
```

Benodigd gereedschap: `dpkg-deb` (DEB), `rpmbuild` (RPM), `tar` en `zstd` (Arch), en `apk-tools` 3 of Docker (Alpine). De pakketten worden in `dist/` geplaatst. Op sommige Debian-/Ubuntu-systemen kan het RPM-script de benodigde hulpmiddelen lokaal ophalen.

## Talen

De interface is beschikbaar in het Frans, Engels, Duits, Spaans, Portugees, Italiaans, Nederlands, Japans, Koreaans, Chinees en Arabisch. De Franse, Engelse en Spaanse vertalingen zijn volledig; andere kunnen gedeeltelijk zijn. Ontbrekende teksten vallen terug op Engels.

## Projectstructuur

- `src/main.rs`: ingangspunt en keuze tussen GUI/CLI.
- `src/lib.rs`: gedeelde kern en systeemmodules.
- `src/cli.rs`: CLI-opdrachten.
- `src/gui.rs`, `src/gui/`: grafische interface.
- `src/p_*.rs`, `src/panels.rs`: gespecialiseerde panelen.
- `src/lang/`: vertalingen van de toepassing.
- `packaging/`: pakketscripts en metadata.
- `assets/`: grafische bestanden; `dist/`: meegeleverde pakketten.

## Licentie

Het project vermeldt de MIT-licentie in [`Cargo.toml`](Cargo.toml). De meegeleverde Lucide-pictogrammen hebben een aparte licentie in [`assets/icons/LICENSE`](assets/icons/LICENSE).
