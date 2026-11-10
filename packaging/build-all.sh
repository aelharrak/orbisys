#!/usr/bin/env bash
# Construit tous les paquets possibles sur la machine courante.
#
#   cargo build --release
#   ./packaging/build-all.sh            # .deb, .rpm, Arch, Alpine dans dist/
#
# Un format est sauté (avec un message) quand son outil ou son binaire manque :
# .deb sur Debian/Ubuntu, .rpm partout (rpmbuild récupéré localement si absent),
# Arch avec zstd, Alpine uniquement si le binaire musl a été compilé.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE="$(cd "$HERE/.." && pwd)"
MUSL="$CRATE/target/x86_64-unknown-linux-musl/release/orbisys"
[ "$(uname -m)" = "aarch64" ] && MUSL="$CRATE/target/aarch64-unknown-linux-musl/release/orbisys"

run() {
  local script=$1
  echo "== $script"
  if ! "$HERE/$script"; then
    echo "   échec : $script" >&2
    return 1
  fi
}

failures=0
run build-deb.sh || failures=$((failures + 1))
run build-rpm.sh || failures=$((failures + 1))
run build-arch.sh || failures=$((failures + 1))
if [ -x "$MUSL" ]; then
  run build-apk.sh || failures=$((failures + 1))
else
  echo "== build-apk.sh sauté (binaire musl absent : cargo build --release --target x86_64-unknown-linux-musl)"
fi

echo
if [ "$failures" -eq 0 ]; then
  echo "Paquets dans $CRATE/dist :"
  ls -1sh "$CRATE/dist"
else
  echo "$failures format(s) en échec." >&2
fi
exit $([ "$failures" -eq 0 ] && echo 0 || echo 1)
