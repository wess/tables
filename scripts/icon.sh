#!/usr/bin/env bash
# Regenerate the app icon: render the 1024 master PNG (scripts/icon.swift), build
# the macOS .iconset and compile it to assets/icon.icns, then write the 512px
# downscale used for Linux packaging. Run from anywhere; outputs land in
# assets/. Requires macOS (swift, sips, iconutil).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
assets="$root/assets"
png="$assets/icon.png"
icns="$assets/icon.icns"
png512="$assets/icon512.png"
mkdir -p "$assets"

echo "[icon] rendering master png"
swift "$root/scripts/icon.swift" "$png"

echo "[icon] building iconset"
set="$(mktemp -d)/icon.iconset"
mkdir -p "$set"
for spec in 16:16x16 32:16x16@2x 32:32x32 64:32x32@2x \
            128:128x128 256:128x128@2x 256:256x256 512:256x256@2x \
            512:512x512 1024:512x512@2x; do
  px="${spec%%:*}"
  name="${spec#*:}"
  sips -z "$px" "$px" "$png" --out "$set/icon_${name}.png" >/dev/null
done

echo "[icon] compiling icns"
iconutil -c icns "$set" -o "$icns"
rm -rf "$(dirname "$set")"

echo "[icon] writing 512px png for linux packaging"
sips -z 512 512 "$png" --out "$png512" >/dev/null

echo "[icon] wrote $png, $icns, and $png512"
