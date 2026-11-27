#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="${1:-$ROOT/target/trickest-cve-source}"
COMMIT="0330ab928da94d1210b84fbe537bb328073f6114"
LIST="$ROOT/scripts/security-intel/data/trickest-kev-top30-2026-11-26.txt"
mkdir -p "$DEST"
while IFS= read -r cve; do
  [[ -n "$cve" ]] || continue
  [[ "$cve" =~ ^CVE-([0-9]{4})-[0-9]{4,19}$ ]] || { echo "invalid CVE: $cve" >&2; exit 1; }
  year="${BASH_REMATCH[1]}"
  curl --fail --location --retry 3 --retry-delay 2 --max-time 60 \
    --max-filesize 2000000 \
    --output "$DEST/$cve.md" \
    "https://raw.githubusercontent.com/trickest/cve/$COMMIT/$year/$cve.md"
done < "$LIST"
printf 'Downloaded %s Trickest/CVE metadata pages to %s\n' "$(find "$DEST" -maxdepth 1 -type f -name 'CVE-*.md' | wc -l)" "$DEST"
