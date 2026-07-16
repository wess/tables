#!/usr/bin/env bash
# Package dist/Tables.app into dist/Tables.dmg — a compressed disk image with the
# app and a /Applications symlink for drag-to-install. Run scripts/bundle.sh
# first. Usage: scripts/dmg.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

app_name="Tables"
app="dist/$app_name.app"
dmg="dist/$app_name.dmg"
[ -d "$app" ] || { echo "error: $app not found — run scripts/bundle.sh first" >&2; exit 1; }

stage="$(mktemp -d)"
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"

rm -f "$dmg"
echo "[dmg] building $dmg"
hdiutil create \
  -volname "$app_name" \
  -srcfolder "$stage" \
  -fs HFS+ \
  -format UDZO \
  -ov \
  "$dmg" >/dev/null
rm -rf "$stage"

# Sign the image too when a real identity is available (notarization staples
# onto the .dmg in CI).
if [ "${CODESIGN_IDENTITY:--}" != "-" ]; then
  codesign --force --timestamp -s "$CODESIGN_IDENTITY" "$dmg"
fi
echo "[dmg] -> $dmg"
