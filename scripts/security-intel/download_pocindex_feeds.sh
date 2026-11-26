#!/usr/bin/env bash
set -euo pipefail
DEST="${1:-target/pocindex-source}"
BASE="https://pocindex.io"
mkdir -p "$DEST"
for name in kev.json epss.json cve_metadata.json nuclei.json; do
  curl --fail --location --retry 3 --retry-delay 2 --max-time 180 \
    --dump-header "$DEST/$name.headers" \
    --output "$DEST/$name" \
    "$BASE/$name"
done
