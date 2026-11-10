#!/usr/bin/env bash
# Compare l'inventaire de la version Python (main.py) et du binaire Rust.
#
# Utilisation :
#   ./rust/orbisys/parity.sh            # depuis la racine du dépôt
#   ./parity.sh                                # depuis rust/orbisys
#
# Sortie : 0 si les compteurs coïncident, 1 sinon.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
BIN="$HERE/target/debug/orbisys"

[ -x "$BIN" ] || { echo "Binaire absent : lancer 'cargo build' dans $HERE" >&2; exit 2; }
[ -f "$ROOT/main.py" ] || { echo "main.py introuvable dans $ROOT" >&2; exit 2; }

extract_python() {
    ( cd "$ROOT" && timeout 180 python3 main.py inventory 2>/dev/null ) | head -3
}

python_line="$(extract_python)"
rust_line="$("$BIN" inventory 2>/dev/null | head -3)"

python_counts="$(printf '%s\n' "$python_line" | grep -c 'logiciel')"
if [ "$python_counts" -eq 0 ]; then
    echo "Impossible de lire la sortie Python (PySide6 absent ?)" >&2
    exit 2
fi

python_total="$(printf '%s\n' "$python_line" | sed -n 's/^\([0-9]\+\) logiciel(s).*/\1/p' | head -1)"
rust_total="$(printf '%s\n' "$rust_line" | sed -n 's/.*: \([0-9]\+\) paquets.*/\1/p' | head -1)"

# Vérité terrain indépendante des deux applications.
native_truth="$(dpkg-query -W -f='${db:Status-Abbrev}\n' 2>/dev/null | grep -c '^ii')"
snap_truth="$(snap list 2>/dev/null | tail -n +2 | wc -l)"
flatpak_truth="$(flatpak list --app 2>/dev/null | wc -l)"

rust_native="$(printf '%s\n' "$rust_line" | sed -n 's/.*: \([0-9]\+\) paquets, \([0-9]\+\) snap, \([0-9]\+\) flatpak, \([0-9]\+\) manuels.*/\1/p' | head -1)"
rust_snap="$(printf '%s\n' "$rust_line" | sed -n 's/.*: [0-9]\+ paquets, \([0-9]\+\) snap.*/\1/p' | head -1)"
rust_flatpak="$(printf '%s\n' "$rust_line" | sed -n 's/.*, \([0-9]\+\) flatpak,.*/\1/p' | head -1)"
rust_manual="$(printf '%s\n' "$rust_line" | sed -n 's/.*, \([0-9]\+\) manuels.*/\1/p' | head -1)"
rust_all=$(( ${rust_native:-0} + ${rust_snap:-0} + ${rust_flatpak:-0} + ${rust_manual:-0} ))

echo "Python : $python_line"
echo "Rust   : $rust_line"
echo ""
printf '%-18s %10s %10s %10s\n' "source" "python" "rust" "terrain"
printf '%-18s %10s %10s %10s\n' "paquets natifs" "-" "${rust_native:-?}" "$native_truth"
printf '%-18s %10s %10s %10s\n' "snap" "-" "${rust_snap:-?}" "$snap_truth"
printf '%-18s %10s %10s %10s\n' "flatpak" "-" "${rust_flatpak:-?}" "$flatpak_truth"
printf '%-18s %10s %10s %10s\n' "hors dépôts" "-" "${rust_manual:-?}" "-"
printf '%-18s %10s %10s %10s\n' "total" "${python_total:-?}" "$rust_all" "-"

fail=0
[ "${rust_native:-x}" = "$native_truth" ] || { echo "ÉCART : paquets natifs"; fail=1; }
[ "${rust_snap:-x}" = "$snap_truth" ] || { echo "ÉCART : snap"; fail=1; }
[ "${rust_flatpak:-x}" = "$flatpak_truth" ] || { echo "ÉCART : flatpak"; fail=1; }
[ "${python_total:-x}" = "$rust_all" ] || { echo "ÉCART : total Python ($python_total) ≠ total Rust ($rust_all)"; fail=1; }

if [ "$fail" -ne 0 ]; then
    echo ""
    echo "Parité rompue : corriger avant de remplacer la version Python."
    exit 1
fi

echo ""
echo "Parité OK sur les compteurs (les noms des entrées .desktop peuvent différer)."
