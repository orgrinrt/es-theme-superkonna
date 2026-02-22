#!/bin/bash
# =============================================================================
# Superkonna Theme — Dynamic Button Hint Generator
# =============================================================================
# Parses es_input.cfg to detect connected controller type and generates
# per-view hint strip SVGs with correct controller-specific button icons
# from the Kenney Input Prompts pack (CC0).
#
# Usage:
#   ./update-hints.sh [--theme-dir /path/to/theme] [--style xbox|playstation|steamdeck|switch]
#
# Install as ES hook:
#   ln -s /path/to/theme/scripts/update-hints.sh \
#     /userdata/system/configs/emulationstation/scripts/controls-changed/
#   ln -s /path/to/theme/scripts/update-hints.sh \
#     /userdata/system/configs/emulationstation/scripts/controller-connected/
# =============================================================================

set -euo pipefail

# ─── Configuration ───────────────────────────────────────────────────────────

THEME_DIR="$(cd "$(dirname "$0")/.." && pwd)"
STYLE="auto"
ES_INPUT_CFG="/userdata/system/configs/emulationstation/es_input.cfg"
OUTPUT_DIR="${THEME_DIR}/generated"
BUTTONS_DIR="${THEME_DIR}/assets/buttons"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --theme-dir) THEME_DIR="$2"; OUTPUT_DIR="${THEME_DIR}/generated"; BUTTONS_DIR="${THEME_DIR}/assets/buttons"; shift 2 ;;
    --style)     STYLE="$2"; shift 2 ;;
    --es-input)  ES_INPUT_CFG="$2"; shift 2 ;;
    *) shift ;;
  esac
done

mkdir -p "${OUTPUT_DIR}"

# ─── Controller Detection ───────────────────────────────────────────────────
# Auto-detects controller family from es_input.cfg device names/GUIDs.
# Returns: xbox | playstation | steamdeck | switch

detect_style() {
  if [[ "${STYLE}" != "auto" ]]; then
    echo "${STYLE}"
    return
  fi

  if [[ ! -f "${ES_INPUT_CFG}" ]]; then
    echo "xbox"
    return
  fi

  local name
  name=$(grep -oi 'deviceName="[^"]*"' "${ES_INPUT_CFG}" | tail -1 | sed 's/deviceName="\(.*\)"/\1/' || true)
  name=$(echo "${name}" | tr '[:upper:]' '[:lower:]')

  if echo "${name}" | grep -qE 'playstation|dualshock|dualsense|ps[345]|sony'; then
    echo "playstation"
  elif echo "${name}" | grep -qE 'steam.?deck|valve.?deck'; then
    echo "steamdeck"
  elif echo "${name}" | grep -qE 'nintendo|switch|pro.controller|joy.?con'; then
    echo "switch"
  else
    echo "xbox"
  fi
}

# ─── Icon Mapping ────────────────────────────────────────────────────────────
# Maps abstract button names (a, b, x, y, lb, rb, start, select, dpad_up...)
# to Kenney filenames per controller family.
#
# ES uses SDL Xbox convention internally:
#   a = bottom face button (confirm)
#   b = right face button (back)
#   x = left face button
#   y = top face button

icon_file() {
  local button="$1"
  local style="$2"

  case "${style}" in
    xbox)
      case "${button}" in
        a)          echo "xbox/xbox_button_a.svg" ;;
        b)          echo "xbox/xbox_button_b.svg" ;;
        x)          echo "xbox/xbox_button_x.svg" ;;
        y)          echo "xbox/xbox_button_y.svg" ;;
        lb)         echo "xbox/xbox_lb.svg" ;;
        rb)         echo "xbox/xbox_rb.svg" ;;
        lt)         echo "xbox/xbox_lt.svg" ;;
        rt)         echo "xbox/xbox_rt.svg" ;;
        start)      echo "xbox/xbox_button_menu.svg" ;;
        select)     echo "xbox/xbox_button_view.svg" ;;
        dpad_up)    echo "xbox/xbox_dpad_up.svg" ;;
        dpad_down)  echo "xbox/xbox_dpad_down.svg" ;;
        dpad_left)  echo "xbox/xbox_dpad_left.svg" ;;
        dpad_right) echo "xbox/xbox_dpad_right.svg" ;;
        *)          echo "generic/generic_button.svg" ;;
      esac
      ;;

    playstation)
      case "${button}" in
        a)          echo "playstation/playstation_button_cross.svg" ;;
        b)          echo "playstation/playstation_button_circle.svg" ;;
        x)          echo "playstation/playstation_button_square.svg" ;;
        y)          echo "playstation/playstation_button_triangle.svg" ;;
        lb)         echo "playstation/playstation_trigger_l1.svg" ;;
        rb)         echo "playstation/playstation_trigger_r1.svg" ;;
        lt)         echo "playstation/playstation_trigger_l2.svg" ;;
        rt)         echo "playstation/playstation_trigger_r2.svg" ;;
        start)      echo "playstation/playstation5_button_options.svg" ;;
        select)     echo "playstation/playstation5_button_create.svg" ;;
        dpad_up)    echo "playstation/playstation_dpad_up.svg" ;;
        dpad_down)  echo "playstation/playstation_dpad_down.svg" ;;
        dpad_left)  echo "playstation/playstation_dpad_left.svg" ;;
        dpad_right) echo "playstation/playstation_dpad_right.svg" ;;
        *)          echo "generic/generic_button.svg" ;;
      esac
      ;;

    steamdeck)
      case "${button}" in
        a)          echo "steamdeck/steamdeck_button_a.svg" ;;
        b)          echo "steamdeck/steamdeck_button_b.svg" ;;
        x)          echo "steamdeck/steamdeck_button_x.svg" ;;
        y)          echo "steamdeck/steamdeck_button_y.svg" ;;
        lb)         echo "steamdeck/steamdeck_button_l1.svg" ;;
        rb)         echo "steamdeck/steamdeck_button_r1.svg" ;;
        lt)         echo "steamdeck/steamdeck_button_l2.svg" ;;
        rt)         echo "steamdeck/steamdeck_button_r2.svg" ;;
        start)      echo "steamdeck/steamdeck_button_options.svg" ;;
        select)     echo "steamdeck/steamdeck_button_view.svg" ;;
        dpad_up)    echo "steamdeck/steamdeck_dpad_up.svg" ;;
        dpad_down)  echo "steamdeck/steamdeck_dpad_down.svg" ;;
        dpad_left)  echo "steamdeck/steamdeck_dpad_left.svg" ;;
        dpad_right) echo "steamdeck/steamdeck_dpad_right.svg" ;;
        *)          echo "generic/generic_button.svg" ;;
      esac
      ;;

    switch)
      case "${button}" in
        # Note: Nintendo uses B=right (confirm in JP), A=bottom.
        # ES SDL maps: a=bottom, b=right regardless. We show the
        # physical button that ES "a" maps to on Switch.
        a)          echo "switch/switch_button_b.svg" ;;
        b)          echo "switch/switch_button_a.svg" ;;
        x)          echo "switch/switch_button_y.svg" ;;
        y)          echo "switch/switch_button_x.svg" ;;
        lb)         echo "switch/switch_button_l.svg" ;;
        rb)         echo "switch/switch_button_r.svg" ;;
        lt)         echo "switch/switch_button_zl.svg" ;;
        rt)         echo "switch/switch_button_zr.svg" ;;
        start)      echo "switch/switch_button_plus.svg" ;;
        select)     echo "switch/switch_button_minus.svg" ;;
        dpad_up)    echo "switch/switch_dpad_up.svg" ;;
        dpad_down)  echo "switch/switch_dpad_down.svg" ;;
        dpad_left)  echo "switch/switch_dpad_left.svg" ;;
        dpad_right) echo "switch/switch_dpad_right.svg" ;;
        *)          echo "generic/generic_button.svg" ;;
      esac
      ;;

    *)
      echo "generic/generic_button.svg"
      ;;
  esac
}

# ─── SVG Hint Strip Generator ───────────────────────────────────────────────
# Generates a composite SVG with inline icon+label pairs.
# Args: output_file label1 button1 label2 button2 ...

generate_hint_strip() {
  local output="$1"
  shift

  local style
  style=$(detect_style)

  local pair_count=$(( $# / 2 ))
  local pair_width=130
  local icon_size=26
  local gap=8
  local padding=12
  local total_width=$(( pair_count * pair_width + padding * 2 ))
  local height=38

  cat > "${output}" <<EOF
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 ${total_width} ${height}">
  <style>
    text { font-family: -apple-system, 'Segoe UI', Roboto, sans-serif; font-size: 13px; fill: #FFFFFF; dominant-baseline: central; }
  </style>
EOF

  local x=${padding}
  while [[ $# -ge 2 ]]; do
    local label="$1"
    local button="$2"
    shift 2

    local rel_icon
    rel_icon=$(icon_file "${button}" "${style}")
    local abs_icon="${BUTTONS_DIR}/${rel_icon}"

    local icon_y=$(( (height - icon_size) / 2 ))

    if [[ -f "${abs_icon}" ]]; then
      # Read the source SVG and extract its viewBox for proper scaling
      local vbox
      vbox=$(grep -o 'viewBox="[^"]*"' "${abs_icon}" | head -1 | sed 's/viewBox="\(.*\)"/\1/' || echo "0 0 100 100")

      # Embed as nested <svg> with the original viewBox for correct scaling
      echo "  <svg x=\"${x}\" y=\"${icon_y}\" width=\"${icon_size}\" height=\"${icon_size}\" viewBox=\"${vbox}\">" >> "${output}"
      # Strip the outer <svg> wrapper and </svg>, keep inner content
      sed -e '1s/.*<svg[^>]*>//' -e '$s/<\/svg>//' "${abs_icon}" >> "${output}"
      echo "  </svg>" >> "${output}"
    fi

    local text_x=$(( x + icon_size + gap ))
    local text_y=$(( height / 2 ))
    echo "  <text x=\"${text_x}\" y=\"${text_y}\">${label}</text>" >> "${output}"

    x=$(( x + pair_width ))
  done

  echo "</svg>" >> "${output}"
}

# ─── Per-View Hint Definitions ──────────────────────────────────────────────

generate_all_hints() {
  # System view (carousel): menu/settings left, browse/random right
  generate_hint_strip "${OUTPUT_DIR}/hints-system-left.svg" \
    "Menu" "start" \
    "Settings" "select"

  generate_hint_strip "${OUTPUT_DIR}/hints-system-right.svg" \
    "Open" "a" \
    "Random" "y"

  # Game list views (basic, detailed, grid, video, flix)
  generate_hint_strip "${OUTPUT_DIR}/hints-gamelist-left.svg" \
    "Back" "b" \
    "Menu" "start"

  generate_hint_strip "${OUTPUT_DIR}/hints-gamelist-right.svg" \
    "Launch" "a" \
    "Favorite" "y" \
    "Options" "x"
}

# ─── Main ────────────────────────────────────────────────────────────────────

echo "[superkonna] Generating button hints..."
echo "[superkonna] Theme dir: ${THEME_DIR}"
echo "[superkonna] Detected style: $(detect_style)"

generate_all_hints

echo "[superkonna] Hints written to ${OUTPUT_DIR}/"
