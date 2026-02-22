#!/bin/bash
# =============================================================================
# Superkonna Theme — System Background Generator
# =============================================================================
# Generates gradient SVG backgrounds for systems that don't have
# custom artwork. Each system gets a unique color derived from its name.
#
# Artwork priority: .webp > .jpg > .png > generated .svg
# For real artwork: place files in assets/systems/{system}.webp
#
# Usage:
#   ./generate-system-backgrounds.sh [--theme-dir /path/to/theme]
# =============================================================================

set -euo pipefail

THEME_DIR="$(cd "$(dirname "$0")/.." && pwd)"
LOGOS_DIR="${THEME_DIR}/assets/logos"
SYSTEMS_DIR="${THEME_DIR}/assets/systems"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --theme-dir) THEME_DIR="$2"; LOGOS_DIR="${THEME_DIR}/assets/logos"; SYSTEMS_DIR="${THEME_DIR}/assets/systems"; shift 2 ;;
    *) shift ;;
  esac
done

mkdir -p "${SYSTEMS_DIR}"

# macOS uses md5, Linux uses md5sum
if command -v md5sum &>/dev/null; then
  md5cmd="md5sum"
elif command -v md5 &>/dev/null; then
  md5cmd="md5 -q"
else
  echo "[superkonna] Warning: no md5 command found, using fallback colors"
  md5cmd=""
fi

# Generate a deterministic color from a system name (hash-based)
name_to_hue() {
  local name="$1"
  if [[ -n "${md5cmd}" ]]; then
    local hash
    hash=$(echo -n "${name}" | ${md5cmd} | cut -c1-4)
    echo $(( 16#${hash} % 360 ))
  else
    echo "220"
  fi
}

# HSL to hex (fixed S=40%, L=15% for dark, moody backgrounds)
hsl_to_hex() {
  local h="$1"
  python3 -c "
import colorsys
r, g, b = colorsys.hls_to_rgb(${h}/360, 0.15, 0.4)
print(f'{int(r*255):02x}{int(g*255):02x}{int(b*255):02x}')
" 2>/dev/null || echo "1a1a2e"
}

generate_gradient_bg() {
  local system="$1"
  local output="${SYSTEMS_DIR}/${system}.svg"

  # Skip if any real artwork exists (webp, jpg, or png)
  if [[ -f "${SYSTEMS_DIR}/${system}.webp" ]] || \
     [[ -f "${SYSTEMS_DIR}/${system}.jpg" ]] || \
     [[ -f "${SYSTEMS_DIR}/${system}.png" ]]; then
    return 1
  fi

  local hue
  hue=$(name_to_hue "${system}")
  local color1
  color1=$(hsl_to_hex "${hue}")
  local hue2=$(( (hue + 30) % 360 ))
  local color2
  color2=$(hsl_to_hex "${hue2}")

  cat > "${output}" <<EOF
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1920 1080">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="#${color1}"/>
      <stop offset="100%" stop-color="#${color2}"/>
    </linearGradient>
  </defs>
  <rect width="1920" height="1080" fill="url(#bg)"/>
</svg>
EOF
  return 0
}

# Get all system names from existing logos
echo "[superkonna] Generating fallback backgrounds..."
generated=0
skipped=0

for logo in "${LOGOS_DIR}"/*.svg; do
  [[ -f "${logo}" ]] || continue
  system=$(basename "${logo}" .svg)
  if generate_gradient_bg "${system}"; then
    generated=$((generated + 1))
  else
    skipped=$((skipped + 1))
  fi
done

echo "[superkonna] Done: ${generated} gradients generated, ${skipped} skipped (artwork exists)"
echo "[superkonna] To replace gradients with real artwork: assets/systems/{system}.webp"
