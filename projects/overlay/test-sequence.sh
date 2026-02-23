#!/usr/bin/env bash
# Test harness for superkonna-overlay
# Sends a sequence of popup and menu commands via the unix socket.
# Usage: ssh root@superkonna.local 'bash -s' < test-sequence.sh
#   or:  scp test-sequence.sh root@superkonna.local:/tmp/ && ssh root@superkonna.local bash /tmp/test-sequence.sh

set -euo pipefail

SOCK="/tmp/superkonna-overlay.sock"

send() {
  echo "$1" | socat - UNIX-CONNECT:"$SOCK" 2>/dev/null \
    || echo "$1" > "$SOCK" 2>/dev/null \
    || printf '%s\n' "$1" | nc -U "$SOCK" 2>/dev/null \
    || { echo "WARN: could not send '$1' — is overlay running?" >&2; return 1; }
  echo "  → $1"
}

echo "=== Overlay QA test sequence ==="
echo ""

# ── Achievement popups ───────────────────────────────────────
echo "▶ Phase 1: Achievement popups"
echo ""

send "POPUP First Blood|Defeat the first enemy"
sleep 6

send "POPUP Speed Demon|Complete level 1 in under 60 seconds"
sleep 6

send "POPUP Completionist|Collect all 120 stars across all worlds"
sleep 6

# rapid-fire: queue 3 fast to test queuing
echo ""
echo "▶ Phase 2: Rapid-fire popups (queue test)"
echo ""

send "POPUP Headshot|Land a perfect critical hit"
sleep 1
send "POPUP Treasure Hunter|Find the hidden cave treasure chest"
sleep 1
send "POPUP No Damage Run|Clear the boss without taking a hit"
echo "  (waiting for queue to drain...)"
sleep 18

# long text truncation test
echo ""
echo "▶ Phase 3: Long text (truncation test)"
echo ""

send "POPUP The Legend of the Ancient Dragon Slayer|This is a very long achievement description that should be truncated with an ellipsis because it exceeds the maximum width of the popup card"
sleep 6

# no description
send "POPUP Silent Victory|"
sleep 6

# ── Menu interaction ──────────────────────────────────────────
echo ""
echo "▶ Phase 4: Menu overlay"
echo ""

echo "  Opening menu..."
send "MENU_TOGGLE"
sleep 2

echo "  Navigating down..."
send "MENU_DOWN"
sleep 1
send "MENU_DOWN"
sleep 1

echo "  Navigating up..."
send "MENU_UP"
sleep 1

echo "  Selecting item (first press = highlight)..."
send "MENU_SELECT"
sleep 2

echo "  Closing menu via back..."
send "MENU_BACK"
sleep 2

# ── Menu + popup overlap ─────────────────────────────────────
echo ""
echo "▶ Phase 5: Popup while menu is closed"
echo ""

send "POPUP Game Over|Thanks for testing the overlay"
sleep 6

echo ""
echo "=== QA sequence complete ==="
