# Building and releasing

## Build times

The first build compiles Servo (~800 crates) and downloads a prebuilt SpiderMonkey for the target: expect 20–60 minutes, ~8 GB RAM and ~20 GB disk. Afterwards, changing Ferrum's own code rebuilds in under a minute.

`Cargo.toml` sets `opt-level = 2` for dependencies even in debug builds, because an unoptimised Servo is too slow to browse with.

## From a Mac: `scripts/build.sh`

```sh
scripts/build.sh macos | linux | windows | all
```

Everything lands in `dist/`.

### macOS → macOS (`build-macos.sh`)

Builds `aarch64-apple-darwin` and `x86_64-apple-darwin`, joins them with `lipo` into one universal binary, wraps it in `Ferrum.app` (with an `Info.plist` that also registers Ferrum for http/https links), signs it, and packages `Ferrum-<version>-macos-universal.dmg`.

- Needs Xcode command line tools and `brew install llvm cmake pkg-config`.
- `SIGN_IDENTITY="Developer ID Application: …"` signs with your certificate; otherwise it's ad-hoc signed (fine for your own machine; other Macs will need right-click → Open the first time).
- `MACOSX_DEPLOYMENT_TARGET` defaults to 11.0.
- Put an `assets/Ferrum.icns` in the repo to give the app an icon.

### macOS → Linux (`build-linux.sh`)

Runs the build inside a Debian container (`docker/Dockerfile.linux`), so the Mac needs only Docker Desktop, OrbStack or Colima. Builds `amd64` and `arm64` by default; choose with `ARCHS="arm64"`. The cargo registry and each architecture's `target/` live in named Docker volumes, so later builds are incremental.

On Apple Silicon, `arm64` is native; `amd64` runs under emulation and is several times slower.

### macOS → Windows (`build-windows.sh`)

Cross-compiles `x86_64-pc-windows-msvc` with [cargo-xwin](https://github.com/rust-cross/cargo-xwin), which fetches the MSVC runtime and Windows SDK and links with LLVM's `clang-cl`/`lld-link`. SpiderMonkey comes prebuilt for Windows, so it isn't cross-compiled.

- Needs `brew install llvm cmake ninja nasm` and `cargo install cargo-xwin` (the script installs it if missing).
- Running it accepts the Microsoft SDK licence (`XWIN_ACCEPT_LICENSE=1`).
- This is the least common route for a project of Servo's size. If a native dependency refuses to cross-compile, use the CI build below.

## CI: `.github/workflows/build.yml`

On every push to `main` and every pull request, GitHub Actions builds and tests natively on Linux x86_64, Linux arm64, macOS (universal, via `build-macos.sh`) and Windows x86_64, and uploads each package as a workflow artifact.

Pushing a version tag publishes a release with all packages attached:

```sh
git tag v0.2.0
git push origin v0.2.0
```

## Tests

```sh
cargo test --bins
```

Unit tests cover address-bar parsing (`location.rs`), download file naming and detection (`downloads.rs`) and the keyboard name mapping (`keys.rs`).
