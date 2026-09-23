#!/usr/bin/env bash
# Renders assets/AppIcon.icns (menu-bar app icon) with ImageMagick + iconutil.
# Only needed when changing the icon; the generated .icns is committed.
set -euo pipefail
cd "$(dirname "$0")/.."

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# 1024 px canvas, 824 px body with ~185 px corners (macOS icon grid).
magick -size 1024x1024 gradient:'#7B6CFF-#3B2FC9' -alpha set \
  \( -size 1024x1024 xc:none -fill white -draw "roundrectangle 100,100 923,923 185,185" \) \
  -compose DstIn -composite \
  -fill white -stroke none -draw "roundrectangle 417,240 606,580 95,95" \
  -fill none -stroke white -strokewidth 46 -draw "stroke-linecap round path 'M 327,470 A 185,185 0 0,0 697,470'" \
  -draw "stroke-linecap round line 512,655 512,760" \
  -draw "stroke-linecap round line 417,770 607,770" \
  "$WORK/icon-1024.png"

ICONSET="$WORK/AppIcon.iconset"
mkdir "$ICONSET"
for s in 16 32 128 256 512; do
  magick "$WORK/icon-1024.png" -resize ${s}x${s} "$ICONSET/icon_${s}x${s}.png"
  magick "$WORK/icon-1024.png" -resize $((s*2))x$((s*2)) "$ICONSET/icon_${s}x${s}@2x.png"
done
mkdir -p assets
iconutil -c icns "$ICONSET" -o assets/AppIcon.icns
echo "Wrote assets/AppIcon.icns"
