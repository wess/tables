#!/usr/bin/env bash
# Build Tables (release) and assemble dist/Tables.app. The binary is the `app`
# bin from crates/app; the icon comes from assets/icon.icns; the version is read
# from the workspace Cargo.toml. Codesigns with CODESIGN_IDENTITY if set (a real
# Developer ID for a notarizable build), otherwise ad-hoc ("-") so it still runs
# locally. Usage: scripts/bundle.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

app_name="Tables"
# The cargo bin target is `tablesdev`; the shipped executable is `tables`.
src_bin="tablesdev"
bin_name="tables"
bundle_id="dev.tables.app"
identity="${CODESIGN_IDENTITY:--}"

version="$(sed -n 's/^version = "\([0-9][^"]*\)".*/\1/p' Cargo.toml | head -1)"
[ -n "$version" ] || { echo "error: could not read version from Cargo.toml" >&2; exit 1; }
echo "[bundle] $app_name $version"

# The icon should exist in the repo; regenerate it if missing (macOS only).
if [ ! -f assets/icon.icns ]; then
  echo "[bundle] assets/icon.icns missing — generating"
  scripts/icon.sh
fi

echo "[bundle] cargo build --release -p app"
cargo build --release -p app

app="dist/$app_name.app"
contents="$app/Contents"
rm -rf "$app"
mkdir -p "$contents/MacOS" "$contents/Resources"

cp "target/release/$src_bin" "$contents/MacOS/$bin_name"
cp assets/icon.icns "$contents/Resources/icon.icns"

cat > "$contents/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>$app_name</string>
	<key>CFBundleDisplayName</key>
	<string>$app_name</string>
	<key>CFBundleIdentifier</key>
	<string>$bundle_id</string>
	<key>CFBundleExecutable</key>
	<string>$bin_name</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleVersion</key>
	<string>$version</string>
	<key>CFBundleShortVersionString</key>
	<string>$version</string>
	<key>CFBundleIconFile</key>
	<string>icon</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.developer-tools</string>
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
PLIST

# Sign inside-out: the executable with hardened runtime + entitlements, then the
# bundle. Ad-hoc ("-") still seals the bundle so it launches on the build host.
echo "[bundle] codesign ($identity)"
runtime_opts=()
[ "$identity" != "-" ] && runtime_opts=(--options runtime --timestamp)
codesign --force ${runtime_opts[@]+"${runtime_opts[@]}"} \
  --entitlements assets/tables.entitlements \
  -s "$identity" "$contents/MacOS/$bin_name"
codesign --force ${runtime_opts[@]+"${runtime_opts[@]}"} \
  --entitlements assets/tables.entitlements \
  -s "$identity" "$app"

codesign --verify --strict --verbose=2 "$app" || true
echo "[bundle] -> $app"
