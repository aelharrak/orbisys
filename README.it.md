# OrbiSys

**Manutenzione e amministrazione Linux in un'unica applicazione.** OrbiSys riunisce un'interfaccia grafica e una CLI basate sullo stesso nucleo Rust.

[![Versione](https://img.shields.io/badge/version-1.0.0-2f6f62)](Cargo.toml) [![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange)](https://www.rust-lang.org/) [![Licenza MIT](https://img.shields.io/badge/license-MIT-blue)](Cargo.toml)

<p align="center"><img src="packaging/orbisys.svg" alt="Logo OrbiSys" width="112"></p>

**Lingue:** [English](README.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Español](README.es.md) · [Português](README.pt.md) · Italiano · [Nederlands](README.nl.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [中文](README.zh.md) · [العربية](README.ar.md)

## Funzionalità

OrbiSys raccoglie strumenti di consultazione e manutenzione:

- **Sistema e monitoraggio:** stato del computer, processi, servizi, log, avvio e attività pianificate.
- **Software:** inventario di pacchetti nativi, Snap, Flatpak e applicazioni installate manualmente; aggiornamenti, repository e runtime.
- **Archiviazione e backup:** dischi, mount, pulizia, archivi, Btrfs, snapshot e backup.
- **Rete e hardware:** rete locale, traffico, Wi-Fi, VPN, Bluetooth, USB, alimentazione, sensori e firmware.
- **Sicurezza:** audit, firewall, Secure Boot, AppArmor, certificati, chiavi SSH/GPG e controlli d'integrità.
- **Esportazioni:** inventario TXT, CSV o JSON; rapporti di sistema TXT, CSV, JSON o PDF.

Alcune funzioni dipendono dagli strumenti installati nella distribuzione. Tema e lingua dell'interfaccia sono configurabili.

## Installazione

I pacchetti precompilati sono in [`dist/`](dist/), per le architetture indicate nei nomi dei file. Dalla directory principale del repository, scegli il formato per la tua distribuzione.

| Distribuzione | Comando d'installazione |
| --- | --- |
| Fedora, RHEL e compatibili | `sudo dnf install ./dist/orbisys-*.rpm` |
| openSUSE | `sudo zypper install ./dist/orbisys-*.rpm` |
| Debian e Ubuntu | `sudo apt install ./dist/orbisys_*_*.deb` |
| Arch Linux e derivate | `sudo pacman -U ./dist/orbisys-*.pkg.tar.zst` |
| Alpine Linux | `sudo apk add --allow-untrusted ./dist/orbisys-*.apk` |

Il pacchetto Alpine incluso è un file locale non firmato da un repository configurato; è quindi necessaria l'opzione `--allow-untrusted`. Gli altri gestori installano il file locale e risolvono le dipendenze dichiarate. Polkit è necessario per le operazioni privilegiate: il pacchetto Debian lo raccomanda, gli altri lo dichiarano come dipendenza.

Dopo l'installazione, avvia `orbisys` da terminale oppure apri **OrbiSys** dal menu delle applicazioni.

## Disinstallazione

Rimuovi OrbiSys con il gestore di pacchetti della tua distribuzione:

| Distribuzione | Comando |
| --- | --- |
| Fedora e RHEL | `sudo dnf remove orbisys` |
| openSUSE | `sudo zypper remove orbisys` |
| Debian e Ubuntu | `sudo apt remove orbisys` |
| Arch Linux | `sudo pacman -R orbisys` |
| Alpine Linux | `sudo apk del orbisys` |

Se hai installato OrbiSys dal PPA, rimuovi anche il repository con `sudo add-apt-repository --remove ppa:amine-elharrak/ppa`. Se hai compilato dai sorgenti, chiudi l'applicazione ed elimina la cartella clonata del repository.

### Compilazione dai sorgenti

Requisiti: Rust e Cargo 1.80 o successivi. Dalla directory principale del progetto:

```sh
cargo build --release
./target/release/orbisys
```

Senza argomenti si apre l'interfaccia grafica. Per la CLI: `./target/release/orbisys help`. In sviluppo, ad esempio: `cargo run -- help`.

## Avvio rapido

```sh
orbisys help
orbisys version
orbisys inventory
orbisys inventory --csv software.csv
orbisys updates
orbisys clean                 # analizza senza eliminare
orbisys report ./rapporti
orbisys panel                 # elenca le sezioni
orbisys panel monitor
```

## Comandi CLI

| Comando | Descrizione |
| --- | --- |
| `help`, `--help`, `-h` | Mostra la guida. |
| `version`, `--version` | Mostra la versione. |
| `inventory [--txt FILE] [--csv FILE] [--json FILE]` | Mostra l'inventario e scrive le esportazioni richieste. |
| `updates` | Controlla gli aggiornamenti con il gestore rilevato. |
| `clean` | Mostra gli elementi da pulire senza eliminarli. |
| `clean --apply` | Pulisce gli elementi utente supportati; salta quelli di sistema che richiedono root. |
| `report [CARTELLA]` | Scrive rapporti TXT, CSV, JSON e PDF nella cartella indicata o, per impostazione predefinita, in Documenti. |
| `panel` / `panel NOME` | Elenca le sezioni / mostra una sezione in sola lettura. |

`orbisys panel` mostra i nomi validi, ad esempio `monitor`, `health`, `mounts`, `lan` e `certs`.

## Sicurezza e permessi

Le schermate di consultazione e i comandi diagnostici non modificano il sistema. Le operazioni privilegiate passano tramite Polkit (`pkexec`) e richiedono autorizzazione. Nella CLI, `clean --apply` elabora solo gli elementi utente.

OrbiSys avvia i programmi con argomenti separati e convalida nomi di pacchetti, unità e percorsi prima dell'uso. Gli strumenti di sistema disponibili possono variare in base alla distribuzione.

## Creazione dei pacchetti

Gli script in [`packaging/`](packaging/) creano pacchetti DEB, RPM, Arch e Alpine dal binario release:

```sh
cargo build --release
./packaging/build-deb.sh
./packaging/build-rpm.sh
./packaging/build-arch.sh
./packaging/build-apk.sh
# Tenta tutti i formati
./packaging/build-all.sh
```

Strumenti richiesti: `dpkg-deb` (DEB), `rpmbuild` (RPM), `tar` e `zstd` (Arch), `apk-tools` 3 o Docker (Alpine). I pacchetti vengono salvati in `dist/`. Su alcuni sistemi Debian/Ubuntu, lo script RPM può scaricare localmente i propri strumenti.

## Lingue

L'interfaccia è disponibile in francese, inglese, tedesco, spagnolo, portoghese, italiano, olandese, giapponese, coreano, cinese e arabo. Le traduzioni francese, inglese e spagnola sono complete; le altre possono essere parziali e i testi mancanti ricorrono all'inglese.

## Struttura del progetto

- `src/main.rs`: punto d'ingresso e scelta GUI/CLI.
- `src/lib.rs`: nucleo condiviso e moduli di sistema.
- `src/cli.rs`: comandi CLI.
- `src/gui.rs`, `src/gui/`: interfaccia grafica.
- `src/p_*.rs`, `src/panels.rs`: pannelli specializzati.
- `src/lang/`: traduzioni dell'applicazione.
- `packaging/`: script e metadati dei pacchetti.
- `assets/`: risorse grafiche; `dist/`: pacchetti precompilati.

## Licenza

Il progetto dichiara la licenza MIT in [`Cargo.toml`](Cargo.toml). Le icone Lucide incluse hanno una licenza separata in [`assets/icons/LICENSE`](assets/icons/LICENSE).
