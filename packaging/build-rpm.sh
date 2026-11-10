#!/usr/bin/env bash
# Construit un paquet .rpm à partir du binaire Rust déjà compilé.
#
#   cd rust/orbisys
#   cargo build --release
#   ./packaging/build-rpm.sh            # produit dist/orbisys-<version>-<release>.<arch>.rpm
#
# Si rpmbuild est absent (cas d'Ubuntu), les outils RPM sont récupérés sans
# privilège dans .cache/rpm-tools (apt-get download puis dpkg-deb -x) ; rien
# n'est installé sur le système. Sur Fedora/openSUSE, le paquet 'rpm-build'
# fournit rpmbuild et aucun téléchargement n'a lieu.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE="$(cd "$HERE/.." && pwd)"
BIN="${ORBISYS_BIN:-$CRATE/target/release/orbisys}"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$CRATE/Cargo.toml" | head -1)"
RELEASE=1
ARCH="$(uname -m)"
DIST="$CRATE/dist"

[ -x "$BIN" ] || { echo "Binaire absent : lancer 'cargo build --release' d'abord." >&2; exit 1; }
mkdir -p "$DIST"

# Récupère rpmbuild localement quand la distribution ne le fournit pas.
ensure_rpmbuild() {
  if command -v rpmbuild >/dev/null 2>&1; then
    RPMBUILD="$(command -v rpmbuild)"
    return 0
  fi
  local tools="$CRATE/.cache/rpm-tools"
  if [ ! -x "$tools/root/usr/bin/rpmbuild" ]; then
    command -v apt-get >/dev/null 2>&1 || return 1
    command -v dpkg-deb >/dev/null 2>&1 || return 1
    echo "rpmbuild absent : récupération locale des outils RPM dans .cache/rpm-tools…" >&2
    mkdir -p "$tools/debs"
    apt-get -o Debug::NoLocking=1 \
            -o Dir::Cache::archives="$tools/debs/" \
            -o Dir::Cache="$tools/cache/" \
            install --download-only -y rpm >/dev/null || return 1
    local deb
    for deb in "$tools"/debs/*.deb; do
      dpkg-deb -x "$deb" "$tools/root"
    done
  fi
  local libdir
  libdir="$(find "$tools/root/usr/lib" -maxdepth 1 -name '*-linux-gnu' | head -1)"
  export PATH="$tools/root/usr/bin:$PATH"
  export RPM_CONFIGDIR="$tools/root/usr/lib/rpm"
  [ -n "$libdir" ] && export LD_LIBRARY_PATH="$libdir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  RPMBUILD="$tools/root/usr/bin/rpmbuild"
}

RPMBUILD=""
ensure_rpmbuild || { echo "rpmbuild introuvable : installer le paquet 'rpm' (Debian/Ubuntu) ou 'rpm-build' (Fedora)." >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
SPEC="$WORK/orbisys.spec"

cat > "$SPEC" <<EOF
# Paquet binaire : le binaire est déjà compilé par cargo build --release.
%global debug_package %{nil}
%global __os_install_post %{nil}

Name:           orbisys
Version:        $VERSION
Release:        $RELEASE
Summary:        Gestionnaire de systeme Linux : paquets, disques, reseau, securite
License:        MIT
URL:            https://github.com/aelharrak/orbisys
Requires:       polkit
Requires:       libc.so.6
Requires:       libgcc_s.so.1
Requires:       libm.so.6
AutoReqProv:    no
BuildArch:      $ARCH

%description
Inventaire des logiciels, mises a jour, espace disque, nettoyage, processus,
services, journaux, materiel, reseau, pare-feu, securite, sauvegarde, noyaux,
volumes LVM/LUKS, peripheriques et bureau GNOME. Interface egui native et mode
ligne de commande dans un binaire unique.

%install
install -Dm0755 "$BIN" %{buildroot}%{_bindir}/orbisys
install -Dm0644 "$HERE/orbisys.desktop" %{buildroot}%{_datadir}/applications/orbisys.desktop
install -Dm0644 "$HERE/orbisys.svg" %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/orbisys.svg

%files
%{_bindir}/orbisys
%{_datadir}/applications/orbisys.desktop
%{_datadir}/icons/hicolor/scalable/apps/orbisys.svg

%post
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f %{_datadir}/icons/hicolor || true
fi

%postun
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f %{_datadir}/icons/hicolor || true
fi
EOF

# gzdio plutôt que zstd : lisible par les rpm les plus anciens.
"$RPMBUILD" -bb \
  --define "_topdir $WORK/rpmbuild" \
  --define "_binary_payload w9.gzdio" \
  "$SPEC" >"$WORK/rpmbuild.log" 2>&1 || {
    echo "Échec de rpmbuild :" >&2
    tail -40 "$WORK/rpmbuild.log" >&2
    exit 1
  }

find "$WORK/rpmbuild/RPMS" -name '*.rpm' -exec cp {} "$DIST/" \;
echo "Paquet créé : $(ls -1 "$DIST"/orbisys-"$VERSION"-"$RELEASE".*.rpm | tail -1)"
