#!/usr/bin/env bash
# setup-overlay.sh — Build and deploy the superkonna-overlay daemon on Batocera.
#
# What it does:
# 1. Cross-compiles the overlay daemon for x86_64-linux
# 2. Deploys the binary to the theme directory on Batocera
# 3. Configures RetroArch to suppress native achievement visuals
# 4. Creates an autostart entry for the daemon
#
# Usage:
#   ./scripts/setup-overlay.sh [--build-only] [--no-retroarch-config]
#
# Environment:
#   BATOCERA_HOST  — SSH host for the Batocera machine (default: batocera)
#   THEME_DIR      — Theme path on Batocera (default: /userdata/themes/es-theme-superkonna)

set -euo pipefail

RED='\033[0;31m'
GRN='\033[0;32m'
YEL='\033[0;33m'
CYN='\033[0;36m'
RST='\033[0m'

step() { printf "${CYN}:: %s${RST}\n" "$1"; }
ok()   { printf "${GRN}   ✓ %s${RST}\n" "$1"; }
warn() { printf "${YEL}   ⚠ %s${RST}\n" "$1"; }
fail() { printf "${RED}   ✗ %s${RST}\n" "$1"; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OVERLAY_DIR="$PROJECT_ROOT/projects/overlay"

BATOCERA_HOST="${BATOCERA_HOST:-batocera}"
THEME_DIR="${THEME_DIR:-/userdata/themes/es-theme-superkonna}"

BUILD_ONLY=false
NO_RA_CONFIG=false

for arg in "$@"; do
    case "$arg" in
        --build-only) BUILD_ONLY=true ;;
        --no-retroarch-config) NO_RA_CONFIG=true ;;
    esac
done

# ── Build ──────────────────────────────────────────────
step "Building overlay daemon"

if ! command -v cargo &>/dev/null; then
    fail "cargo not found — install Rust toolchain"
fi

# Check for cross-compilation target
TARGET="x86_64-unknown-linux-gnu"
if ! rustup target list --installed 2>/dev/null | grep -q "$TARGET"; then
    step "Adding cross-compilation target: $TARGET"
    rustup target add "$TARGET" || warn "Could not add target — you may need cross or cargo-zigbuild"
fi

cd "$OVERLAY_DIR"

# Try cargo-zigbuild first (most reliable cross-compilation), fall back to regular cargo
if command -v cargo-zigbuild &>/dev/null; then
    cargo zigbuild --release --target "$TARGET" 2>&1
elif command -v cross &>/dev/null; then
    cross build --release --target "$TARGET" 2>&1
else
    warn "Neither cargo-zigbuild nor cross found — attempting native cargo build"
    warn "This will only work if you have the right linker configured"
    cargo build --release --target "$TARGET" 2>&1
fi

BINARY="$OVERLAY_DIR/target/$TARGET/release/superkonna-overlay"
if [[ ! -f "$BINARY" ]]; then
    fail "Binary not found at $BINARY"
fi

ok "Built: $(du -h "$BINARY" | cut -f1) binary"

if $BUILD_ONLY; then
    ok "Build-only mode — done"
    exit 0
fi

# ── Deploy ─────────────────────────────────────────────
step "Deploying to $BATOCERA_HOST"

if ! ssh -o ConnectTimeout=5 "$BATOCERA_HOST" true 2>/dev/null; then
    fail "Cannot connect to $BATOCERA_HOST via SSH"
fi

# Create bin directory in theme
ssh "$BATOCERA_HOST" "mkdir -p $THEME_DIR/bin"
scp "$BINARY" "$BATOCERA_HOST:$THEME_DIR/bin/superkonna-overlay"
ssh "$BATOCERA_HOST" "chmod +x $THEME_DIR/bin/superkonna-overlay"
ok "Binary deployed to $THEME_DIR/bin/superkonna-overlay"

# ── RetroArch Configuration ───────────────────────────
if ! $NO_RA_CONFIG; then
    step "Configuring RetroArch to suppress native achievement visuals"

    RA_CONFIG="/userdata/system/retroarch.cfg"

    # Settings to suppress RA visual widgets while keeping data integration
    SETTINGS=(
        'cheevos_visibility_unlock = "false"'
        'cheevos_visibility_mastery = "false"'
        'cheevos_visibility_account = "false"'
        'cheevos_visibility_lboard_start = "false"'
        'cheevos_visibility_lboard_submit = "false"'
        'cheevos_visibility_lboard_cancel = "false"'
        'cheevos_visibility_lboard_trackers = "false"'
        'cheevos_visibility_progress_tracker = "false"'
        'cheevos_badges_enable = "false"'
        'cheevos_verbose_enable = "true"'
        'log_verbosity = "true"'
        'log_to_file = "true"'
        'log_to_file_timestamp = "false"'
    )

    for setting in "${SETTINGS[@]}"; do
        key="${setting%% =*}"
        # Remove existing line, then append
        ssh "$BATOCERA_HOST" "sed -i '/^${key} =/d' $RA_CONFIG 2>/dev/null; echo '$setting' >> $RA_CONFIG"
    done

    ok "RetroArch configured for overlay integration"
fi

# ── Autostart ──────────────────────────────────────────
step "Setting up autostart"

AUTOSTART_SCRIPT="$THEME_DIR/bin/start-overlay.sh"

ssh "$BATOCERA_HOST" "cat > $AUTOSTART_SCRIPT" << 'SCRIPT'
#!/bin/sh
# Start the superkonna overlay daemon.
# Called from custom.sh or ES init script.

THEME_DIR="/userdata/themes/es-theme-superkonna"
OVERLAY="$THEME_DIR/bin/superkonna-overlay"
LOG="/tmp/superkonna-overlay.log"

# Kill existing instance
pkill -f superkonna-overlay 2>/dev/null || true

# Wait for X11 to be ready
sleep 3

# Start daemon
if [ -x "$OVERLAY" ]; then
    SUPERKONNA_THEME_ROOT="$THEME_DIR" \
    DISPLAY=:0 \
    "$OVERLAY" /tmp/retroarch.log > "$LOG" 2>&1 &
    echo "Overlay started (PID: $!)"
else
    echo "Overlay binary not found: $OVERLAY"
fi
SCRIPT

ssh "$BATOCERA_HOST" "chmod +x $AUTOSTART_SCRIPT"

# Add to custom.sh if not already present
CUSTOM_SH="/userdata/system/custom.sh"
AUTOSTART_LINE="$THEME_DIR/bin/start-overlay.sh &"

ssh "$BATOCERA_HOST" "
    touch $CUSTOM_SH
    chmod +x $CUSTOM_SH
    if ! grep -q 'start-overlay.sh' $CUSTOM_SH; then
        echo '' >> $CUSTOM_SH
        echo '# Superkonna theme overlay daemon' >> $CUSTOM_SH
        echo '$AUTOSTART_LINE' >> $CUSTOM_SH
        echo 'Added to custom.sh'
    else
        echo 'Already in custom.sh'
    fi
"

ok "Autostart configured via custom.sh"

# ── Done ───────────────────────────────────────────────
echo ""
ok "Overlay deployment complete!"
echo "   Restart EmulationStation or reboot to activate."
echo "   Logs: ssh $BATOCERA_HOST cat /tmp/superkonna-overlay.log"
