#!/usr/bin/env bash
# Copy shared data from AstroSideKick (the sibling app) into fittle-core.
# Files are copied byte-for-byte; edit them upstream, then re-run this script.
#
#   scripts/sync-astrosidekick-data.sh [path/to/AstroSideKick]
set -euo pipefail
SRC="${1:-../AstroSideKick}/packages/scope-profiles/src"
DEST="$(cd "$(dirname "$0")/.." && pwd)/crates/fittle-core/data"
for f in registry.json targets.json targets-extra.json targets-names.json; do
  cp "$SRC/$f" "$DEST/${f/registry/scope-profiles}"
done
REV=$(git -C "$SRC" rev-parse --short HEAD 2>/dev/null || echo unknown)
sed -i.bak "s/^Synced from AstroSideKick at commit .*/Synced from AstroSideKick at commit $REV./" "$DEST/SOURCES.md" && rm -f "$DEST/SOURCES.md.bak"
echo "synced from $SRC ($REV)"
