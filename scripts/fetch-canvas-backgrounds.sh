#!/bin/bash
# =============================================================================
# Superkonna Theme — Canvas Background Downloader
# =============================================================================
# Downloads per-system background artwork from the Canvas ES theme
# (Siddy212/canvas-es) for local use. These are NOT shipped in the repo
# due to licensing ambiguity (CC0 in LICENSE vs CC BY-NC-SA in README).
#
# The shipped Elementerial backgrounds (MIT) remain as fallback.
# Canvas backgrounds are higher quality and cover more systems (306 vs 145).
#
# Usage:
#   ./fetch-canvas-backgrounds.sh [--theme-dir /path/to/theme] [--force]
#
# Requires: curl or wget, and an internet connection.
# =============================================================================

set -euo pipefail

THEME_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SYSTEMS_DIR="${THEME_DIR}/assets/systems"
FORCE=false
CANVAS_REPO="Siddy212/canvas-es"
CANVAS_BRANCH="master"
CANVAS_ART_PATH="_inc/systems/carousel-icons-art"
RAW_BASE="https://raw.githubusercontent.com/${CANVAS_REPO}/${CANVAS_BRANCH}/${CANVAS_ART_PATH}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --theme-dir) THEME_DIR="$2"; SYSTEMS_DIR="${THEME_DIR}/assets/systems"; shift 2 ;;
    --force) FORCE=true; shift ;;
    *) shift ;;
  esac
done

mkdir -p "${SYSTEMS_DIR}"

# Check for marker file to avoid re-downloading
MARKER="${SYSTEMS_DIR}/.canvas-fetched"
if [[ -f "${MARKER}" ]] && [[ "${FORCE}" != "true" ]]; then
  echo "[superkonna] Canvas backgrounds already fetched. Use --force to re-download."
  exit 0
fi

# Get file listing from GitHub API
echo "[superkonna] Fetching Canvas system artwork list..."
FILE_LIST=$(curl -sL "https://api.github.com/repos/${CANVAS_REPO}/contents/${CANVAS_ART_PATH}?ref=${CANVAS_BRANCH}" \
  | python3 -c "
import json, sys
data = json.load(sys.stdin)
if isinstance(data, list):
    for item in data:
        if item.get('name', '').endswith('.webp') and item['name'] != '_default.webp':
            print(item['name'])
" 2>/dev/null)

if [[ -z "${FILE_LIST}" ]]; then
  echo "[superkonna] Error: Could not fetch file list from GitHub API."
  echo "[superkonna] You may be rate-limited. Try again later or use a GITHUB_TOKEN."
  exit 1
fi

total=$(echo "${FILE_LIST}" | wc -l | tr -d ' ')
echo "[superkonna] Found ${total} Canvas system backgrounds."

downloaded=0
skipped=0

while IFS= read -r filename; do
  [[ -z "${filename}" ]] && continue
  dest="${SYSTEMS_DIR}/${filename}"

  # Skip if file exists and not forcing
  if [[ -f "${dest}" ]] && [[ "${FORCE}" != "true" ]]; then
    skipped=$((skipped + 1))
    continue
  fi

  url="${RAW_BASE}/${filename}"
  if curl -sL -o "${dest}" "${url}" 2>/dev/null; then
    downloaded=$((downloaded + 1))
  else
    echo "[superkonna] Warning: Failed to download ${filename}"
  fi

  # Rate-limit courtesy
  if (( downloaded % 50 == 0 )) && (( downloaded > 0 )); then
    echo "[superkonna] Downloaded ${downloaded}/${total}..."
    sleep 1
  fi
done <<< "${FILE_LIST}"

# Write marker
date -u '+%Y-%m-%dT%H:%M:%SZ' > "${MARKER}"

echo "[superkonna] Done: ${downloaded} downloaded, ${skipped} skipped (already exist)"
echo "[superkonna] Canvas artwork is for local use only (not committed to repo)."
