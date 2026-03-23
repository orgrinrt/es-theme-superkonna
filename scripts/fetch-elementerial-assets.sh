#!/bin/bash
# =============================================================================
# Superkonna Theme — Elementerial Asset Fetcher
# =============================================================================
# Downloads system logos from the Elementerial ES theme (mluizvitor).
# Strips baked-in card backgrounds (480x320 rect) programmatically,
# leaving only the logo artwork on transparent background.
#
# Only fills gaps — never overwrites existing logos (higher-priority
# sources like saalis and Canvas run first).
#
# Usage:
#   ./fetch-elementerial-assets.sh [--theme-dir /path] [--force]
# =============================================================================

set -euo pipefail

THEME_DIR="$(cd "$(dirname "$0")/.." && pwd)"
FORCE=false
REPO="mluizvitor/es-theme-elementerial"
BRANCH="main"
RAW_BASE="https://raw.githubusercontent.com/${REPO}/${BRANCH}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --theme-dir) THEME_DIR="$2"; shift 2 ;;
    --force) FORCE=true; shift ;;
    *) shift ;;
  esac
done

LOGO_DST="${THEME_DIR}/assets/logos"
mkdir -p "$LOGO_DST"

echo "[superkonna] Elementerial Logo Fetcher"

# Get file list from GitHub API
file_list=$(curl -sL "https://api.github.com/repos/${REPO}/contents/assets/logos?ref=${BRANCH}" \
  | python3 -c "
import json, sys
data = json.load(sys.stdin)
if isinstance(data, list):
    for item in data:
        name = item.get('name', '')
        if name.endswith('.svg') and not name.startswith('_'):
            print(name)
" 2>/dev/null || true)

if [[ -z "${file_list}" ]]; then
  echo "[superkonna] Warning: Could not fetch file list. Skipping."
  exit 0
fi

total=$(echo "${file_list}" | wc -l | tr -d ' ')
echo "[superkonna] Found ${total} logos in Elementerial."

downloaded=0
skipped=0
failed=0

while IFS= read -r filename; do
  [[ -z "${filename}" ]] && continue
  dest="${LOGO_DST}/${filename}"

  # Skip if already exists (higher-priority source provided it)
  if [[ -f "${dest}" ]] && [[ "${FORCE}" != "true" ]]; then
    skipped=$((skipped + 1))
    continue
  fi

  # Download to temp file
  tmp="${dest}.tmp"
  if ! curl -sL -o "$tmp" "${RAW_BASE}/assets/logos/${filename}" 2>/dev/null; then
    rm -f "$tmp"
    failed=$((failed + 1))
    continue
  fi

  # Verify it's a real file (not a 404 HTML page)
  size=$(wc -c < "$tmp" 2>/dev/null || echo 0)
  if (( size < 100 )); then
    rm -f "$tmp"
    failed=$((failed + 1))
    continue
  fi

  # Strip baked-in card rect (480x320 with ry=24 and gradient defs)
  # Elementerial SVGs have: <defs>..gradient..</defs><rect width="480" height="320" ry="24" .../>
  # Remove both, leaving only the logo paths on transparent background
  python3 -c "
import re, sys
with open('$tmp', 'r') as f:
    content = f.read()
# Remove <defs>...</defs> (gradient definitions for card rect)
content = re.sub(r'<defs>.*?</defs>', '', content, flags=re.DOTALL)
# Remove the card background rect
content = re.sub(r'<rect\s+width=\"480\"\s+height=\"320\"\s+ry=\"24\"[^/]*/>', '', content)
# Clean up whitespace
content = re.sub(r'\s+', ' ', content).strip()
with open('$tmp', 'w') as f:
    f.write(content)
" 2>/dev/null

  mv "$tmp" "$dest"
  downloaded=$((downloaded + 1))

  done_count=$((downloaded + skipped + failed))
  if (( done_count % 50 == 0 )) && (( done_count > 0 )); then
    echo "[superkonna]   ${done_count}/${total} (${downloaded} new)..."
    sleep 0.5
  fi
done <<< "${file_list}"

echo "[superkonna] Elementerial logos: ${downloaded} downloaded (stripped), ${skipped} skipped, ${failed} failed"
