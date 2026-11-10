# OrbiSys

**Maintenance et administration Linux dans une seule application.** OrbiSys réunit une interface graphique et une CLI construites sur le même noyau Rust.

[![Version](https://img.shields.io/badge/version-1.0.0-2f6f62)](Cargo.toml) [![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange)](https://www.rust-lang.org/) [![Licence MIT](https://img.shields.io/badge/licence-MIT-blue)](Cargo.toml)

<p align="center"><img src="packaging/orbisys.svg" alt="Logo OrbiSys" width="112"></p>

**Langues :** [English](README.md) · Français · [Deutsch](README.de.md) · [Español](README.es.md) · [Português](README.pt.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [中文](README.zh.md) · [العربية](README.ar.md)

## Fonctionnalités

OrbiSys rassemble des outils d'inspection et de maintenance :

- **Système et surveillance :** état de la machine, processus, services, journaux, démarrage et tâches planifiées.
- **Logiciels :** inventaire des paquets natifs, Snap, Flatpak et applications installées manuellement ; mises à jour, dépôts et environnements d'exécution.
- **Stockage et sauvegardes :** disques, montages, nettoyage, archives, Btrfs, instantanés et sauvegardes.
- **Réseau et matériel :** réseau local, trafic, Wi-Fi, VPN, Bluetooth, USB, énergie, capteurs et micrologiciel.
- **Sécurité :** audits, pare-feu, Secure Boot, AppArmor, certificats, clés SSH/GPG et vérifications d'intégrité.
- **Exports :** inventaire en TXT, CSV ou JSON ; rapports système en TXT, CSV, JSON ou PDF.

Certaines fonctions dépendent des outils installés sur la distribution. Le thème et la langue de l'interface sont configurables.

## Installation

Des paquets précompilés sont fournis dans [`dist/`](dist/) pour les architectures indiquées dans leur nom. Depuis la racine du dépôt, choisissez le format de votre distribution.

| Distribution | Commande d'installation |
| --- | --- |
| Fedora, RHEL et dérivées | `sudo dnf install ./dist/orbisys-*.rpm` |
| openSUSE | `sudo zypper install ./dist/orbisys-*.rpm` |
| Debian et Ubuntu | `sudo apt install ./dist/orbisys_*_*.deb` |
| Arch Linux et dérivées | `sudo pacman -U ./dist/orbisys-*.pkg.tar.zst` |
| Alpine Linux | `sudo apk add --allow-untrusted ./dist/orbisys-*.apk` |

Le paquet Alpine fourni est un fichier local non signé par un dépôt configuré ; l'option `--allow-untrusted` est donc nécessaire. Les autres gestionnaires installent le fichier local et résolvent ses dépendances. Polkit est nécessaire aux opérations privilégiées : il est recommandé par le paquet Debian et déclaré comme dépendance par les autres formats.

Après installation, lancez `orbisys` dans un terminal ou ouvrez **OrbiSys** depuis le menu des applications.

## Désinstallation

Utilisez le gestionnaire de paquets de votre distribution pour supprimer OrbiSys :

| Distribution | Commande |
| --- | --- |
| Fedora et RHEL | `sudo dnf remove orbisys` |
| openSUSE | `sudo zypper remove orbisys` |
| Debian et Ubuntu | `sudo apt remove orbisys` |
| Arch Linux | `sudo pacman -R orbisys` |
| Alpine Linux | `sudo apk del orbisys` |

Si OrbiSys a été installé depuis le PPA, retirez aussi le dépôt avec `sudo add-apt-repository --remove ppa:amine-elharrak/ppa`. Si vous l'avez compilé depuis les sources, fermez l'application puis supprimez le dossier du dépôt cloné.

### Compiler depuis les sources

Prérequis : Rust et Cargo 1.80 ou ultérieur. Depuis la racine du projet :

```sh
cargo build --release
./target/release/orbisys
```

Sans argument, le binaire ouvre l'interface graphique. Pour la CLI, utilisez `./target/release/orbisys help`. En développement, utilisez par exemple `cargo run -- help`.

## Démarrage rapide

```sh
orbisys help
orbisys version
orbisys inventory
orbisys inventory --csv logiciels.csv
orbisys updates
orbisys clean                 # analyse sans suppression
orbisys report ./rapports
orbisys panel                 # liste les rubriques
orbisys panel monitor
```

## Commandes CLI

| Commande | Description |
| --- | --- |
| `help`, `--help`, `-h` | Affiche l'aide. |
| `version`, `--version` | Affiche la version. |
| `inventory [--txt FICHIER] [--csv FICHIER] [--json FICHIER]` | Affiche l'inventaire et écrit les exports demandés. |
| `updates` | Consulte les mises à jour via le gestionnaire détecté. |
| `clean` | Affiche les cibles de nettoyage sans rien supprimer. |
| `clean --apply` | Nettoie les cibles personnelles prises en charge ; les cibles système nécessitant root sont ignorées. |
| `report [DOSSIER]` | Écrit les rapports TXT, CSV, JSON et PDF dans le dossier indiqué, ou dans Documents par défaut. |
| `panel` / `panel NOM` | Liste les rubriques / affiche une rubrique en lecture seule. |

Les noms acceptés sont affichés par `orbisys panel` ; exemples : `monitor`, `health`, `mounts`, `lan`, `certs`.

## Sécurité et permissions

Les vues de consultation et les commandes de diagnostic ne modifient pas le système. Les opérations privilégiées passent par Polkit (`pkexec`) et demandent une autorisation. En CLI, `clean --apply` ne traite que les cibles personnelles.

OrbiSys lance les programmes avec des arguments séparés et valide les noms de paquets, d'unités et les chemins avant leur utilisation. Les outils disponibles varient selon la distribution.

## Construire les paquets

Les scripts de [`packaging/`](packaging/) produisent les formats DEB, RPM, Arch et Alpine à partir du binaire de production :

```sh
cargo build --release
./packaging/build-deb.sh
./packaging/build-rpm.sh
./packaging/build-arch.sh
./packaging/build-apk.sh
# Tous les formats
./packaging/build-all.sh
```

Outils nécessaires : `dpkg-deb` (DEB), `rpmbuild` (RPM), `tar` et `zstd` (Arch), `apk-tools` 3 ou Docker (Alpine). Les paquets sont écrits dans `dist/`. La construction RPM peut récupérer ses outils localement sur certains systèmes Debian/Ubuntu.

### Préparer la publication des paquets

Les paquets de `dist/` conviennent aux tests et aux téléchargements directs. Les recettes Arch, Alpine et RPM téléchargent une archive source versionnée depuis une release GitHub. Créez-la avec :

```sh
./packaging/create-vendor-source.sh
```

Le fichier `dist/orbisys-1.0.0-vendor.tar.gz` doit être ajouté à la release GitHub `v1.0.0`. Publiez ensuite la recette dans le dépôt communautaire correspondant (AUR, Alpine aports ou un service RPM comme OBS), en mettant à jour les versions et sommes de contrôle à chaque version.

Pour préparer un envoi vers un PPA Ubuntu :

```sh
./packaging/create-ubuntu-source.sh
```

Les fichiers source non signés apparaissent dans `dist/ubuntu-source/`. Signez le fichier `.changes` avec votre clé Launchpad (`debsign`), puis envoyez-le à votre PPA avec `dput`. Launchpad construira les paquets binaires. Les métadonnées actuelles ciblent Ubuntu 26.04 (`resolute`) ; adaptez `debian/changelog` pour une autre version d’Ubuntu.

## Langues

L'interface est disponible en français, anglais, allemand, espagnol, portugais, italien, néerlandais, japonais, coréen, chinois et arabe. Les traductions française, anglaise et espagnole sont complètes ; les autres peuvent être partielles et les libellés manquants reviennent à l'anglais.

## Structure

- `src/main.rs` : point d'entrée et choix GUI/CLI.
- `src/lib.rs` : noyau partagé et modules système.
- `src/cli.rs` : commandes CLI.
- `src/gui.rs`, `src/gui/` : interface graphique.
- `src/p_*.rs`, `src/panels.rs` : panneaux spécialisés.
- `src/lang/` : traductions de l'application.
- `packaging/` : scripts et métadonnées des paquets.
- `assets/` : ressources graphiques ; `dist/` : paquets précompilés.

## Licence

Le projet déclare la licence MIT dans [`Cargo.toml`](Cargo.toml). Les icônes Lucide ont leur propre licence dans [`assets/icons/LICENSE`](assets/icons/LICENSE).
