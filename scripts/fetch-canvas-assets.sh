#!/bin/bash
# =============================================================================
# Superkonna Theme — Canvas Asset Fetcher
# =============================================================================
# Downloads per-system assets from the Canvas ES theme (Siddy212/canvas-es)
# for local use. NOT shipped in the repo due to licensing ambiguity
# (CC0 in LICENSE file vs CC BY-NC-SA 2.0 in README).
#
# Downloads three asset sets:
#   1. System backgrounds (306 webp) → assets/systems/
#   2. System icons (310 webp)       → assets/systems-icons/
#   3. System logos (322 svg)         → assets/logos/
#
# Logos (SVG) supplement/replace the shipped Elementerial logos.
# Backgrounds supplement/replace the shipped Elementerial backgrounds.
# Icons are a bonus set not used by default but available for customization.
#
# Usage:
#   ./fetch-canvas-assets.sh [--theme-dir /path] [--force] [--only backgrounds|icons|logos]
#
# Requires: curl, python3, internet connection
# =============================================================================

set -euo pipefail

THEME_DIR="$(cd "$(dirname "$0")/.." && pwd)"
FORCE=false
ONLY=""
CANVAS_REPO="Siddy212/canvas-es"
CANVAS_BRANCH="master"
RAW_BASE="https://raw.githubusercontent.com/${CANVAS_REPO}/${CANVAS_BRANCH}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --theme-dir) THEME_DIR="$2"; shift 2 ;;
    --force) FORCE=true; shift ;;
    --only) ONLY="$2"; shift 2 ;;
    *) shift ;;
  esac
done

SYSTEMS_DIR="${THEME_DIR}/assets/systems"
ICONS_DIR="${THEME_DIR}/assets/systems-icons"
LOGOS_DIR="${THEME_DIR}/assets/logos"
MARKER="${THEME_DIR}/.canvas-fetched"

# Check marker unless forcing
if [[ -f "${MARKER}" ]] && [[ "${FORCE}" != "true" ]]; then
  echo "[superkonna] Canvas assets already fetched ($(cat "${MARKER}")). Use --force to re-download."
  exit 0
fi

# ─────────────────────────────────────────────────────
# Helper: fetch a directory of files from Canvas repo
# Args: $1=github_path $2=local_dest_dir $3=extension $4=label
# ─────────────────────────────────────────────────────
fetch_asset_set() {
  local gh_path="$1"
  local dest_dir="$2"
  local ext="$3"
  local label="$4"

  mkdir -p "${dest_dir}"

  echo ""
  echo "[superkonna] ── ${label} ──"
  echo "[superkonna] Fetching file list from ${gh_path}..."

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
    echo "[superkonna] Warning: Could not fetch list for ${label}. Skipping."
    return
  fi

  local total
  total=$(echo "${file_list}" | wc -l | tr -d ' ')
  echo "[superkonna] Found ${total} files."

  local downloaded=0
  local skipped=0
  local failed=0

  while IFS= read -r filename; do
    [[ -z "${filename}" ]] && continue
    local dest="${dest_dir}/${filename}"

    # Skip if exists and not forcing
    if [[ -f "${dest}" ]] && [[ "${FORCE}" != "true" ]]; then
      skipped=$((skipped + 1))
      continue
    fi

    local url="${RAW_BASE}/${gh_path}/${filename}"
    if curl -sL -o "${dest}" "${url}" 2>/dev/null; then
      # Verify we got actual content (not a GitHub error page)
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

    # Progress + rate-limit courtesy
    local done_count=$((downloaded + skipped + failed))
    if (( done_count % 50 == 0 )) && (( done_count > 0 )); then
      echo "[superkonna]   Progress: ${done_count}/${total} (${downloaded} new, ${skipped} exist)..."
      sleep 0.5
    fi
  done <<< "${file_list}"

  echo "[superkonna] ${label}: ${downloaded} downloaded, ${skipped} already existed, ${failed} failed"
}

# ─────────────────────────────────────────────────────
# Main
# ─────────────────────────────────────────────────────
echo "[superkonna] Canvas Asset Fetcher"
echo "[superkonna] Source: github.com/${CANVAS_REPO}"
echo "[superkonna] License note: CC0 in LICENSE file, CC BY-NC-SA in README — local use only"

if [[ -z "${ONLY}" ]] || [[ "${ONLY}" == "backgrounds" ]]; then
  fetch_asset_set \
    "_inc/systems/carousel-icons-art" \
    "${SYSTEMS_DIR}" \
    "webp" \
    "System Backgrounds (306 webp)"
fi

if [[ -z "${ONLY}" ]] || [[ "${ONLY}" == "icons" ]]; then
  fetch_asset_set \
    "_inc/systems/carousel-icons-icons" \
    "${ICONS_DIR}" \
    "webp" \
    "System Icons (310 webp)"
fi

if [[ -z "${ONLY}" ]] || [[ "${ONLY}" == "logos" ]]; then
  fetch_asset_set \
    "_inc/systems/logos" \
    "${LOGOS_DIR}" \
    "svg" \
    "System Logos (322 svg)"
fi

# Write marker
date -u '+%Y-%m-%dT%H:%M:%SZ' > "${MARKER}"

echo ""
echo "[superkonna] All done. Canvas assets are for local/personal use only."
echo "[superkonna] These files should not be committed to the repo."
