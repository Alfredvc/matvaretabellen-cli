#!/usr/bin/env bash
# Refresh the embedded dataset from upstream matvaretabellen.no.
# Writes into data/ and data/VERSION. Fails loud on any HTTP error.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASE="https://www.matvaretabellen.no/api"
DATA="$ROOT/data"

mkdir -p "$DATA/nb" "$DATA/en"

fetch() {
  local url="$1" out="$2"
  echo "fetch $url"
  curl -fSL --retry 3 "$url" -o "$out"
}

for loc in nb en; do
  for res in foods food-groups nutrients sources rda; do
    fetch "$BASE/$loc/$res.json" "$DATA/$loc/$res.json"
  done
done
fetch "$BASE/langual.json" "$DATA/langual.json"

# Capture upstream Last-Modified of the full foods.nb endpoint as the canonical
# data version. This matches the Mattilsynet release cadence.
LM=$(curl -fsI "$BASE/nb/foods.json" | awk -F': ' 'tolower($1)=="last-modified" {print $2}' | tr -d '\r\n')
if [ -z "$LM" ]; then
  echo "warning: no Last-Modified header; falling back to today" >&2
  LM=$(date -u +%Y-%m-%d)
else
  # Convert "Sun, 12 Apr 2026 04:08:02 GMT" -> "2026-04-12"
  LM=$(date -u -j -f "%a, %d %b %Y %H:%M:%S GMT" "$LM" +%Y-%m-%d 2>/dev/null \
       || date -u -d "$LM" +%Y-%m-%d 2>/dev/null \
       || date -u +%Y-%m-%d)
fi
echo "$LM" > "$DATA/VERSION"
echo "wrote data/VERSION = $LM"
