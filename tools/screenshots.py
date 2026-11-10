#!/usr/bin/env python3
"""Produit les captures d'écran du site (docs/assets/screens/).

Usage (depuis rust/orbisys, après `cargo build --release`) :

    python3 tools/screenshots.py                 # toutes les captures
    python3 tools/screenshots.py dashboard scan  # certaines seulement

Les processus nommés dans `ORBISYS_HIDE_PROCESSES` (par défaut `freebuff`, l'agent
qui produit les images) sont masqués dans l'application pendant la prise.

Prérequis : une session graphique avec XWayland ou X11, `xwininfo` et ImageMagick
(`import`, `convert`). L'application est lancée sous X11 (WINIT_UNIX_BACKEND=x11),
car `import` ne sait pas capturer une fenêtre Wayland native. L'écran, le thème et
la langue sont imposés par ORBISYS_PAGE, ORBISYS_THEME et
ORBISYS_LANG, sans toucher aux préférences de l'utilisateur.

Relisez chaque image avant de la publier : elle montre les processus, chemins et
noms d'utilisateur de la machine qui l'a produite.
"""
import os
import re
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
CRATE = os.path.dirname(HERE)
EXE = os.path.join(CRATE, "target", "release", "orbisys")
OUT = os.path.normpath(os.path.join(CRATE, "..", "..", "docs", "assets", "screens"))

# (fichier, écran, thème, langue, attente en secondes avant la capture)
SHOTS = [
    ("dashboard", "dashboard", "dark", "en", 9),
    ("processes", "processes", "dark", "en", 10),
    ("monitor", "monitor", "dark", "en", 14),  # quelques relevés suffisent pour tracer les courbes
    ("scan", "scan", "dark", "en", 9),
    ("health", "health", "dark", "en", 7),
    ("alerts", "alerts", "dark", "en", 8),
    ("software", "software", "dark", "en", 9),
    ("mounts", "mounts", "dark", "en", 7),
    ("disk", "disk", "dark", "en", 7),
    ("services", "services", "dark", "en", 8),
    ("cis", "cis", "dark", "en", 15),  # l'audit prend quelques secondes
    ("hardware", "hardware", "dark", "en", 9),
    ("dashboard-light", "dashboard", "light", "en", 9),
    ("dashboard-en", "dashboard", "dark", "en", 9),
    ("scan-en", "scan", "dark", "en", 9),
    ("dashboard-de", "dashboard", "dark", "de", 9),
    ("dashboard-ja", "dashboard", "dark", "ja", 9),
    ("dashboard-ar", "dashboard", "dark", "ar", 9),
    ("dashboard-zh", "dashboard", "dark", "zh", 9),
    ("scan-de", "scan", "dark", "de", 9),
    ("scan-ja", "scan", "dark", "ja", 9),
]

# Marge noire (ombre de la fenêtre) à retirer autour de la fenêtre de 1180x821.
CROP = "1180x821+28+17"

# Processus de l'environnement de capture à ne pas publier dans les images.
HIDE_PROCESSES = os.environ.get("ORBISYS_HIDE_PROCESSES", "freebuff")


def main() -> int:
    if not os.path.exists(EXE):
        print("binaire introuvable : lancez d'abord `cargo build --release`", file=sys.stderr)
        return 2
    os.makedirs(OUT, exist_ok=True)
    env_base = {k: v for k, v in os.environ.items()
                if k not in ("WAYLAND_DISPLAY", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME")}
    # Dossier personnel vide : les préférences de l'utilisateur (accent, thème) ne faussent pas les captures.
    env_base.setdefault("XAUTHORITY", os.path.expanduser("~/.Xauthority"))
    home = tempfile.mkdtemp(prefix="orbisys-shots-")
    env_base.update(HOME=home, WINIT_UNIX_BACKEND="x11", DISPLAY=os.environ.get("DISPLAY", ":0"))
    only = set(sys.argv[1:])
    failures = 0
    for name, page, theme, lang, wait in SHOTS:
        if only and name not in only:
            continue
        env = dict(
            env_base,
            ORBISYS_PAGE=page,
            ORBISYS_THEME=theme,
            ORBISYS_LANG=lang,
            ORBISYS_HIDE_PROCESSES=HIDE_PROCESSES,
        )
        process = subprocess.Popen([EXE], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            time.sleep(wait)
            info = subprocess.run(["xwininfo", "-name", "OrbiSys"], capture_output=True, text=True, env=env)
            match = re.search(r"Window id: (0x[0-9a-f]+)", info.stdout)
            raw = os.path.join(OUT, f"_{name}.png")
            if not match or subprocess.run(["import", "-window", match.group(1), raw], env=env).returncode != 0:
                print("ÉCHEC", name)
                failures += 1
                continue
            final = os.path.join(OUT, f"{name}.png")
            # La marge d'ombre varie selon la fenêtre : on la déduit de la taille capturée
            # (fenêtre de 1180 × 821 centrée horizontalement, ombre de 17 px en haut).
            size = subprocess.run(["convert", raw, "-format", "%w %h", "info:"], capture_output=True, text=True).stdout.split()
            width = int(size[0]) if size else 1236
            crop = f"1180x821+{max(0, (width - 1180) // 2)}+17"
            subprocess.run(["convert", raw, "-crop", crop, "+repage", "-strip", final], check=True)
            os.remove(raw)
            print("ok", name, os.path.getsize(final), "octets")
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
