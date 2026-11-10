<img src="assets/icon-256.png" width="96" align="right" alt="Ferrum icon">

# Ferrum

A fast, native desktop web browser written in Rust.

- **Engine:** [Servo](https://servo.org), the Rust browser engine that Firefox's Stylo (CSS) and WebRender (GPU painting) came from.
- **Interface:** tabs, toolbar, address bar, menus, side panels and page dialogs are all drawn natively in Rust with [egui](https://github.com/emilk/egui), on the same OpenGL surface as the page. The app itself contains no HTML, CSS or JavaScript.
- **Platforms:** macOS, Windows, Linux.
- **License:** MIT.

## Features

| | |
|---|---|
| Browsing | Tabs (favicons, middle-click close, right-click: reload, duplicate, close others / to the right), back/forward, reload, home, address bar that searches or navigates; automatic retry and HTTPS→HTTP fallback when a site won't connect |
| 3D and web features | WebGL and WebGL 2 (three.js etc.), `crypto.subtle`, Brotli |
| Address bar | Suggestions from bookmarks and history (↑/↓ to pick), selects all on click, choice of 6 search engines |
| Bookmarks | Star button / Ctrl+D, bookmarks bar, bookmarks panel with reordering, start-page tiles |
| History | Searchable history panel (fast even with thousands of entries), frequently-visited tiles, clear history |
| Downloads | File links download automatically, "Save link/image as…", progress, cancel, open, show in folder; uses the site's cookies |
| Page dialogs | `alert` / `confirm` / `prompt`, HTTP sign-in, permission prompts (camera, location, notifications…) |
| Forms | Native `<select>` dropdowns (incl. multi-select and option groups), colour picker, system file picker |
| Context menu | Right-click: open link in new tab, copy link, save link/image, back/forward/reload, cut/copy/paste |
| View | Zoom per tab (Ctrl +/−/0, Ctrl+scroll), default zoom, full screen (F11), pages' own full-screen video etc. |
| Privacy | Private windows (Ctrl+Shift+N — nothing written to disk), clear cookies & site data |
| Session | Reopens your tabs at start-up, reopen closed tab (Ctrl+Shift+T), crashed-tab recovery |
| Find in page | Ctrl+F: highlights every match, "3 of 12", Enter / Shift+Enter, match case |
| Printing | Ctrl+P: captures the whole page into A4 pages, pick a printer and copies, or save as PDF |
| Customization | Theme, accent colour, interface size, density, corner rounding, tabs on top or down the side, which toolbar buttons show, start page title / background (gradient, colour or your own image) / clock / tiles, custom search engine, what new tabs open |
| Theme | System / light / dark; optionally ask websites for their dark style too |

### Efficiency

- Restored tabs load only when you select them, so start-up loads just one page. Run with `FERRUM_TIMING=1` to see start-up timings.
- Draws only when something changes, and sleeps until the next scheduled repaint — no busy redraw loop; background tabs are hidden so Servo throttles them.
- Profile data is batched and written atomically a couple of seconds after changes, never on the UI's critical path.
- Downloads stream straight to disk on their own threads.
- History and bookmark lists only lay out the rows on screen.

## System requirements

| | Minimum | Comfortable |
|---|---|---|
| CPU | 64-bit dual-core | 4+ cores (Servo lays pages out in parallel) |
| RAM | 4 GB — about 300 MB for one simple tab | 8 GB |
| GPU | OpenGL 3.1 (Intel HD 4000 or newer) | any GPU from the last ~10 years |
| OS | 64-bit Windows 10+, macOS 11+, or Linux | |

Light browsing works on old hardware; heavy web apps and video sites need more. Building from source needs far more than running it — on a weak machine, download a prebuilt release instead.

## Download

Every push to `main` publishes a **Nightly** pre-release with builds for macOS, Windows and Linux on the [Releases](../../releases) page. All releases are pre-releases for now — expect bugs.

## Build

You need about 8 GB of RAM and 20 GB of free disk.

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

```sh
cargo run --release                    # start page
cargo run --release -- servo.org       # open a URL
cargo run --release -- --private       # private window
```

### Build scripts (from a Mac)

```sh
scripts/build.sh macos     # universal Ferrum.app + .dmg (Apple Silicon + Intel)
scripts/build.sh linux     # x86_64 + arm64 .tar.gz, built in Docker
scripts/build.sh windows   # x86_64 .zip, cross-compiled with cargo-xwin
scripts/build.sh all
```

Results land in `dist/`.

| Script | You need |
|---|---|
| macOS | `xcode-select --install`, `brew install llvm cmake pkg-config` |
| Linux | Docker Desktop, OrbStack or Colima (nothing else on the Mac) |
| Windows | `brew install llvm cmake ninja nasm`, `cargo install cargo-xwin` |

Notes: on Apple Silicon the Linux arm64 build is native; x86_64 runs under emulation and is much slower (`ARCHS=arm64 scripts/build-linux.sh` to skip it). Cross-compiling Windows is the most experimental route — if it fails, the GitHub Actions workflow builds Windows natively. Set `SIGN_IDENTITY` to sign the Mac app with your Developer ID (otherwise it's ad-hoc signed).

### Building natively

**macOS:** `xcode-select --install`, `brew install llvm cmake pkg-config`, then `cargo build --release`.

**Windows:** Visual Studio 2022 Build Tools ("Desktop development with C++"), then `winget install LLVM.LLVM Kitware.CMake NASM.NASM Python.Python.3.12` and `cargo build --release`.

**Debian/Ubuntu:**
```sh
sudo apt install clang llvm libclang-dev cmake python3 pkg-config \
  libfontconfig-dev libfreetype-dev libharfbuzz-dev libx11-dev libxcb1-dev \
  libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libwayland-dev libegl-dev libgl-dev libgles-dev libssl-dev libdbus-1-dev libudev-dev
cargo build --release
```

## Keyboard shortcuts

On macOS use ⌘ where it says Ctrl.

| Keys | Action |
|---|---|
| Ctrl+T / Ctrl+W | New tab / close tab |
| Ctrl+Shift+T | Reopen closed tab |
| Ctrl+Shift+N | New private window |
| Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+1…9 | Switch tabs |
| Ctrl+L, F6 | Address bar |
| Ctrl+R, F5 | Reload |
| Alt+← / Alt+→ | Back / forward |
| Ctrl+F, Ctrl+G / F3 | Find in page, next match (Shift for previous) |
| Ctrl+P | Print |
| Ctrl+D | Bookmark page |
| Ctrl+Shift+B | Toggle bookmarks bar |
| Ctrl+Shift+O / Ctrl+H / Ctrl+J | Bookmarks / history / downloads panel |
| Ctrl+, | Settings |
| Ctrl +, Ctrl −, Ctrl 0 | Zoom in / out / reset |
| F11 | Full screen |
| Esc | Close menu or popup, leave page full screen |

## Where data is kept

Settings, bookmarks, history and the last session are small JSON files; cookies and site storage are kept by Servo alongside them.

| OS | Folder |
|---|---|
| macOS | `~/Library/Application Support/Ferrum` |
| Windows | `%APPDATA%\Ferrum` |
| Linux | `~/.local/share/Ferrum` |

## Project layout

```
src/main.rs       start-up, command line, event loop
src/browser.rs    window, tabs, Servo callbacks, input, shortcuts
src/ui.rs         the interface: tabs, toolbar, menus, panels, page dialogs
src/profile.rs    settings, bookmarks, history, session (JSON)
src/downloads.rs  download manager
src/find.rs       find in page
src/print.rs      page capture, PDF writer, printers
src/location.rs   address-bar text → URL or search
src/keys.rs       keyboard events → W3C key events for Servo
src/icons.rs      vector toolbar icons
scripts/          build scripts for macOS, Linux and Windows
```

## Limitations

Servo is still maturing, so some heavy sites (big web apps, some video players) won't render perfectly, and a few CSS features are missing. Not built yet: extensions and sync. Find in page and printing are implemented by Ferrum itself, because Servo has no API for them yet: find runs a small highlighting script in the page, and printing captures the page as images (so printed text isn't selectable, and pages longer than 40 screens are cut off). JavaScript runs on SpiderMonkey (C++), the engine Servo uses; everything else in Ferrum's own code is Rust.

## License

MIT — see [LICENSE](LICENSE). Servo and its dependencies keep their own licenses (mostly MPL-2.0, MIT and Apache-2.0).
