#!/bin/bash
# =============================================================================
# Superkonna Theme — Weather Widget Updater
# =============================================================================
# Polls Open-Meteo API for current weather and writes icon + temperature
# to files that the theme displays via file-swap.
#
# Usage:
#   ./update-weather.sh [--theme-dir /path/to/theme] [--lat 60.17] [--lon 24.94]
#
# Install as cron (every 30 minutes):
#   echo "*/30 * * * * /path/to/theme/scripts/update-weather.sh" | crontab -
#
# Or as a systemd timer, or run manually.
# =============================================================================

set -euo pipefail

THEME_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_DIR="${THEME_DIR}/generated"
LAT="${LAT:-60.17}"    # Default: Helsinki
LON="${LON:-24.94}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --theme-dir) THEME_DIR="$2"; OUTPUT_DIR="${THEME_DIR}/generated"; shift 2 ;;
    --lat) LAT="$2"; shift 2 ;;
    --lon) LON="$2"; shift 2 ;;
    *) shift ;;
  esac
done

mkdir -p "${OUTPUT_DIR}"

# ─── Fetch Weather ───────────────────────────────────────────────────────────

API_URL="https://api.open-meteo.com/v1/forecast?latitude=${LAT}&longitude=${LON}&current=temperature_2m,weather_code&timezone=auto"

RESPONSE=$(curl -sf "${API_URL}" 2>/dev/null || echo "")

if [[ -z "${RESPONSE}" ]]; then
  echo "[superkonna] Weather fetch failed, skipping update"
  exit 0
fi

TEMP=$(echo "${RESPONSE}" | grep -o '"temperature_2m":[0-9.-]*' | head -1 | cut -d: -f2)
WMO_CODE=$(echo "${RESPONSE}" | grep -o '"weather_code":[0-9]*' | head -1 | cut -d: -f2)

if [[ -z "${TEMP}" ]] || [[ -z "${WMO_CODE}" ]]; then
  echo "[superkonna] Could not parse weather response"
  exit 0
fi

# Round temperature
TEMP_ROUND=$(printf "%.0f" "${TEMP}")

# ─── WMO Code → Icon Mapping ────────────────────────────────────────────────
# https://open-meteo.com/en/docs — WMO Weather interpretation codes

weather_icon() {
  local code="$1"
  case "${code}" in
    0)         echo "clear" ;;           # Clear sky
    1|2|3)     echo "partly-cloudy" ;;   # Mainly clear, partly cloudy, overcast
    45|48)     echo "fog" ;;             # Fog, rime fog
    51|53|55)  echo "drizzle" ;;         # Drizzle
    56|57)     echo "freezing-drizzle" ;; # Freezing drizzle
    61|63|65)  echo "rain" ;;            # Rain
    66|67)     echo "freezing-rain" ;;   # Freezing rain
    71|73|75)  echo "snow" ;;            # Snow fall
    77)        echo "snow-grains" ;;     # Snow grains
    80|81|82)  echo "showers" ;;         # Rain showers
    85|86)     echo "snow-showers" ;;    # Snow showers
    95)        echo "thunderstorm" ;;    # Thunderstorm
    96|99)     echo "thunderstorm-hail" ;; # Thunderstorm with hail
    *)         echo "unknown" ;;
  esac
}

ICON_NAME=$(weather_icon "${WMO_CODE}")

# ─── Generate Weather Icon SVG ───────────────────────────────────────────────

generate_weather_svg() {
  local icon="$1"
  local temp="$2"
  local output="$3"

  # Simple weather symbols using basic SVG shapes
  local symbol=""
  case "${icon}" in
    clear)
      symbol='<circle cx="16" cy="16" r="8" fill="#FFD700" stroke="#FFA500" stroke-width="1"/>'
      symbol+='<g stroke="#FFD700" stroke-width="1.5"><line x1="16" y1="2" x2="16" y2="6"/><line x1="16" y1="26" x2="16" y2="30"/><line x1="2" y1="16" x2="6" y2="16"/><line x1="26" y1="16" x2="30" y2="16"/></g>'
      ;;
    partly-cloudy)
      symbol='<circle cx="13" cy="14" r="6" fill="#FFD700"/>'
      symbol+='<path d="M10 22 C10 18 14 16 18 16 C22 16 24 18 24 20 C26 20 28 22 28 24 C28 27 26 28 24 28 L12 28 C9 28 7 26 7 24 C7 22 9 21 10 22Z" fill="#FFFFFF"/>'
      ;;
    fog)
      symbol='<g fill="none" stroke="#CCCCCC" stroke-width="2" stroke-linecap="round"><line x1="4" y1="12" x2="28" y2="12"/><line x1="6" y1="18" x2="26" y2="18"/><line x1="4" y1="24" x2="28" y2="24"/></g>'
      ;;
    drizzle|freezing-drizzle)
      symbol='<path d="M8 16 C8 12 12 10 16 10 C20 10 22 12 22 14 C24 14 26 16 26 18 C26 21 24 22 22 22 L10 22 C7 22 5 20 5 18 C5 16 7 15 8 16Z" fill="#AACCEE"/>'
      symbol+='<g fill="#6699CC"><circle cx="12" cy="26" r="1"/><circle cx="18" cy="28" r="1"/></g>'
      ;;
    rain|freezing-rain|showers)
      symbol='<path d="M8 14 C8 10 12 8 16 8 C20 8 22 10 22 12 C24 12 26 14 26 16 C26 19 24 20 22 20 L10 20 C7 20 5 18 5 16 C5 14 7 13 8 14Z" fill="#8899BB"/>'
      symbol+='<g fill="#4477AA"><circle cx="10" cy="24" r="1.2"/><circle cx="16" cy="26" r="1.2"/><circle cx="22" cy="24" r="1.2"/></g>'
      ;;
    snow|snow-grains|snow-showers)
      symbol='<path d="M8 14 C8 10 12 8 16 8 C20 8 22 10 22 12 C24 12 26 14 26 16 C26 19 24 20 22 20 L10 20 C7 20 5 18 5 16 C5 14 7 13 8 14Z" fill="#BBCCDD"/>'
      symbol+='<g fill="#FFFFFF"><circle cx="10" cy="24" r="1.5"/><circle cx="16" cy="26" r="1.5"/><circle cx="22" cy="24" r="1.5"/></g>'
      ;;
    thunderstorm|thunderstorm-hail)
      symbol='<path d="M8 12 C8 8 12 6 16 6 C20 6 22 8 22 10 C24 10 26 12 26 14 C26 17 24 18 22 18 L10 18 C7 18 5 16 5 14 C5 12 7 11 8 12Z" fill="#556677"/>'
      symbol+='<polygon points="15,19 12,25 17,25 14,31" fill="#FFD700"/>'
      ;;
    *)
      symbol='<circle cx="16" cy="16" r="4" fill="#888888"/>'
      ;;
  esac

  cat > "${output}" <<EOF
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">
  ${symbol}
</svg>
EOF
}

# ─── Write Output ────────────────────────────────────────────────────────────

generate_weather_svg "${ICON_NAME}" "${TEMP_ROUND}" "${OUTPUT_DIR}/weather-icon.svg"

# Write temperature as a simple text file (theme can't read text files,
# but we include it for potential hook scripts that inject into ES settings)
echo "${TEMP_ROUND}°" > "${OUTPUT_DIR}/weather-temp.txt"

echo "[superkonna] Weather updated: ${TEMP_ROUND}° (${ICON_NAME}, WMO ${WMO_CODE})"
