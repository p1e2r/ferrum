#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# macOS -> macOS: builds Apple Silicon and Intel binaries, merges them into one
# universal Ferrum.app, and packages it as a .dmg.
#
# Needs: Xcode command line tools (xcode-select --install), Rust via rustup,
#        Homebrew packages: brew install llvm cmake python pkg-config
set -euo pipefail

cd "$(dirname "$0")/.."
[[ "$(uname)" == "Darwin" ]] || { echo "build-macos.sh must run on macOS"; exit 1; }

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
DIST=dist
APP="$DIST/Ferrum.app"
ARCHS=(aarch64-apple-darwin x86_64-apple-darwin)
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-11.0}

# Homebrew's LLVM is needed by Servo's bindgen step.
if [[ -z "${LIBCLANG_PATH:-}" ]] && command -v brew >/dev/null; then
  export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
fi

echo "==> Building Ferrum $VERSION for macOS (${ARCHS[*]})"
for target in "${ARCHS[@]}"; do
  rustup target add "$target" >/dev/null
  cargo build --release --target "$target"
done

mkdir -p "$DIST"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

echo "==> Creating universal binary"
lipo -create \
  "target/aarch64-apple-darwin/release/ferrum" \
  "target/x86_64-apple-darwin/release/ferrum" \
  -output "$APP/Contents/MacOS/ferrum"

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

if [[ -f assets/Ferrum.icns ]]; then
  cp assets/Ferrum.icns "$APP/Contents/Resources/"
  /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string Ferrum.icns" "$APP/Contents/Info.plist"
fi

echo "==> Signing (ad-hoc; set SIGN_IDENTITY to sign with a Developer ID)"
codesign --force --deep --options runtime --sign "${SIGN_IDENTITY:--}" "$APP"

echo "==> Packaging .dmg"
DMG="$DIST/Ferrum-$VERSION-macos-universal.dmg"
rm -f "$DMG"
STAGE=$(mktemp -d)
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "Ferrum" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"

echo "==> macOS: $DMG"
