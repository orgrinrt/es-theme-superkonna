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

# Read active batocera profile
ACTIVE_PROFILE=""
if [ -f /userdata/system/profiles/active ]; then
    ACTIVE_PROFILE="$(cat /userdata/system/profiles/active 2>/dev/null || echo '')"
fi

# Query romhoard for user info
USER_JSON=""
if [ -n "$ACTIVE_PROFILE" ]; then
    USER_JSON="$(curl -sf "${API_URL}/api/profile/${ACTIVE_PROFILE}" 2>/dev/null || echo '')"
fi

if [ -z "$USER_JSON" ]; then
    # fallback: try to get any current user
    USER_JSON="$(curl -sf "${API_URL}/api/current-user" 2>/dev/null || echo '')"
fi

if [ -n "$USER_JSON" ] && command -v python3 >/dev/null 2>&1; then
    USERNAME="$(echo "$USER_JSON" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('name',''))" 2>/dev/null || echo '')"
    BALANCE="$(echo "$USER_JSON" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('balance',''))" 2>/dev/null || echo '')"
    CURRENCY="$(echo "$USER_JSON" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('currency_symbol',''))" 2>/dev/null || echo '')"
    AVATAR="$(echo "$USER_JSON" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('avatar_url',''))" 2>/dev/null || echo '')"
elif [ -n "$USER_JSON" ] && command -v jq >/dev/null 2>&1; then
    USERNAME="$(echo "$USER_JSON" | jq -r '.name // empty' 2>/dev/null || echo '')"
    BALANCE="$(echo "$USER_JSON" | jq -r '.balance // empty' 2>/dev/null || echo '')"
    CURRENCY="$(echo "$USER_JSON" | jq -r '.currency_symbol // empty' 2>/dev/null || echo '')"
    AVATAR="$(echo "$USER_JSON" | jq -r '.avatar_url // empty' 2>/dev/null || echo '')"
else
    USERNAME=""
    BALANCE=""
    CURRENCY=""
    AVATAR=""
fi

# Write profile display name
if [ -n "$USERNAME" ]; then
    echo "$USERNAME" > "$OUTPUT_DIR/profile-name.txt"
else
    echo "Player" > "$OUTPUT_DIR/profile-name.txt"
fi

# Write balance display
if [ -n "$BALANCE" ] && [ -n "$CURRENCY" ]; then
    echo "${BALANCE} ${CURRENCY}" > "$OUTPUT_DIR/profile-balance.txt"
else
    echo "" > "$OUTPUT_DIR/profile-balance.txt"
fi

# Download avatar if provided and different from cached
if [ -n "$AVATAR" ]; then
    CACHED_URL=""
    [ -f "$OUTPUT_DIR/.avatar-url" ] && CACHED_URL="$(cat "$OUTPUT_DIR/.avatar-url")"
    if [ "$AVATAR" != "$CACHED_URL" ]; then
        if curl -sf "$AVATAR" -o "$OUTPUT_DIR/profile-avatar.png" 2>/dev/null; then
            echo "$AVATAR" > "$OUTPUT_DIR/.avatar-url"
        fi
    fi
fi

# Generate profile badge SVG (username + balance) for ES theme display
DISPLAY_NAME="${USERNAME:-Player}"
BALANCE_TEXT=""
if [ -n "$BALANCE" ] && [ -n "$CURRENCY" ] && [ "$BALANCE" != "0" ]; then
    BALANCE_TEXT="${BALANCE} ${CURRENCY}"
fi

# SVG with user icon + name + optional balance pill
cat > "$OUTPUT_DIR/profile-badge.svg" << SVGEOF
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 32">
  <!-- user icon -->
  <circle cx="14" cy="12" r="6" fill="#cdd6f4" opacity="0.9"/>
  <path d="M4,28 Q4,20 14,20 Q24,20 24,28" fill="#cdd6f4" opacity="0.7"/>
  <!-- username -->
  <text x="30" y="20" font-family="sans-serif" font-size="14" fill="#cdd6f4" font-weight="500">${DISPLAY_NAME}</text>
$(if [ -n "$BALANCE_TEXT" ]; then
  cat << BALEOF
  <!-- balance pill -->
  <rect x="120" y="4" width="72" rx="10" ry="10" height="24" fill="#1e1e2e" opacity="0.7"/>
  <text x="156" y="21" font-family="sans-serif" font-size="11" fill="#f9e2af" text-anchor="middle" font-weight="600">${BALANCE_TEXT}</text>
BALEOF
fi)
</svg>
SVGEOF

echo "Profile updated: ${DISPLAY_NAME}"
