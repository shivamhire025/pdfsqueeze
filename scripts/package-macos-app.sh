#!/bin/bash
# Build a double-clickable macOS app from the machine we are running on:
#   dist/pdfsqueeze-macos-<arch>.app.zip   (arch = arm64 or x86_64)
#
# The app is the same program as the command-line package, with the bundled
# Ghostscript, qpdf and Tesseract inside it (Contents/Resources/engines).
# Needs Homebrew + `brew install ghostscript qpdf tesseract` on the BUILD machine
# only. The result is ad-hoc signed (free), not notarized: see README for the
# first-launch steps. macOS version floor = the build machine (engines/MIN_MACOS).
set -euo pipefail
cd "$(dirname "$0")/.."

ARCH="$(uname -m)"
NAME="pdfsqueeze-macos-$ARCH"
APP="dist/pdfsqueeze.app"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"

[ -x target/release/pdfsqueeze ] || cargo build --release

rm -rf "$APP" "dist/$NAME.app.zip" dist/AppIcon.iconset
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/pdfsqueeze "$APP/Contents/MacOS/pdfsqueeze"
./scripts/bundle-engines.sh "$APP/Contents/Resources/engines"
cp LICENSE README.md AGENTS.md CREDITS.md "$APP/Contents/Resources/" 2>/dev/null || true

# Icon: build AppIcon.icns from the padded 1024 px artwork.
mkdir -p dist/AppIcon.iconset
for s in 16 32 128 256 512; do
  sips -z $s $s assets/icon-mac.png --out "dist/AppIcon.iconset/icon_${s}x${s}.png" >/dev/null
  sips -z $((s*2)) $((s*2)) assets/icon-mac.png --out "dist/AppIcon.iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns dist/AppIcon.iconset -o "$APP/Contents/Resources/AppIcon.icns"
rm -rf dist/AppIcon.iconset

MIN="$(cat "$APP/Contents/Resources/engines/MIN_MACOS")"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>pdfsqueeze</string>
  <key>CFBundleDisplayName</key><string>pdfsqueeze</string>
  <key>CFBundleIdentifier</key><string>io.github.shivamhire025.pdfsqueeze</string>
  <key>CFBundleExecutable</key><string>pdfsqueeze</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>$MIN</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>pdfsqueeze is free software under the AGPL-3.0.</string>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP"          # free ad-hoc signature
codesign --verify --deep --strict "$APP"

ditto -c -k --sequesterRsrc --keepParent "$APP" "dist/$NAME.app.zip"
( cd dist && shasum -a 256 "$NAME.app.zip" > "$NAME.app.zip.sha256" )
echo "built dist/$NAME.app.zip ($(du -h "dist/$NAME.app.zip" | cut -f1)), needs macOS $MIN+"
