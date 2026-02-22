#!/bin/bash
# =============================================================================
# Superkonna Theme — Canvas Asset Fetcher
# =============================================================================
# Downloads per-system assets from the Canvas ES theme (Siddy212/canvas-es)
# directly into this theme's asset directories.
#
# WARNING: This modifies tracked files! After fetching, git will show
# local changes. That's intentional — these are local enhancements.
# Re-run this script after git pull if needed.
#
# Downloads:
#   - 306 system backgrounds (webp) → assets/systems/
#   - 322 system logos (svg)         → assets/logos/
#
# Usage:
#   ./fetch-canvas-assets.sh [--theme-dir /path] [--force]
# =============================================================================

set -euo pipefail

THEME_DIR="$(cd "$(dirname "$0")/.." && pwd)"
FORCE=false
CANVAS_REPO="Siddy212/canvas-es"
CANVAS_BRANCH="master"
RAW_BASE="https://raw.githubusercontent.com/${CANVAS_REPO}/${CANVAS_BRANCH}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --theme-dir) THEME_DIR="$2"; shift 2 ;;
    --force) FORCE=true; shift ;;
    *) shift ;;
  esac
done

fetch_asset_set() {
  local gh_path="$1"
  local dest_dir="$2"
  local ext="$3"
  local label="$4"

  mkdir -p "${dest_dir}"
  echo "[superkonna] ── ${label} ──"

  local file_list
  file_list=$(curl -sL "https://api.github.com/repos/${CANVAS_REPO}/contents/${gh_path}?ref=${CANVAS_BRANCH}" \
    | python3 -c "
import json, sys
data = json.load(sys.stdin)
if isinstance(data, list):
    for item in data:
        name = item.get('name', '')
        if name.endswith('.${ext}') and not name.startswith('_'):
            print(name)
" 2>/dev/null || true)

  if [[ -z "${file_list}" ]]; then
    echo "[superkonna] Warning: Could not fetch list. Skipping."
    return
  fi

  local total
  total=$(echo "${file_list}" | wc -l | tr -d ' ')
  echo "[superkonna] Found ${total} files."

  local downloaded=0 skipped=0 failed=0

  while IFS= read -r filename; do
    [[ -z "${filename}" ]] && continue
    local dest="${dest_dir}/${filename}"

    if [[ -f "${dest}" ]] && [[ "${FORCE}" != "true" ]]; then
      skipped=$((skipped + 1))
      continue
    fi

    if curl -sL -o "${dest}" "${RAW_BASE}/${gh_path}/${filename}" 2>/dev/null; then
      local size
      size=$(wc -c < "${dest}" 2>/dev/null || echo 0)
      if (( size < 100 )); then
        rm -f "${dest}"
        failed=$((failed + 1))
      else
        downloaded=$((downloaded + 1))
      fi
    else
      failed=$((failed + 1))
    fi

    local done_count=$((downloaded + skipped + failed))
    if (( done_count % 50 == 0 )) && (( done_count > 0 )); then
      echo "[superkonna]   ${done_count}/${total} (${downloaded} new)..."
      sleep 0.5
    fi
  done <<< "${file_list}"

  echo "[superkonna] ${label}: ${downloaded} downloaded, ${skipped} skipped, ${failed} failed"
}

echo "[superkonna] Canvas Asset Fetcher"
echo "[superkonna] Downloading into ${THEME_DIR}/assets/"
echo ""

fetch_asset_set "_inc/systems/carousel-icons-art" "${THEME_DIR}/assets/systems" "webp" "System Backgrounds (306 webp)"
echo ""
fetch_asset_set "_inc/systems/logos" "${THEME_DIR}/assets/logos" "svg" "System Logos (322 svg)"

echo ""
echo "[superkonna] Done. Re-run after git pull to restore enhancements."
