#!/bin/bash
# =============================================================================
# Superkonna Theme — Alekfull-NX Asset Fetcher
# =============================================================================
# Copies sounds and system background images from an installed Alekfull-NX
# theme. Run this on the box where both themes are installed.
#
# Copies:
#   - UI sounds (klick.wav, select.wav) → assets/sounds/ (mapped to all roles)
#   - System backgrounds (443 .jpg)     → assets/systems/
#
# Usage:
#   ./fetch-alekful-assets.sh [--theme-dir /path] [--source-dir /path]
# =============================================================================

set -euo pipefail

THEME_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SOURCE_DIR=""
SOUNDS_ONLY=false
BGS_ONLY=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --theme-dir)  THEME_DIR="$2"; shift 2 ;;
    --source-dir) SOURCE_DIR="$2"; shift 2 ;;
    --sounds)     SOUNDS_ONLY=true; shift ;;
    --backgrounds) BGS_ONLY=true; shift ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# Auto-detect Alekfull-NX location
if [ -z "$SOURCE_DIR" ]; then
  for candidate in \
    /userdata/themes/Alekfull-NX \
    /userdata/themes/alekfull-nx \
    "$THEME_DIR/../Alekfull-NX" \
    "$THEME_DIR/../alekfull-nx"; do
    if [ -d "$candidate" ]; then
      SOURCE_DIR="$candidate"
      break
    fi
  done
fi

if [ -z "$SOURCE_DIR" ] || [ ! -d "$SOURCE_DIR" ]; then
  echo "ERROR: Alekfull-NX theme not found. Install it first or use --source-dir."
  exit 1
fi

echo "Source: $SOURCE_DIR"
echo "Target: $THEME_DIR"

# ── Sounds ──────────────────────────────────────────────────────────────
if [ "$BGS_ONLY" = false ]; then
  SOUND_SRC="$SOURCE_DIR/assets/uisounds"
  SOUND_DST="$THEME_DIR/assets/sounds"

  if [ ! -d "$SOUND_SRC" ]; then
    echo "WARN: No uisounds dir at $SOUND_SRC"
  else
    mkdir -p "$SOUND_DST"

    # klick.wav → navigation sounds (tick, scroll, back, favorite)
    if [ -f "$SOUND_SRC/klick.wav" ]; then
      for target in tick.wav scroll.wav back.wav favorite.wav; do
        cp "$SOUND_SRC/klick.wav" "$SOUND_DST/$target"
      done
      echo "OK: Navigation sounds (tick, scroll, back, favorite) ← klick.wav"
    fi

    # select.wav → selection sounds (select, launch)
    if [ -f "$SOUND_SRC/select.wav" ]; then
      for target in select.wav launch.wav; do
        cp "$SOUND_SRC/select.wav" "$SOUND_DST/$target"
      done
      echo "OK: Selection sounds (select, launch) ← select.wav"
    fi
  fi
fi

# ── System backgrounds ──────────────────────────────────────────────────
if [ "$SOUNDS_ONLY" = false ]; then
  ART_SRC="$SOURCE_DIR/assets/arts"
  ART_DST="$THEME_DIR/assets/systems"

  if [ ! -d "$ART_SRC" ]; then
    echo "WARN: No arts dir at $ART_SRC"
  else
    mkdir -p "$ART_DST"
    count=0
    skipped=0

    for f in "$ART_SRC"/*.jpg "$ART_SRC"/*.png "$ART_SRC"/*.webp; do
      [ -f "$f" ] || continue
      name="$(basename "$f")"
      stem="${name%.*}"

      # Skip resolution-specific directories that got globbed
      [ -d "$f" ] && continue

      # Copy as-is (preserve original format)
      cp "$f" "$ART_DST/$name"

      # Also remove any .webp duplicate with same stem (prefer Alekfull art)
      if [ "${name##*.}" = "jpg" ] && [ -f "$ART_DST/$stem.webp" ]; then
        rm "$ART_DST/$stem.webp"
      fi

      count=$((count + 1))
    done
    echo "OK: Copied $count system backgrounds"
  fi
fi

# ── System logos (fill gaps — only copy if we don't already have one) ──
if [ "$SOUNDS_ONLY" = false ]; then
  LOGO_SRC="$SOURCE_DIR/assets/logos"
  LOGO_DST="$THEME_DIR/assets/logos"

  if [ -d "$LOGO_SRC" ]; then
    mkdir -p "$LOGO_DST"
    logo_count=0

    for f in "$LOGO_SRC"/*.jpg "$LOGO_SRC"/*.png "$LOGO_SRC"/*.svg; do
      [ -f "$f" ] || continue
      name="$(basename "$f")"
      stem="${name%.*}"

      # Only copy if no logo exists for this system (any format)
      if [ ! -f "$LOGO_DST/$stem.svg" ] && [ ! -f "$LOGO_DST/$stem.png" ] && [ ! -f "$LOGO_DST/$stem.jpg" ]; then
        cp "$f" "$LOGO_DST/$name"
        logo_count=$((logo_count + 1))
      fi
    done
    echo "OK: Copied $logo_count missing logos"
  fi
fi

echo "Done."
