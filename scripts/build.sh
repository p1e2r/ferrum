#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Build Ferrum for one or more platforms from a Mac (or Linux host).
#
#   scripts/build.sh macos      # Apple Silicon and Intel .dmg files
#   scripts/build.sh linux      # x86_64 + arm64 tarballs, built in Docker
#   scripts/build.sh windows    # x86_64 .exe zip, cross-compiled with cargo-xwin
#   scripts/build.sh all        # everything above
#
# Output goes to dist/.
set -euo pipefail

cd "$(dirname "$0")/.."
targets=("${@:-all}")

run() {
  case "$1" in
    macos)   scripts/build-macos.sh ;;
    linux)   scripts/build-linux.sh ;;
    windows) scripts/build-windows.sh ;;
    all)     scripts/build-macos.sh; scripts/build-linux.sh; scripts/build-windows.sh ;;
    *) echo "Unknown target '$1' (use macos, linux, windows or all)"; exit 1 ;;
  esac
}

for t in "${targets[@]}"; do run "$t"; done
echo
echo "Done. Builds are in $(pwd)/dist:"
ls -lh dist
