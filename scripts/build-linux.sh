#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# macOS -> Linux: builds inside a Debian container with Docker (or OrbStack /
# Colima), so no Linux toolchain is needed on the Mac.
#
#   scripts/build-linux.sh              # x86_64 and arm64
#   ARCHS=arm64 scripts/build-linux.sh  # just one
#
# On Apple Silicon the arm64 build is native and fast; the x86_64 build runs
# under emulation and is several times slower. The cargo cache is kept in
# Docker volumes, so rebuilds only recompile what changed.
set -euo pipefail

cd "$(dirname "$0")/.."
command -v docker >/dev/null || { echo "Docker is required (Docker Desktop, OrbStack or Colima)"; exit 1; }

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
read -r -a ARCHS <<< "${ARCHS:-amd64 arm64}"
mkdir -p dist

for arch in "${ARCHS[@]}"; do
  image="ferrum-build-linux:$arch"
  echo "==> Preparing Linux build image ($arch)"
  docker build --platform "linux/$arch" -t "$image" -f docker/Dockerfile.linux docker

  echo "==> Building Ferrum $VERSION for Linux ($arch)"
  docker run --rm --platform "linux/$arch" \
    -v "$(pwd)":/src \
    -v "ferrum-cargo-registry:/usr/local/cargo/registry" \
    -v "ferrum-target-$arch:/src/target" \
    -w /src \
    "$image" \
    bash -c "cargo build --release && \
      mkdir -p /tmp/pkg/ferrum && \
      cp target/release/ferrum README.md LICENSE /tmp/pkg/ferrum/ && \
      tar -C /tmp/pkg -czf /src/dist/ferrum-$VERSION-linux-$arch.tar.gz ferrum"

  echo "==> Linux ($arch): dist/ferrum-$VERSION-linux-$arch.tar.gz"
done
