# Ferrum

A fast, native desktop web browser written in Rust.

- **Engine:** [Servo](https://servo.org), the Rust browser engine that Firefox's Stylo (CSS) and WebRender (GPU painting) came from.
- **Interface:** tabs, toolbar, address bar, menus, side panels and page dialogs are all drawn natively in Rust with [egui](https://github.com/emilk/egui), on the same OpenGL surface as the page. The app itself contains no HTML, CSS or JavaScript.
- **Platforms:** macOS, Windows, Linux.
- **License:** MIT.

## Features

| | |
|---|---|
| Browsing | Tabs (with favicons, middle-click close), back/forward, reload, home, address bar that searches or navigates |
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
| Theme | System / light / dark — also passed to sites (`prefers-color-scheme`) |

### Efficiency

- Draws only when something changes (no constant redraw loop); background tabs are hidden so Servo throttles them.
- Profile data is batched and written atomically a couple of seconds after changes, never on the UI's critical path.
- Downloads stream straight to disk on their own threads.
- History and bookmark lists only lay out the rows on screen.

## Download

Pre-built binaries for every platform are produced by [GitHub Actions](../../actions) on each push, and attached to [Releases](../../releases) for tagged versions.

## Build

Servo is a large engine: the first build takes 20–60 minutes and needs ~8 GB RAM and ~20 GB disk. Later builds are quick.

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
src/location.rs   address-bar text → URL or search
src/keys.rs       keyboard events → W3C key events for Servo
src/icons.rs      vector toolbar icons
scripts/          build scripts for macOS, Linux and Windows
```

## Limitations

Servo is still maturing, so some heavy sites (big web apps, some video players) won't render perfectly, and a few CSS features are missing. Not built yet: find in page (Servo has no API for it yet), extensions, sync, and printing. JavaScript runs on SpiderMonkey (C++), the engine Servo uses; everything else in Ferrum's own code is Rust.

## License

MIT — see [LICENSE](LICENSE). Servo and its dependencies keep their own licenses (mostly MPL-2.0, MIT and Apache-2.0).
