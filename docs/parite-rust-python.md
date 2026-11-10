# Parité Rust / Python — mesures du 2 novembre 2026

Comparaison des deux implémentations sur la même machine (Ubuntu 26.10, APT/dpkg,
39 snap, 26 flatpak), avec les mêmes données système.

## Méthode

```bash
# Version Python
python3 main.py inventory     # en-tête + liste
python3 main.py updates

# Version Rust
./target/debug/orbisys inventory
./target/debug/orbisys updates
./target/debug/orbisys inventory --json /tmp/rs.json

# Vérité terrain, indépendante des deux applications
dpkg-query -W -f='${db:Status-Abbrev}\n' | sort | uniq -c
snap list | tail -n +2 | wc -l
flatpak list --app | wc -l
df -B1 -x tmpfs -x devtmpfs -x squashfs -x overlay --output=target | tail -n +2 | wc -l
```

## Résultats

| Mesure | Python | Rust | Vérité terrain | Écart |
|---|---|---|---|---|
| Paquets APT installés | 3 068 | 3 068 | 3 068 (`ii`) | **0** |
| Snap | 39 | 39 | 39 | **0** |
| Flatpak | 26 | 26 | 26 | **0** |
| Logiciels hors dépôts | 61 | 61 | — | **0** (noms différents, voir ci-dessous) |
| Mises à jour disponibles | 0 | 0 | 0 | **0** |
| Systèmes de fichiers montés | — | 3 | 3 (`df`) | **0** |

## Bug trouvé et corrigé grâce à cette comparaison

La première mesure donnait **3 373** paquets côté Rust contre 3 068 côté Python.
Cause : `dpkg-query -W` liste aussi les paquets désinstallés dont la configuration
subsiste (état `rc`) — 305 entrées sur cette machine. La version Python filtrait
déjà sur l'état installé.

Correction dans `src/pkg.rs` : le format de requête inclut désormais
`${db:Status-Abbrev}` et `parse_dpkg_query` ne retient que les états commençant
par `ii`. Un test unitaire couvre le cas (`parses_dpkg_query_output_and_skips_removed_packages`).
Le nombre est depuis identique à `dpkg` et à la version Python.

C'est exactement ce que la comparaison devait apporter : sans elle, l'écran
d'inventaire Rust aurait affiché 10 % de logiciels fantômes.

## Écarts connus et assumés

1. **Nom des entrées `.desktop`** : Rust utilise le binaire lancé (`antigravity`,
   `zed-editor`), Python parfois le nom affiché (`Antigravity IDE`, `Zed`).
   Les ensembles se recouvrent à 53/61 ; les 8 différences restantes sont des
   variantes de nom du même logiciel, pas des logiciels manquants.
2. **Taille des Snap** : inconnue dans les deux cas (0).
3. **Rapports PDF** : texte simple (Helvetica) côté Rust, mise en page Qt côté
   Python.
4. **Nombre d'écrans** : 77 côté Rust (39 écrans dédiés + 38 rubriques génériques, regroupés en 39 fenêtres à onglets) pour 71 rubriques côté Python (`NAV_GROUPS`, 9 groupes). Toutes les rubriques Python ont un équivalent ; plusieurs rubriques Python voisines sont regroupées dans un même écran Rust (voir la table ci-dessous).
5. **Langue** : navigation, titres, conseils et environ 350 textes statiques des rubriques génériques sont traduits (fr/en/es) ; seules les valeurs variables (nombres, noms) restent en français ; notes, confirmations et messages d'absence d'outil sont traduits.
6. **Vérification visuelle** : le rendu graphique n'a pas été contrôlé à l'œil ; chaque rubrique a été ouverte dans l'application sans erreur (`ORBISYS_PAGE=<nom>`) et son contenu comparé au système via `orbisys panel <nom>`.

## Tests sur d'autres distributions

Jusqu'ici tout était exécuté sur Ubuntu 26.10. Le binaire a ensuite été compilé en
statique (musl) et lancé dans des conteneurs Docker : `version`, `inventory`,
`updates` et chacune des 39 rubriques génériques via `panel`.

| Distribution | Gestionnaire détecté | Inventaire | Mises à jour | Rubriques sans plantage |
|---|---|---|---|---|
| Debian 12 (bookworm) | APT / dpkg | 88 paquets | 0 | 39 / 39 |
| Debian 13 (trixie) | APT / dpkg | 78 paquets | 0 | 39 / 39 |
| Ubuntu 24.04 | APT / dpkg | 92 paquets | 0 | 39 / 39 |
| Arch Linux | Pacman | 137 paquets | 0 | 39 / 39 |
| Fedora 44 | DNF / RPM | 147 paquets | 41 | 39 / 39 |
| openSUSE Tumbleweed | Zypper | 123 paquets | 1 | 39 / 39 |
| Alpine 3.24 | APK | 16 paquets | 0 | 39 / 39 |

Le test a corrigé des rubriques trop liées à Debian : intégrité (`rpm -Va` sur
Fedora/openSUSE), pare-feu de la santé (firewalld), outils à double usage du scan
(rpm, pacman, apk), profils et paquets avancés (limités à APT, avec un message clair).
Il a aussi confirmé que le scan des comportements anormaux repère un faux mineur
lancé depuis `/tmp` sur Fedora.

Limites : le test couvre la ligne de commande, pas l'interface graphique (un
conteneur n'a ni écran, ni systemd, ni matériel) ; Wi-Fi, Bluetooth, firmware,
capteurs, Docker/Podman et Btrfs n'y sont pas exercés (outils absents). Restent
propres à Debian/Ubuntu : dépôts APT, verrouillage de version, historique dpkg,
installation des profils ; l'intégrité d'Arch (`pacman -Qkk`) et d'Alpine
(`apk verify`) n'est pas portée. Le paquet `.deb` n'a été installé que sur Ubuntu ;
les `PKGBUILD`, `spec` RPM et `APKBUILD` n'ont jamais été construits.

Reproduire : `./distro-test.sh [image…]`.

## Second bug trouvé pendant la migration

`disk::mounts()` reconnaissait l'en-tête de `df` par le mot anglais « Filesystem ».
Sur un système en français l'en-tête est « Sys. de fichiers » : la lecture
retombait sur le repli POSIX, qui liste tout (tmpfs, boucles Snap…), soit
**75 montages** au lieu de **3**. Le critère est maintenant le nombre de lignes,
indépendant de la langue ; `df`, l'écran Espace disque et la rubrique Montages
donnent les mêmes 3 systèmes de fichiers.

## Correspondance des rubriques

| Groupe Python | Rubriques Python → écran Rust |
|---|---|
| Surveillance & rapports | Accueil → Tableau de bord · Processus → Processus · Surveillance → Surveillance en direct · Alertes → Alertes · Santé → Santé du système · Rapports → Rapports · Historique → Historique & tendances · Envoi → Envoi des rapports |
| Logiciels | Désinstallation → Inventaire & désinstallation · Mises à jour · Dépôts APT · Paquets avancés · Runtimes & langages · Conteneurs · Conteneurs avancés |
| Système & services | Services · Démarrage · Journaux système → Journaux / Journaux avancés · Journal → Journal des actions · Paramètres noyau et Swap & zram → Système & noyaux · Benchmark · Maintenance |
| Stockage & disques | Espace disque · Nettoyage · Nettoyage avancé · Instantanés & disques → Instantanés & archives · Partitions & volumes → Stockage / Volumes · Btrfs · Montages · Doublons · Clé USB bootable · Archives |
| Réseau & partage | Réseau · Trafic réseau · DNS & hôtes → Réseau avancé · Réseau local · Serveur SSH et Partage Samba/NFS → Réseau & partage / Montages réseau |
| Sécurité | Sécurité · Pare-feu & VPN → Pare-feu · Audit sécurité → Audit CIS · Clés SSH & GPG · Certificats & CA · Secure Boot · AppArmor / SELinux · Intégrité · ACL & permissions |
| Matériel | Matériel · Pilotes & noyaux → Système & noyaux · Bluetooth & USB · Énergie & batterie · Ventilateurs & capteurs · Imprimantes · Audio → Imprimantes & audio |
| Sauvegarde & récupération | Sauvegarde · Sauvegarde fichiers → Sauvegarde & récupération · Sauvegarde cloud · Dotfiles et GRUB & noyaux → Sauvegarde & récupération · Dépannage |
| Automatisation & réglages | Utilisateurs · Tâches planifiées et Tâches cron → Automatisation · Automatisation (règles) → Alertes · Profils · Optimisation · Extensions et Raccourcis GNOME → Bureau GNOME · Polices · À propos |

## Reproduire

Le script `parity.sh` reproduit cette comparaison et renvoie
un code non nul dès qu'un compteur diverge de la vérité terrain :

```bash
./parity.sh        # 0 = parité, 1 = écart, 2 = prérequis manquant
```

Pour un contrôle automatique du cœur :

```bash
cargo test          # 347 tests : parsers, validation des entrées, sécurité, /proc, stockage
```

Les tests de parité ne sont pas automatisés dans `cargo test` car ils dépendent
des paquets installés sur la machine ; le contrôle se fait par comparaison des
sorties, comme décrit ici.
