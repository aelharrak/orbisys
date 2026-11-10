#!/usr/bin/env bash
# Create the source archive used by downstream distro builds.
# Usage: packaging/create-vendor-source.sh [OUTPUT.tar.gz]

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
OUT="${1:-$ROOT/dist/orbisys-$VERSION-vendor.tar.gz}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/orbisys-vendor.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

SOURCE="$WORK/orbisys-$VERSION"
mkdir -p "$SOURCE/.cargo" "$(dirname "$OUT")"
for item in Cargo.toml Cargo.lock README.md src assets; do
  cp -a "$ROOT/$item" "$SOURCE/"
done
mkdir -p "$SOURCE/packaging"
cp -a "$ROOT/packaging/orbisys.desktop" "$ROOT/packaging/orbisys.svg" "$SOURCE/packaging/"

# Build systems for distro packages may not access the network. Vendor every
# locked crate and rewrite Cargo's generated absolute path to a portable one.
TASK_CARGO_HOME="${CARGO_HOME:-$WORK/cargo-home}"
if ! CARGO_HOME="$TASK_CARGO_HOME" cargo vendor --locked --manifest-path "$ROOT/Cargo.toml" "$WORK/vendor" \
  > "$SOURCE/.cargo/config.toml" 2> "$WORK/vendor.log"; then
  tail -40 "$WORK/vendor.log" >&2
  exit 1
fi
sed -i -E 's#directory = ".*"#directory = "vendor"#' "$SOURCE/.cargo/config.toml"
cp -a "$WORK/vendor" "$SOURCE/vendor"

# Stable ordering, timestamps and gzip headers make the archive reproducible.
tar --sort=name --mtime='@0' --owner=0 --group=0 --numeric-owner \
  -cf - -C "$WORK" "orbisys-$VERSION" | gzip -n -9 > "$OUT"
echo "Source archive created: $OUT"
