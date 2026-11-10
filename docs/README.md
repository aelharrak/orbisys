# Site du projet (GitHub Pages)

Site statique : HTML, CSS et un peu de JavaScript, sans dépendance ni outil de
construction. Il fonctionne hors ligne.

| Fichier | Contenu |
|---|---|
| `index.html` | Présentation : fonctionnalités, captures, installation, distributions testées, FAQ |
| `utilisateurs.html` | Guide utilisateur : prise en main, tour des écrans, tâches courantes, dépannage |
| `developpeurs.html` | Architecture, ajout d'une rubrique, règles de sécurité, tests, contribution |
| `404.html` | Page d'erreur |
| `assets/style.css`, `assets/site.js` | Thème clair/sombre, onglets, visionneuse d'images |
| `assets/lang.js`, `assets/i18n/` | Sélecteur de langue et dictionnaires (11 langues, arabe de droite à gauche) ; voir `assets/i18n/README.md` |
| `tools/check-i18n.py` | Couverture des traductions du site |
| `assets/screens/` | Captures d'écran réelles de l'application |
| `parite-rust-python.md` | Comparaison mesurée Rust / Python et tests multi-distributions |

## Publier

1. Le site et le paquet Rust pointent vers `https://github.com/aelharrak/orbisys`. Si le dépôt change de compte, remplacez cette URL dans `docs/*.html` et `Cargo.toml`.

2. Poussez le dépôt, puis sur GitHub : **Settings → Pages → Build and deployment →
   Source : Deploy from a branch → Branch : `main`, dossier `/docs`**.
3. Le site est publié sur `https://aelharrak.github.io/orbisys/`.

## Prévisualiser en local

```bash
python3 -m http.server 8000 --directory docs
# puis http://localhost:8000
```

## Refaire les captures

```bash
cargo build --release
python3 tools/screenshots.py            # toutes ; ou : python3 tools/screenshots.py dashboard scan
```

Relisez chaque image avant de la publier : une capture montre les processus, chemins et
noms d'utilisateur de la machine qui l'a produite.
