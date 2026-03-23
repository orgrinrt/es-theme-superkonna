#!/bin/bash
# =============================================================================
# Superkonna Theme — Card Color Generator
# =============================================================================
# Generates tiny per-system colored PNGs from metadata XMLs.
# Each system gets a 64x64 solid-color PNG used as the card background
# in the carousel itemTemplate.
#
# Usage:
#   ./generate-card-colors.sh [--theme-dir /path]
# =============================================================================

set -euo pipefail

THEME_DIR="$(cd "$(dirname "$0")/.." && pwd)"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --theme-dir) THEME_DIR="$2"; shift 2 ;;
    *) shift ;;
  esac
done

METADATA_DIR="$THEME_DIR/_inc/metadata"
CARDS_DIR="$THEME_DIR/assets/cards"
mkdir -p "$CARDS_DIR"

if [ ! -d "$METADATA_DIR" ]; then
  echo "[superkonna] No metadata dir at $METADATA_DIR"
  exit 0
fi

# Get default color from _default.xml
default_color="1E2840"
if [ -f "$METADATA_DIR/_default.xml" ]; then
  c=$(grep -o 'cardSystemColor>[^<]*' "$METADATA_DIR/_default.xml" | head -1 | sed 's/cardSystemColor>//')
  [ -n "$c" ] && default_color="$c"
fi

count=0

for xml in "$METADATA_DIR"/*.xml; do
  [ -f "$xml" ] || continue
  name="$(basename "$xml" .xml)"
  [ "$name" = "_default" ] && continue

  # Extract color from XML
  color=$(grep -o 'cardSystemColor>[^<]*' "$xml" | head -1 | sed 's/cardSystemColor>//')
  [ -z "$color" ] && color="$default_color"

  dest="$CARDS_DIR/${name}.png"

  # Generate 64x64 solid color PNG using python3 (pure, no PIL needed)
  python3 -c "
import struct, zlib
r,g,b = int('$color'[0:2],16), int('$color'[2:4],16), int('$color'[4:6],16)
w,h = 64,64
raw = b''
for y in range(h):
    raw += b'\x00'
    for x in range(w):
        raw += struct.pack('BBBB', r, g, b, 255)
def chunk(t,d):
    c = t+d
    return struct.pack('>I',len(d)) + c + struct.pack('>I',zlib.crc32(c)&0xffffffff)
with open('$dest','wb') as f:
    f.write(b'\x89PNG\r\n\x1a\n')
    f.write(chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,6,0,0,0)))
    f.write(chunk(b'IDAT',zlib.compress(raw)))
    f.write(chunk(b'IEND',b''))
" 2>/dev/null && count=$((count + 1))
done

# Also generate default card
python3 -c "
import struct, zlib
r,g,b = int('$default_color'[0:2],16), int('$default_color'[2:4],16), int('$default_color'[4:6],16)
w,h = 64,64
raw = b''
for y in range(h):
    raw += b'\x00'
    for x in range(w):
        raw += struct.pack('BBBB', r, g, b, 255)
def chunk(t,d):
    c = t+d
    return struct.pack('>I',len(d)) + c + struct.pack('>I',zlib.crc32(c)&0xffffffff)
with open('$CARDS_DIR/_default.png','wb') as f:
    f.write(b'\x89PNG\r\n\x1a\n')
    f.write(chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,6,0,0,0)))
    f.write(chunk(b'IDAT',zlib.compress(raw)))
    f.write(chunk(b'IEND',b''))
" 2>/dev/null

echo "[superkonna] Generated $count per-system card color PNGs + default"
