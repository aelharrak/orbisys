#!/usr/bin/env bash
# Construit un paquet .deb à partir du binaire Rust déjà compilé.
#
#   cd rust/orbisys
#   cargo build --release
#   ./packaging/build-deb.sh            # produit dist/orbisys_<version>_<arch>.deb
#
# Dépendances : dpkg-deb (Debian/Ubuntu). Aucun privilège requis.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE="$(cd "$HERE/.." && pwd)"
BIN="${ORBISYS_BIN:-$CRATE/target/release/orbisys}"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$CRATE/Cargo.toml" | head -1)"
ARCH="$(dpkg --print-architecture 2>/dev/null || echo amd64)"
DIST="$CRATE/dist"
OUT="$DIST/orbisys_${VERSION}_${ARCH}.deb"

[ -x "$BIN" ] || { echo "Binaire absent : lancer 'cargo build --release' d'abord." >&2; exit 1; }
command -v dpkg-deb >/dev/null || { echo "dpkg-deb est requis." >&2; exit 1; }
mkdir -p "$DIST"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

# Version minimale de glibc réellement exigée par le binaire (symboles versionnés).
GLIBC_FLOOR=""
if command -v objdump >/dev/null 2>&1; then
  GLIBC_FLOOR="$(objdump -T "$BIN" 2>/dev/null | grep -o 'GLIBC_[0-9][0-9.]*' \
    | sed 's/^GLIBC_//' | sort -Vu | tail -1)"
fi
LIBC_DEP="libc6"
[ -n "$GLIBC_FLOOR" ] && LIBC_DEP="libc6 (>= $GLIBC_FLOOR)"

mkdir -p "$STAGE/DEBIAN" \
         "$STAGE/usr/bin" \
         "$STAGE/usr/share/applications" \
         "$STAGE/usr/share/icons/hicolor/scalable/apps"

chmod 0755 "$STAGE"
install -m 0755 "$BIN" "$STAGE/usr/bin/orbisys"
install -m 0644 "$HERE/orbisys.desktop" "$STAGE/usr/share/applications/orbisys.desktop"
install -m 0644 "$HERE/orbisys.svg" "$STAGE/usr/share/icons/hicolor/scalable/apps/orbisys.svg"

cat > "$STAGE/DEBIAN/control" <<EOF
Package: orbisys
Version: $VERSION
Section: admin
Priority: optional
Architecture: $ARCH
Maintainer: OrbiSys <noreply@example.com>
Homepage: https://github.com/aelharrak/orbisys
Installed-Size: $(du -sk "$STAGE/usr" | cut -f1)
Depends: $LIBC_DEP, libgcc-s1
Recommends: polkitd | policykit-1
Description: Gestionnaire de systeme pour Linux
 Inventaire des logiciels, mises a jour, espace disque, nettoyage,
 processus, services, journaux, materiel, reseau, pare-feu, securite,
 sauvegarde, noyaux, volumes LVM/LUKS, peripheriques et bureau GNOME.
EOF

# Rafraîchit les caches du menu et des icônes après installation ou retrait.
cat > "$STAGE/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi
EOF
cat > "$STAGE/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi
EOF
chmod 0755 "$STAGE/DEBIAN/postinst" "$STAGE/DEBIAN/postrm"

dpkg-deb --build --root-owner-group "$STAGE" "$OUT" >/dev/null
echo "Paquet créé : $OUT"
