#!/usr/bin/env bash
# Teste le binaire sur plusieurs distributions, dans des conteneurs Docker.
#
#   ./distro-test.sh                      # images par défaut
#   ./distro-test.sh fedora:latest        # images au choix
#
# Le binaire est compilé en statique (musl) pour tourner partout, Alpine
# comprise. Ce test couvre la ligne de commande (inventaire, mises à jour, toutes
# les rubriques via `panel`), pas l'interface graphique : un conteneur n'a ni
# systemd, ni écran, ni matériel. Code de sortie : 0 = aucun plantage.
set -u
cd "$(dirname "$0")"
TARGET=x86_64-unknown-linux-musl
command -v docker >/dev/null || { echo "docker est requis" >&2; exit 2; }
rustup target list --installed 2>/dev/null | grep -q "$TARGET" || rustup target add "$TARGET" || exit 2
cargo build --release --target "$TARGET" >/dev/null 2>&1 || { echo "échec de compilation" >&2; exit 2; }
BIN="$PWD/target/$TARGET/release/orbisys"
IMAGES=("$@")
[ ${#IMAGES[@]} -eq 0 ] && IMAGES=(debian:bookworm debian:trixie ubuntu:24.04 archlinux:latest fedora:latest opensuse/tumbleweed:latest alpine:latest)
failures=0
run() { local image=$1; shift; timeout 300 docker run --rm -v "$BIN:/um:ro" -e HOME=/root "$image" /um "$@"; }
for image in "${IMAGES[@]}"; do
  echo "== $image"
  if ! out=$(run "$image" inventory 2>&1); then echo "   inventory : ÉCHEC"; failures=$((failures+1)); continue; fi
  echo "$out" | grep -E '^(Distribution|Gestionnaire|Logiciels)' | sed 's/^/   /'
  run "$image" updates 2>&1 | head -1 | sed 's/^/   /'
  bad=0; total=0
  for panel in $(run "$image" panel); do
    total=$((total+1))
    if ! run "$image" panel "$panel" >/dev/null 2>/tmp/um-distro-err; then
      echo "   rubrique $panel : ÉCHEC ($(head -c 120 /tmp/um-distro-err))"; bad=$((bad+1))
    elif grep -q panicked /tmp/um-distro-err; then
      echo "   rubrique $panel : PANIC"; bad=$((bad+1))
    fi
  done
  echo "   rubriques : $((total-bad))/$total sans plantage"
  failures=$((failures+bad))
done
rm -f /tmp/um-distro-err
[ "$failures" -eq 0 ] && echo "OK : aucun plantage" || echo "ÉCHECS : $failures"
exit $([ "$failures" -eq 0 ] && echo 0 || echo 1)
