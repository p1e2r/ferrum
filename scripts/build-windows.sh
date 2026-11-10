#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# macOS -> Windows: cross-compiles an x86_64 Windows build with cargo-xwin,
# which downloads the Microsoft C runtime and Windows SDK headers and links
# with LLVM's MSVC-compatible tools. SpiderMonkey comes prebuilt for Windows.
#
# Needs: brew install llvm cmake ninja nasm
#        cargo install cargo-xwin
#
# By running this you accept the Microsoft Visual Studio licence terms for the
# SDK files that cargo-xwin downloads.
#
# Cross-compiling a whole browser engine is the least-trodden path here. If it
# fails on your machine, the GitHub Actions workflow (.github/workflows/build.yml)
# builds Windows natively on every push.
set -euo pipefail

cd "$(dirname "$0")/.."
TARGET=x86_64-pc-windows-msvc
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)

command -v cargo-xwin >/dev/null || { echo "Installing cargo-xwin"; cargo install cargo-xwin --locked; }
rustup target add "$TARGET" >/dev/null

# Use Homebrew LLVM's clang-cl / lld-link rather than Apple's clang.
if command -v brew >/dev/null; then
  LLVM="$(brew --prefix llvm)"
  export PATH="$LLVM/bin:$PATH"
  export LIBCLANG_PATH="${LIBCLANG_PATH:-$LLVM/lib}"
fi
for tool in clang-cl lld-link llvm-lib cmake nasm; do
  command -v "$tool" >/dev/null || { echo "Missing $tool — run: brew install llvm cmake ninja nasm"; exit 1; }
done

export XWIN_ACCEPT_LICENSE=1
export CMAKE_GENERATOR=Ninja

echo "==> Cross-compiling Ferrum $VERSION for Windows ($TARGET)"
cargo xwin build --release --target "$TARGET"

mkdir -p dist
STAGE=$(mktemp -d)
mkdir -p "$STAGE/Ferrum"
cp "target/$TARGET/release/ferrum.exe" README.md LICENSE "$STAGE/Ferrum/"
# Ship any DLLs the build produced next to the exe.
find "target/$TARGET/release" -maxdepth 1 -name '*.dll' -exec cp {} "$STAGE/Ferrum/" \;
OUT="dist/ferrum-$VERSION-windows-x86_64.zip"
rm -f "$OUT"
(cd "$STAGE" && zip -qr "$OLDPWD/$OUT" Ferrum)
rm -rf "$STAGE"

echo "==> Windows: $OUT"
