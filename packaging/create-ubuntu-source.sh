#!/usr/bin/env bash
# Build an unsigned Ubuntu PPA source upload from a vendor source archive.
# Usage: packaging/create-ubuntu-source.sh [SOURCE_ARCHIVE.tar.gz] [OUTPUT_DIR]

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
ARCHIVE="${1:-$ROOT/dist/orbisys-$VERSION-vendor.tar.gz}"
OUT="${2:-$ROOT/dist/ubuntu-source}"

[ -s "$ARCHIVE" ] || {
  echo "Source archive absent. Run packaging/create-vendor-source.sh first." >&2
  exit 1
}

WORK="$(mktemp -d "${TMPDIR:-/tmp}/orbisys-ubuntu-source.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
cp "$ARCHIVE" "$WORK/orbisys_${VERSION}.orig.tar.gz"
tar -xzf "$WORK/orbisys_${VERSION}.orig.tar.gz" -C "$WORK"
cp -a "$ROOT/debian" "$WORK/orbisys-$VERSION/debian"

(cd "$WORK/orbisys-$VERSION" && dpkg-buildpackage -S -us -uc -d)
mkdir -p "$OUT"
cp "$WORK"/orbisys_* "$OUT/"
echo "Unsigned Ubuntu source package created in: $OUT"
echo "Before upload: sign the .changes file with debsign, then upload it with dput."
