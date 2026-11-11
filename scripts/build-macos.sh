#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# macOS -> macOS: builds Ferrum.app for Apple Silicon and for Intel, and
# packages each as its own .dmg:
#   dist/Ferrum-<version>-macos-apple-silicon.dmg
#   dist/Ferrum-<version>-macos-intel.dmg
#
#   ARCHS=aarch64-apple-darwin scripts/build-macos.sh   # just one
#
# Needs: Xcode command line tools (xcode-select --install), Rust via rustup,
#        (Pages use the system's WebKit, so nothing else is needed.)
set -euo pipefail

cd "$(dirname "$0")/.."
[[ "$(uname)" == "Darwin" ]] || { echo "build-macos.sh must run on macOS"; exit 1; }

VERSION=${FERRUM_VERSION:-$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)}
DIST=dist
read -r -a ARCHS <<< "${ARCHS:-aarch64-apple-darwin x86_64-apple-darwin}"
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-11.0}

echo "==> Building Ferrum $VERSION for macOS (${ARCHS[*]})"
for target in "${ARCHS[@]}"; do
  rustup target add "$target" >/dev/null
  cargo build --release --target "$target"
done

ICONSET=$(mktemp -d)/Ferrum.iconset
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z $size $size assets/icon.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  sips -z $((size * 2)) $((size * 2)) assets/icon.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done

mkdir -p "$DIST"
for target in "${ARCHS[@]}"; do
case "$target" in
  aarch64-*) LABEL=apple-silicon ;;
  x86_64-*) LABEL=intel ;;
  *) LABEL=$target ;;
esac
APP="$DIST/$LABEL/Ferrum.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "target/$target/release/ferrum" "$APP/Contents/MacOS/ferrum"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Ferrum</string>
  <key>CFBundleDisplayName</key><string>Ferrum</string>
  <key>CFBundleIdentifier</key><string>io.github.p1e2r.ferrum</string>
  <key>CFBundleExecutable</key><string>ferrum</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>$MACOSX_DEPLOYMENT_TARGET</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSCameraUsageDescription</key><string>Websites you allow can use the camera.</string>
  <key>NSMicrophoneUsageDescription</key><string>Websites you allow can use the microphone.</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>CFBundleURLTypes</key>
  <array>
    <dict>
      <key>CFBundleURLName</key><string>Web site URL</string>
      <key>CFBundleURLSchemes</key><array><string>http</string><string>https</string></array>
    </dict>
  </array>
</dict>
</plist>
PLIST

iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Ferrum.icns"
/usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string Ferrum.icns" "$APP/Contents/Info.plist"

echo "==> Signing $LABEL (ad-hoc; set SIGN_IDENTITY to sign with a Developer ID)"
codesign --force --deep --options runtime \
  --entitlements assets/Ferrum.entitlements \
  --sign "${SIGN_IDENTITY:--}" "$APP"
codesign --verify --strict "$APP"

echo "==> Packaging .dmg"
DMG="$DIST/Ferrum-$VERSION-macos-$LABEL.dmg"
rm -f "$DMG"
STAGE=$(mktemp -d)
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "Ferrum" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"

echo "==> macOS ($LABEL): $DMG"
done
