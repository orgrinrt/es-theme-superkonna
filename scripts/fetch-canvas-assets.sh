#!/bin/bash
# =============================================================================
# Superkonna Theme — Canvas Asset Fetcher
# =============================================================================
# Downloads per-system assets from the Canvas ES theme (Siddy212/canvas-es)
# into a LOCAL CACHE outside the theme directory. A separate link step
# creates symlinks from the theme into this cache for missing assets only.
#
# Assets fetched:
#   - System backgrounds (306 webp)
#   - System logos (322 svg)
#   - System icons (310 webp) [bonus, not linked by default]
#
# Cache location: /userdata/theme-assets/canvas/
# (configurable via --cache-dir)
#
# Usage:
#   ./fetch-canvas-assets.sh [--cache-dir /path] [--force]
#   ./fetch-canvas-assets.sh --link [--theme-dir /path] [--cache-dir /path]
#   ./fetch-canvas-assets.sh --unlink [--theme-dir /path]
#
# Modes:
#   (default)   Download Canvas assets to cache directory
#   --link      Create symlinks from theme into cache (missing assets only)
#   --unlink    Remove all symlinks from theme that point into the cache
#
# Requires: curl, python3, internet connection (for fetch)
# =============================================================================

set -euo pipefail

THEME_DIR="$(cd "$(dirname "$0")/.." && pwd)"
CACHE_DIR="/userdata/theme-assets/canvas"
FORCE=false
MODE="fetch"
CANVAS_REPO="Siddy212/canvas-es"
CANVAS_BRANCH="master"
RAW_BASE="https://raw.githubusercontent.com/${CANVAS_REPO}/${CANVAS_BRANCH}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --theme-dir) THEME_DIR="$2"; shift 2 ;;
    --cache-dir) CACHE_DIR="$2"; shift 2 ;;
    --force) FORCE=true; shift ;;
    --link) MODE="link"; shift ;;
    --unlink) MODE="unlink"; shift ;;
    *) shift ;;
  esac
done

# ─────────────────────────────────────────────────────
# FETCH MODE: download Canvas assets to cache
# ─────────────────────────────────────────────────────
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
    echo "[superkonna] Warning: Could not fetch list for ${label}. Skipping."
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

    local url="${RAW_BASE}/${gh_path}/${filename}"
    if curl -sL -o "${dest}" "${url}" 2>/dev/null; then
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
      echo "[superkonna]   Progress: ${done_count}/${total} (${downloaded} new)..."
      sleep 0.5
    fi
  done <<< "${file_list}"

  echo "[superkonna] ${label}: ${downloaded} downloaded, ${skipped} existed, ${failed} failed"
}

do_fetch() {
  local marker="${CACHE_DIR}/.fetched"

  if [[ -f "${marker}" ]] && [[ "${FORCE}" != "true" ]]; then
    echo "[superkonna] Cache already populated ($(cat "${marker}")). Use --force to re-download."
    exit 0
  fi

  echo "[superkonna] Canvas Asset Fetcher"
  echo "[superkonna] Cache: ${CACHE_DIR}"
  echo ""

  fetch_asset_set "_inc/systems/carousel-icons-art" "${CACHE_DIR}/systems" "webp" "System Backgrounds (306 webp)"
  echo ""
  fetch_asset_set "_inc/systems/logos" "${CACHE_DIR}/logos" "svg" "System Logos (322 svg)"
  echo ""
  fetch_asset_set "_inc/systems/carousel-icons-icons" "${CACHE_DIR}/icons" "webp" "System Icons (310 webp)"

  date -u '+%Y-%m-%dT%H:%M:%SZ' > "${marker}"

  echo ""
  echo "[superkonna] Cache populated at ${CACHE_DIR}"
  echo "[superkonna] Run with --link to symlink into theme."
}

# ─────────────────────────────────────────────────────
# LINK MODE: symlink missing theme assets from cache
# ─────────────────────────────────────────────────────
do_link() {
  if [[ ! -d "${CACHE_DIR}" ]]; then
    echo "[superkonna] Error: Cache not found at ${CACHE_DIR}"
    echo "[superkonna] Run without --link first to download assets."
    exit 1
  fi

  echo "[superkonna] Linking Canvas assets into theme..."

  local linked=0 skipped=0

  # Link system backgrounds (webp)
  if [[ -d "${CACHE_DIR}/systems" ]]; then
    local dest_dir="${THEME_DIR}/assets/systems"
    mkdir -p "${dest_dir}"
    for src in "${CACHE_DIR}/systems"/*.webp; do
      [[ -f "${src}" ]] || continue
      local name
      name=$(basename "${src}")
      local dest="${dest_dir}/${name}"

      # Only link if the theme doesn't already have this file (as a real file)
      if [[ -e "${dest}" ]] && [[ ! -L "${dest}" ]]; then
        skipped=$((skipped + 1))
        continue
      fi

      ln -sf "${src}" "${dest}"
      linked=$((linked + 1))
    done
    echo "[superkonna] Backgrounds: ${linked} linked, ${skipped} shipped (kept)"
  fi

  linked=0 skipped=0

  # Link logos (svg)
  if [[ -d "${CACHE_DIR}/logos" ]]; then
    local dest_dir="${THEME_DIR}/assets/logos"
    mkdir -p "${dest_dir}"
    for src in "${CACHE_DIR}/logos"/*.svg; do
      [[ -f "${src}" ]] || continue
      local name
      name=$(basename "${src}")
      local dest="${dest_dir}/${name}"

      if [[ -e "${dest}" ]] && [[ ! -L "${dest}" ]]; then
        skipped=$((skipped + 1))
        continue
      fi

      ln -sf "${src}" "${dest}"
      linked=$((linked + 1))
    done
    echo "[superkonna] Logos: ${linked} linked, ${skipped} shipped (kept)"
  fi

  echo "[superkonna] Done. Shipped assets are untouched; Canvas fills the gaps."
}

# ─────────────────────────────────────────────────────
# UNLINK MODE: remove all symlinks pointing into cache
# ─────────────────────────────────────────────────────
do_unlink() {
  echo "[superkonna] Removing Canvas symlinks from theme..."

  local removed=0

  for dir in "${THEME_DIR}/assets/systems" "${THEME_DIR}/assets/logos"; do
    [[ -d "${dir}" ]] || continue
    while IFS= read -r -d '' link; do
      local target
      target=$(readlink "${link}" 2>/dev/null || true)
      if [[ "${target}" == "${CACHE_DIR}"* ]]; then
        rm "${link}"
        removed=$((removed + 1))
      fi
    done < <(find "${dir}" -maxdepth 1 -type l -print0 2>/dev/null)
  done

  echo "[superkonna] Removed ${removed} symlinks. Theme is back to shipped assets only."
}

# ─────────────────────────────────────────────────────
# Main dispatch
# ─────────────────────────────────────────────────────
case "${MODE}" in
  fetch) do_fetch ;;
  link) do_link ;;
  unlink) do_unlink ;;
esac
