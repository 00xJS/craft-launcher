#!/usr/bin/env bash
# Regenerate every app icon from assets/app-icon/artcraft-launcher.svg (the canonical master) and
# the tray icons from assets/app-icon/tray/*.svg.
#
# Needs: resvg (brew install resvg / cargo install resvg). On macOS, iconutil also writes the
# .icns. The outputs are committed, so packaging (and the Rust build, which include_bytes!s some
# of them) never needs these tools.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/app-icon"
SVG="$DIR/artcraft-launcher.svg"
ID="ai.storyteller.artcraft-launcher"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

command -v resvg >/dev/null || { echo "error: resvg not found (brew install resvg)" >&2; exit 1; }

# The artwork is a 512-unit rounded tile (rx=112). macOS icons pad it to Apple's 824/1024 body
# grid (transparent margin). Windows and Linux icons use the whole tile with a hairline margin.
grep -q 'viewBox="0 0 512 512"' "$SVG" || { echo "error: expected viewBox=\"0 0 512 512\" in $SVG" >&2; exit 1; }
MAC="$TMP/mac.svg"
sed 's/viewBox="0 0 512 512"/viewBox="-62 -62 636 636"/' "$SVG" >"$MAC"
FLAT="$TMP/flat.svg"
sed 's/viewBox="0 0 512 512"/viewBox="-6 -6 524 524"/' "$SVG" >"$FLAT"

render() { resvg -w "$2" -h "$2" "$1" "$3" </dev/null; }

render "$MAC" 1024 "$DIR/artcraft-launcher-1024.png"

# Linux hicolor theme.
for s in 16 24 32 48 64 128 256 512; do
  mkdir -p "$DIR/hicolor/${s}x${s}/apps"
  render "$FLAT" "$s" "$DIR/hicolor/${s}x${s}/apps/$ID.png"
done
mkdir -p "$DIR/hicolor/scalable/apps"
cp "$SVG" "$DIR/hicolor/scalable/apps/$ID.svg"

# Windows .ico.
ICO_PNGS=()
for s in 16 20 24 32 40 48 64 128 256; do
  render "$FLAT" "$s" "$TMP/ico-$s.png"
  ICO_PNGS+=("$TMP/ico-$s.png")
done
(cd "$ROOT" && cargo run -q -p xtask -- ico "$DIR/artcraft-launcher.ico" "${ICO_PNGS[@]}")

# macOS .icns.
if command -v iconutil >/dev/null; then
  SET="$TMP/artcraft-launcher.iconset"
  mkdir -p "$SET"
  for s in 16 32 128 256 512; do
    render "$MAC" "$s" "$SET/icon_${s}x${s}.png"
    render "$MAC" $((s * 2)) "$SET/icon_${s}x${s}@2x.png"
  done
  iconutil -c icns -o "$DIR/artcraft-launcher.icns" "$SET"
else
  echo "warning: iconutil not found (macOS only); artcraft-launcher.icns not regenerated" >&2
fi

# Tray icons: a black template glyph for the macOS menu bar (44 px = 22 pt @2x) and a colour
# glyph for the Windows notification area and Linux/BSD status notifiers.
render "$DIR/tray/tray-template.svg" 44 "$DIR/tray/tray-template.png"
render "$DIR/tray/tray-color.svg" 64 "$DIR/tray/tray-color.png"
echo "icons written to $DIR"
