#!/bin/bash
# =============================================================================
# Superkonna Theme — Profile Widget Updater
# =============================================================================
# Reads the active romhoard user profile via API and writes display files
# that the theme shows in the top bar (username, balance, avatar).
#
# Usage:
#   ./update-profile.sh [--theme-dir /path/to/theme] [--api http://localhost:9090]
#
# Run from ES event hooks or periodically via cron.
# =============================================================================

set -euo pipefail

THEME_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_DIR="${THEME_DIR}/generated"
API_URL="${ROMHOARD_API:-http://localhost:9090}"

mkdir -p "$OUTPUT_DIR"

# ── Read active color scheme ──────────────────────────────────────────

# Parse the active theme colors from the color scheme XML
# so the badge SVG matches the theme palette.
read_color() {
    local file="$1" tag="$2"
    if [ -f "$file" ]; then
        # Extract <tag>VALUE</tag>
        sed -n "s/.*<${tag}>\([^<]*\)<\/${tag}>.*/\1/p" "$file" | head -1
    fi
}

# Determine active color scheme from es_settings.cfg
ES_SETTINGS="/userdata/system/configs/emulationstation/es_settings.cfg"
THEME_NAME="$(basename "$THEME_DIR")"
SCHEME="light"

if [ -f "$ES_SETTINGS" ]; then
    FOUND_SCHEME="$(grep "subset\.${THEME_NAME}\.Color scheme" "$ES_SETTINGS" 2>/dev/null \
        | sed 's/.*value="\([^"]*\)".*/\1/' | head -1)"
    if [ -n "$FOUND_SCHEME" ]; then
        SCHEME="$FOUND_SCHEME"
    fi
fi

SCHEME_XML="${THEME_DIR}/settings/colors/${SCHEME}/main.xml"
if [ ! -f "$SCHEME_XML" ]; then
    SCHEME_XML="${THEME_DIR}/settings/colors/light/main.xml"
fi

FG_COLOR="$(read_color "$SCHEME_XML" "fgColor")"
BG_COLOR="$(read_color "$SCHEME_XML" "bgColor")"
MAIN_COLOR="$(read_color "$SCHEME_XML" "mainColor")"
SUBTLE_COLOR="$(read_color "$SCHEME_XML" "subtleColor")"

# Defaults if parsing fails
FG_COLOR="${FG_COLOR:-EAEDF3}"
BG_COLOR="${BG_COLOR:-0E1117}"
MAIN_COLOR="${MAIN_COLOR:-D64060}"
SUBTLE_COLOR="${SUBTLE_COLOR:-A0A8B8}"

# Use a warm gold for the balance pill accent (konnapenni = gold coins)
ACCENT_COLOR="f9e2af"

# ── Read active profile ───────────────────────────────────────────────

ACTIVE_PROFILE=""
if [ -f /userdata/system/profiles/active ]; then
    ACTIVE_PROFILE="$(cat /userdata/system/profiles/active 2>/dev/null || echo '')"
fi

# ── Try badge SVG endpoint first (single call, theme-aware) ──────────

BADGE_URL="${API_URL}/api/profile/badge.svg?fg=${FG_COLOR}&bg=${BG_COLOR}&accent=${ACCENT_COLOR}&subtle=${SUBTLE_COLOR}"
if [ -n "$ACTIVE_PROFILE" ]; then
    BADGE_URL="${BADGE_URL}&profile_id=${ACTIVE_PROFILE}"
fi

if curl -sf "$BADGE_URL" -o "$OUTPUT_DIR/profile-badge.svg" 2>/dev/null; then
    echo "Profile badge updated from API (scheme: ${SCHEME})"
    exit 0
fi

# ── Fallback: query JSON API and build badge locally ──────────────────

USER_JSON=""
if [ -n "$ACTIVE_PROFILE" ]; then
    USER_JSON="$(curl -sf "${API_URL}/api/profile/${ACTIVE_PROFILE}" 2>/dev/null || echo '')"
fi

if [ -z "$USER_JSON" ]; then
    USER_JSON="$(curl -sf "${API_URL}/api/current-user" 2>/dev/null || echo '')"
fi

# Parse user info
USERNAME=""
BALANCE=""
CURRENCY=""
AVATAR=""

if [ -n "$USER_JSON" ] && command -v jq >/dev/null 2>&1; then
    USERNAME="$(echo "$USER_JSON" | jq -r '.name // empty' 2>/dev/null || echo '')"
    BALANCE="$(echo "$USER_JSON" | jq -r '.balance // empty' 2>/dev/null || echo '')"
    CURRENCY="$(echo "$USER_JSON" | jq -r '.currency_symbol // empty' 2>/dev/null || echo '')"
    AVATAR="$(echo "$USER_JSON" | jq -r '.avatar_url // empty' 2>/dev/null || echo '')"
elif [ -n "$USER_JSON" ] && command -v python3 >/dev/null 2>&1; then
    USERNAME="$(echo "$USER_JSON" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('name',''))" 2>/dev/null || echo '')"
    BALANCE="$(echo "$USER_JSON" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('balance',''))" 2>/dev/null || echo '')"
    CURRENCY="$(echo "$USER_JSON" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('currency_symbol',''))" 2>/dev/null || echo '')"
    AVATAR="$(echo "$USER_JSON" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('avatar_url',''))" 2>/dev/null || echo '')"
fi

# Write text files (for other consumers)
echo "${USERNAME:-Player}" > "$OUTPUT_DIR/profile-name.txt"
if [ -n "$BALANCE" ] && [ -n "$CURRENCY" ]; then
    echo "${BALANCE} ${CURRENCY}" > "$OUTPUT_DIR/profile-balance.txt"
else
    echo "" > "$OUTPUT_DIR/profile-balance.txt"
fi

# Download avatar if provided
if [ -n "$AVATAR" ]; then
    CACHED_URL=""
    [ -f "$OUTPUT_DIR/.avatar-url" ] && CACHED_URL="$(cat "$OUTPUT_DIR/.avatar-url")"
    if [ "$AVATAR" != "$CACHED_URL" ]; then
        if curl -sf "$AVATAR" -o "$OUTPUT_DIR/profile-avatar.png" 2>/dev/null; then
            echo "$AVATAR" > "$OUTPUT_DIR/.avatar-url"
        fi
    fi
fi

# Generate profile badge SVG locally (fallback)
DISPLAY_NAME="${USERNAME:-Player}"
BALANCE_TEXT=""
if [ -n "$BALANCE" ] && [ -n "$CURRENCY" ] && [ "$BALANCE" != "0" ]; then
    BALANCE_TEXT="${BALANCE} ${CURRENCY}"
fi

# Calculate widths
NAME_LEN=${#DISPLAY_NAME}
NAME_W=$(( NAME_LEN * 8 + 4 ))

if [ -n "$BALANCE_TEXT" ]; then
    BAL_LEN=${#BALANCE_TEXT}
    BAL_W=$(( BAL_LEN * 7 + 32 ))
    TOTAL_W=$(( 28 + NAME_W + 12 + BAL_W + 8 ))
else
    BAL_W=0
    TOTAL_W=$(( 28 + NAME_W + 8 ))
fi

cat > "$OUTPUT_DIR/profile-badge.svg" << SVGEOF
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${TOTAL_W} 32">
  <circle cx="13" cy="11" r="5.5" fill="#${FG_COLOR}" opacity="0.85"/>
  <path d="M4,27 Q4,19 13,19 Q22,19 22,27" fill="#${FG_COLOR}" opacity="0.6"/>
  <text x="28" y="20" font-family="Inter,sans-serif" font-size="13" fill="#${FG_COLOR}" font-weight="500" opacity="0.95">${DISPLAY_NAME}</text>
$(if [ -n "$BALANCE_TEXT" ]; then
  PILL_X=$(( 28 + NAME_W + 8 ))
  COIN_X=$(( PILL_X + 10 ))
  TEXT_X=$(( PILL_X + 22 ))
  cat << BALEOF
  <rect x="${PILL_X}" y="5" width="${BAL_W}" rx="11" ry="11" height="22" fill="#${BG_COLOR}" opacity="0.65"/>
  <circle cx="${COIN_X}" cy="16" r="7" fill="#${ACCENT_COLOR}" opacity="0.9"/>
  <text x="${COIN_X}" y="20" font-family="Inter,sans-serif" font-size="9" fill="#${BG_COLOR}" text-anchor="middle" font-weight="700">K</text>
  <text x="${TEXT_X}" y="20" font-family="Inter,sans-serif" font-size="11" fill="#${ACCENT_COLOR}" font-weight="600">${BALANCE_TEXT}</text>
BALEOF
fi)
</svg>
SVGEOF

echo "Profile updated: ${DISPLAY_NAME} (fallback, scheme: ${SCHEME})"
