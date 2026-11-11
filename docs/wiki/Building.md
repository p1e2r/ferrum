# Building and releasing

## Build times

The first build compiles Servo (~800 crates) and downloads a prebuilt SpiderMonkey for the target. It needs ~8 GB RAM and ~20 GB disk.

> **Build time disclaimer.** The first build compiles all of Servo (~800 crates), so it is slow, and how slow depends on your machine:
>
> | Machine | First build (approx.) |
> |---|---|
> | Apple Silicon Mac (M1–M4, 8+ cores, 16 GB) | 15–25 min |
> | Recent 8-core Windows/Linux desktop, 16 GB | 20–30 min |
> | 4-core laptop, 8 GB | 45–60+ min |
> | 2-core / 7 GB cloud VM (measured: debug build) | ~23 min debug, ~31 min after a dependency change |
>
> Only the last row was measured; the others are estimates. Release builds (`--release`, with link-time optimisation) take longer than debug builds, and `scripts/build-macos.sh` builds twice (Apple Silicon + Intel). With less than 8 GB of RAM, use `cargo build -j2` to avoid running out of memory. After the first build, changes to Ferrum's own code rebuild in about a minute.

`Cargo.toml` sets `opt-level = 2` for dependencies even in debug builds, because an unoptimised Servo is too slow to browse with.

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

Cross-compiles `x86_64-pc-windows-msvc` with [cargo-xwin](https://github.com/rust-cross/cargo-xwin), which fetches the MSVC runtime and Windows SDK and links with LLVM's `clang-cl`/`lld-link`. SpiderMonkey comes prebuilt for Windows, so it isn't cross-compiled.

- Needs `brew install llvm cmake ninja nasm` and `cargo install cargo-xwin` (the script installs it if missing).
- Running it accepts the Microsoft SDK licence (`XWIN_ACCEPT_LICENSE=1`).
- This is the least common route for a project of Servo's size. If a native dependency refuses to cross-compile, use the CI build below.

## CI: `.github/workflows/build.yml`

On every push to `main` and every pull request, GitHub Actions builds and tests natively on Linux x86_64, Linux arm64, macOS (Apple Silicon and Intel, via `build-macos.sh`) and Windows x86_64, and uploads each package as a workflow artifact.

### Versions

`Cargo.toml` holds the base version. In CI, each build job works out the release version *before* compiling: the base version, or the next free patch number if that version has already been released (0.5.0 → 0.5.1 → …). It's passed to the compiler as `FERRUM_VERSION`, so the app's About page, its file names and the release all show the same number. Bump the major/minor number in `Cargo.toml` for bigger changes. Local builds show `Cargo.toml`'s version.

### Releases

Every release is a **pre-release** until Ferrum is ready for production:

- **Every push to `main`** replaces the **Nightly** pre-release (tag `nightly`) with fresh builds for all platforms.
- **Pushing a version tag** publishes a pre-release for that version:

```sh
git tag v0.2.0
git push origin v0.2.0
```

Pull requests are built and tested but publish nothing.

### macOS signing and the JIT

`build-macos.sh` signs with the hardened runtime and `assets/Ferrum.entitlements`. The entitlements `com.apple.security.cs.allow-jit` and `allow-unsigned-executable-memory` are required: without them SpiderMonkey can't allocate executable memory for its JIT, `JSEngine::init` fails with `InitFailed`, and Ferrum crashes at launch. The app icon is generated from `assets/icon.png` with `sips` and `iconutil`.

## Tests

```sh
cargo test --bins
```

Unit tests cover address-bar parsing (`location.rs`), download file naming and detection (`downloads.rs`), the keyboard name mapping (`keys.rs`), PDF output (`print.rs`), tracker matching and list parsing (`blocker.rs`), article parsing (`reader.rs`), console formatting and source highlighting (`devtools.rs`), bookmark files (`bookmarks_io.rs`) and version comparison (`updates.rs`).
