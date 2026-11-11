# Building and releasing

## Build times

Ferrum uses the system's web engine, so there's no browser engine to compile: a first build takes a few minutes (about 3–6 on a recent machine, ~10 on a 2-core cloud VM) and needs ~2 GB of disk. After that, changes to Ferrum's own code rebuild in under a minute. Release builds use thin link-time optimisation and take a little longer; `scripts/build-macos.sh` builds twice (Apple Silicon + Intel).

On Linux, install the WebKitGTK development packages first:

```sh
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev
```

At run time Ferrum needs WebKitGTK 4.1 on Linux, WebView2 on Windows (built into Windows 10 and 11) and macOS 11 or newer.

## From a Mac: `scripts/build.sh`

```sh
scripts/build.sh macos | linux | windows | all
```

Everything lands in `dist/`.

### macOS → macOS (`build-macos.sh`)

Builds `aarch64-apple-darwin` and `x86_64-apple-darwin`, wraps each in its own `Ferrum.app` (with an `Info.plist` that also registers Ferrum for http/https links), signs them, and packages `Ferrum-<version>-macos-apple-silicon.dmg` and `Ferrum-<version>-macos-intel.dmg`. `ARCHS=aarch64-apple-darwin` builds just one.

- Needs Xcode command line tools and `brew install llvm cmake pkg-config`.
- `SIGN_IDENTITY="Developer ID Application: …"` signs with your certificate; otherwise it's ad-hoc signed (fine for your own machine; other Macs will need right-click → Open the first time).
- `MACOSX_DEPLOYMENT_TARGET` defaults to 11.0.
- Put an `assets/Ferrum.icns` in the repo to give the app an icon.

### macOS → Linux (`build-linux.sh`)

Runs the build inside a Debian container (`docker/Dockerfile.linux`), so the Mac needs only Docker Desktop, OrbStack or Colima. Builds `amd64` and `arm64` by default; choose with `ARCHS="arm64"`. The cargo registry and each architecture's `target/` live in named Docker volumes, so later builds are incremental.

On Apple Silicon, `arm64` is native; `amd64` runs under emulation and is several times slower.

### macOS → Windows (`build-windows.sh`)

Cross-compiles `x86_64-pc-windows-msvc` with [cargo-xwin](https://github.com/rust-cross/cargo-xwin), which fetches the MSVC runtime and Windows SDK and links with LLVM's `clang-cl`/`lld-link`.

- Needs `brew install llvm cmake ninja nasm` and `cargo install cargo-xwin` (the script installs it if missing).
- Running it accepts the Microsoft SDK licence (`XWIN_ACCEPT_LICENSE=1`).
- If a native dependency refuses to cross-compile, use the CI build below.

## CI: `.github/workflows/build.yml`

On every push to `main` and every pull request, GitHub Actions builds and tests natively on Linux x86_64, Linux arm64, macOS (Apple Silicon and Intel, via `build-macos.sh`) and Windows x86_64, and uploads each package as a workflow artifact.

### Versions

`Cargo.toml` holds the base version. In CI, each build job works out the release version *before* compiling: the base version, or the next free patch number if that version has already been released (0.5.0 → 0.5.1 → …). It's passed to the compiler as `FERRUM_VERSION`, so the app's About page, its file names and the release all show the same number. Bump the major/minor number in `Cargo.toml` for bigger changes. Local builds show `Cargo.toml`'s version.

### Releases

Every build is published as a versioned nightly release, and the newest is marked **Latest** (so `releases/latest` always points at it, and Ferrum's update check finds it):

- **Every push to `main`** publishes the next version (`Cargo.toml`'s version, or the next free patch number).
- **Pushing a version tag** publishes that version:

```sh
git tag v0.8.0
git push origin v0.8.0
```

Pull requests are built and tested but publish nothing.

### macOS signing

`build-macos.sh` signs with the hardened runtime and `assets/Ferrum.entitlements` (camera and microphone, for sites you allow). The app icon is generated from `assets/icon.png` with `sips` and `iconutil`.

## Tests

```sh
cargo test --bins
```

Unit tests cover address-bar parsing (`location.rs`), download file naming and detection (`downloads.rs`), block-page detection (`browser.rs`), ad-block rules (`adblock.rs`), tracker matching and list parsing (`blocker.rs`), article parsing (`reader.rs`), source highlighting (`devtools.rs`), bookmark files (`bookmarks_io.rs`) and version comparison (`updates.rs`).
