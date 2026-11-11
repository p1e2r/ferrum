<img src="assets/icon-256.png" width="96" align="right" alt="Ferrum icon">

# Ferrum

A fast, light desktop web browser with a native Rust interface.

- **Engine:** the system's own web engine, through [wry](https://github.com/tauri-apps/wry): **WebKit** on macOS (as in Safari), **WebView2** on Windows (Chromium, as in Edge), **WebKitGTK** on Linux. Sites like GitHub, YouTube and Google work as they do in those browsers, and the engine gets security updates with the OS.
- **Interface:** tabs, toolbar, address bar, menus, side panels and popups are all drawn natively in Rust with [egui](https://github.com/emilk/egui). The app itself contains no HTML, CSS or JavaScript. Popups that float over the page (menu, suggestions, site info, notifications) are small native windows of their own.
- **Light:** a ~15 MB app that builds in minutes; runs well on old machines.
- **Platforms:** macOS, Windows, Linux.
- **License:** MIT.

## Features

| | |
|---|---|
| Browsing | Tabs (favicons, middle-click close, right-click: reload, duplicate, close others / to the right), back/forward, reload, home, address bar that searches or navigates; automatic retry and HTTPS→HTTP fallback when a site won't connect |
| Web compatibility | Everything the system engine supports: video, WebGL, modern JavaScript and CSS |
| Address bar | Suggestions from bookmarks and history (↑/↓ to pick), selects all on click, choice of 6 search engines |
| Bookmarks | Star button / Ctrl+D, bookmarks bar, bookmarks panel with reordering, start-page tiles |
| History | Searchable history panel (fast even with thousands of entries), frequently-visited tiles, clear history |
| Downloads | File links download automatically, "Save link/image as…", progress, cancel, open, show in folder; uses the site's cookies |
| Page dialogs, forms, right-click | The engine's native ones (alerts, sign-in, dropdowns, colour and file pickers, context menu) |
| View | Zoom per tab (Ctrl +/−/0, Ctrl+scroll), default zoom, full screen (F11), pages' own full-screen video etc. |
| Privacy | Private windows (Ctrl+Shift+N — nothing written to disk), clear cookies & site data |
| Session | Reopens your tabs at start-up, reopen closed tab (Ctrl+Shift+T), crashed-tab recovery |
| Find in page | Ctrl+F: highlights every match, "3 of 12", Enter / Shift+Enter, match case |
| Printing | Ctrl+P: the system print dialog (printers, save as PDF) |
| Settings page | `ferrum://settings` (Ctrl+, or ☰ → Settings) in its own tab: sections down the side, search, and every option below |
| Themes | 10 complete colour themes (Ferrum light/dark, Midnight, Graphite, Forest, Arctic, Plum, Paper, Sky, Rose, High contrast) plus any accent colour |
| Customization | Interface size, density, corner rounding, tabs on top or down the side, which toolbar buttons show, start page title / background (gradient, colour or your own image) / clock / tiles, custom search engine, what new tabs open, download folder |
| Privacy & web content | HTTPS-only mode, notification permission, custom user agent |
| Theme | System / light / dark |
| Ad & tracker blocking | Built in and on by default, inside the engine: content-blocker rules on macOS and Linux (like Safari's), a request filter on Windows, plus a page script that catches what's added later. Count on the padlock, switch it off per site. Peter Lowe's list (~3,500 servers), refreshed weekly |
| Site information | Padlock in the address bar: secure / not secure, trackers blocked, the site's remembered permissions, clear this site's cookies and data |
| Permissions | "Remember" in permission prompts; remembered answers are listed in settings and can be reset |
| Reader mode | F9 or the reader button: the article without menus, ads and sidebars, drawn natively; text size, width, and Auto/Light/Sepia/Dark |
| Tabs | Pin tabs (icon-only, kept by "close others", restored), drag to reorder, playing-audio indicator and mute, sleeping tabs (unloaded after 30 min unused, reload when selected) |
| Developer tools | F12: the engine's own inspector, console and network tools; Ctrl+U page source with highlighting |
| Notifications | Web notifications on every engine: Ferrum asks Allow/Block per site, shows a card in the window, and uses the system notification centre when Ferrum is in the background |
| Bookmarks import/export | HTML bookmark files from/to Firefox, Chrome, Edge, Safari |
| Updates | Checks GitHub once a day for a newer Ferrum (can be turned off) |
| Pinch zoom, swipe back | Trackpad gestures, handled by the engine |
| Blocked search engines | In Indonesia (where DuckDuckGo is blocked) new installs search with Google; if a network replaces a search with a block page, Ferrum offers to repeat it with another engine |
| Title bar | Like Chrome: tabs sit in the title bar with Ferrum's own minimise / maximise / close buttons (close turns red on hover), drag the empty strip to move the window, double-click to maximise, resize from the window edges. Round-hover ✕ on tabs. On macOS the traffic lights stay over a transparent title bar. Can be switched back to the system title bar in settings |

### Efficiency

- Restored tabs load only when you select them, so start-up loads just one page. Run with `FERRUM_TIMING=1` to see start-up timings.
- Draws only when something changes, and sleeps until the next scheduled repaint — no busy redraw loop; background tabs are hidden so the engine throttles them.
- Profile data is batched and written atomically a couple of seconds after changes, never on the UI's critical path.
- Downloads stream straight to disk on their own threads.
- Blocking ads and trackers means less to download, parse and run on most sites.
- Tabs you haven't used for a while are put to sleep, freeing their memory.
- History and bookmark lists only lay out the rows on screen.

## System requirements

| | Minimum | Comfortable |
|---|---|---|
| CPU | 64-bit dual-core | 4 cores |
| RAM | 2 GB — the interface itself uses about 40 MB; each tab what the system engine needs | 4 GB+ |
| GPU | OpenGL 2.1 / GLES 2 (almost anything) | any |
| OS | 64-bit Windows 10+, macOS 11+, or Linux | |

Light browsing works on old hardware; heavy web apps and video sites need more. Building from source needs far more than running it — on a weak machine, download a prebuilt release instead.

## Download

Every push to `main` publishes a **Nightly** pre-release with builds for macOS, Windows and Linux on the [Releases](../../releases) page. All releases are pre-releases for now — expect bugs.

## Build

A first build takes a few minutes (about 2–5 on a normal machine), rebuilds about 30 seconds.

```sh
cargo run --release                    # start page
cargo run --release -- github.com      # open a URL
cargo run --release -- --private       # private window
```

| OS | You need |
|---|---|
| macOS | Xcode command line tools (`xcode-select --install`) |
| Windows | Visual Studio 2022 Build Tools ("Desktop development with C++"). WebView2 comes with Windows 10 and 11 |
| Debian/Ubuntu | `sudo apt install pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libx11-dev libxkbcommon-x11-dev libegl-dev libgl-dev` |

### Build scripts (from a Mac)

```sh
scripts/build.sh macos     # Ferrum.app as two .dmg files: Apple Silicon and Intel
scripts/build.sh linux     # x86_64 + arm64 .tar.gz, built in Docker
scripts/build.sh windows   # x86_64 .zip, cross-compiled with cargo-xwin
scripts/build.sh all
```

Results land in `dist/`. GitHub Actions builds all platforms natively on every push and publishes a nightly pre-release.

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
| Ctrl+, | Settings page |
| F9, Ctrl+Alt+R | Reader mode |
| F12, Ctrl+Shift+I / J | Developer tools |
| Ctrl+U | View page source |
| Ctrl+M | Mute tab (Windows/Linux) |
| Ctrl+/ | All keyboard shortcuts (`ferrum://shortcuts`) |
| Ctrl +, Ctrl −, Ctrl 0 | Zoom in / out / reset |
| F11 | Full screen |
| Esc | Close menu or popup, leave page full screen |

## Where data is kept

Settings, bookmarks, history and the last session are small JSON files; cookies and site storage are kept by the system engine (in the profile folder on Windows and Linux).

| OS | Folder |
|---|---|
| macOS | `~/Library/Application Support/Ferrum` |
| Windows | `%APPDATA%\Ferrum` |
| Linux | `~/.local/share/Ferrum` |

## Project layout

```
src/main.rs       start-up, command line, event loop
src/browser.rs    window, tabs, web views (wry), engine messages, shortcuts
src/gl.rs         the window's OpenGL surface (glutin)
src/overlay.rs    popup windows that float over the page
src/overlay_ui.rs what the popups show (menu, suggestions, site info…)
src/pagescript.rs script added to every page (shortcuts, notifications, blocking)
src/adblock.rs    engine-level ad blocking per platform
src/notify.rs     system notifications
src/ui.rs         the interface: tabs, toolbar, menus, panels, page dialogs
src/settings_page.rs  the settings page (ferrum://settings)
src/features_ui.rs    console panel, site info popup, notifications, reader, shortcuts page
src/blocker.rs    ad and tracker blocking
src/reader.rs     reader mode (article extraction + native layout)
src/devtools.rs   page-source viewer
src/bookmarks_io.rs  bookmark import/export (Netscape HTML)
src/updates.rs    update check against GitHub releases
src/theme.rs      colour themes
src/profile.rs    settings, bookmarks, history, session (JSON)
src/downloads.rs  download manager
src/find.rs       find in page
src/location.rs   address-bar text → URL or search
src/icons.rs      vector toolbar icons
scripts/          build scripts for macOS, Linux and Windows
```

## Limitations

Ferrum shows pages with the engine your OS provides, so a page looks the same as in Safari (macOS), Edge (Windows) or GNOME Web (Linux). Not built yet: extensions and sync. Linux runs under X11 (XWayland on Wayland desktops). Opening a page in a popup window (`window.open`) opens it in a new tab, so a few sign-in popups that talk back to their opener may not complete.

## License

MIT — see [LICENSE](LICENSE). Dependencies keep their own licenses (mostly MIT and Apache-2.0).
