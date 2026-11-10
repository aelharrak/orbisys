#!/usr/bin/env bash
# Construit un paquet Alpine (.apk) à partir du binaire musl déjà compilé.
#
#   cd rust/orbisys
#   rustup target add x86_64-unknown-linux-musl   # une fois
#   cargo build --release --target x86_64-unknown-linux-musl
#   ./packaging/build-apk.sh            # produit dist/orbisys-<version>-r0.apk
#
# Le paquet est au format apk v3, écrit par « apk mkpkg » : si apk-tools est
# présent (Alpine) il est utilisé directement, sinon la construction passe par
# un conteneur alpine:latest (docker). Paquet non signé : installer avec
# « apk add --allow-untrusted » (apk-tools 3), ou rebâtir depuis l'APKBUILD
# pour un paquet signé par abuild.
#
# Le binaire musl est lié statiquement : aucune dépendance de bibliothèque.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE="$(cd "$HERE/.." && pwd)"
TARGET="x86_64-unknown-linux-musl"
[ "$(uname -m)" = "aarch64" ] && TARGET="aarch64-unknown-linux-musl"
BIN="${ORBISYS_BIN:-$CRATE/target/$TARGET/release/orbisys}"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$CRATE/Cargo.toml" | head -1)"
PKGREL=0
ARCH="$(uname -m)"
DIST="$CRATE/dist"
OUT="$DIST/orbisys-${VERSION}-r${PKGREL}.apk"

[ -x "$BIN" ] || {
  echo "Binaire musl absent : lancer 'cargo build --release --target $TARGET' d'abord." >&2
  exit 1
}
mkdir -p "$DIST" "$CRATE/.cache"
STAGE="$(mktemp -d "$CRATE/.cache/apk-stage.XXXXXX")"
trap 'rm -rf "$STAGE"' EXIT

install -Dm0755 "$BIN" "$STAGE/root/usr/bin/orbisys"
install -Dm0644 "$HERE/orbisys.desktop" "$STAGE/root/usr/share/applications/orbisys.desktop"
install -Dm0644 "$HERE/orbisys.svg" "$STAGE/root/usr/share/icons/hicolor/scalable/apps/orbisys.svg"

INFO=(
  --info "name:orbisys"
  --info "version:${VERSION}-r${PKGREL}"
  --info "description:Gestionnaire de systeme : paquets, disques, reseau, securite"
  --info "url:https://github.com/aelharrak/orbisys"
  --info "arch:$ARCH"
  --info "license:MIT"
  --info "origin:orbisys"
  --info "maintainer:OrbiSys <noreply@example.com>"
  --info "depends:polkit"
)

rm -f "$OUT"
if command -v apk >/dev/null 2>&1 && apk mkpkg --help >/dev/null 2>&1; then
  apk mkpkg -F "$STAGE/root" "${INFO[@]}" -o "$OUT"
elif command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  echo "apk-tools absent : construction du .apk dans un conteneur alpine…" >&2
  docker run --rm -u "$(id -u):$(id -g)" -v "$CRATE:/w" -w /w alpine:latest \
    apk mkpkg -F "${STAGE#"$CRATE"/}/root" "${INFO[@]}" -o "${OUT#"$CRATE"/}"
else
  echo "apk mkpkg (apk-tools 3) ou docker est requis pour construire le paquet Alpine." >&2
  exit 1
fi

[ -s "$OUT" ] || { echo "Le paquet n'a pas été produit : $OUT" >&2; exit 1; }
echo "Paquet créé : $OUT"
