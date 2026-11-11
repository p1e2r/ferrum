# Module reference

## `main.rs` — start-up and event loop

| Item | Purpose |
|---|---|
| `WakeUp` | User event sent from other threads (downloads, favicons, engine callbacks) to wake the main thread |
| `parse_args()` | `ferrum [--private] [URL]`, plus `--help`, `--version` |
| `App` | winit `ApplicationHandler`: `Starting` → `Running(Browser)` → `Done`. On Linux, `about_to_wait` also runs GTK's main loop, which WebKitGTK needs |
| `install_crash_reporter()` | Writes a crash log to the profile folder on a panic |
| `main()` | installs the rustls crypto provider; on Linux starts GTK on X11; builds the event loop, runs `App` |

On Windows release builds `windows_subsystem = "windows"` hides the console window.

## `browser.rs` — the browser

| Item | Purpose |
|---|---|
| `Event` (private) | Work queued by wry's handlers for `pump`: `Title`, `Load`, `NewTab`, `Download`, `Ipc` (a page-script message) |
| `Shortcut` | A key plus Ctrl/⌘/Shift/Alt, from Ferrum's window or the page script |
| `Tab` | One tab: optional `wry::WebView`, title, URL, loading, zoom, favicon, pinned / muted / playing, blocked count, reader article, page source, page bounds. `title()`, `url()`, `loading()` |
| `InternalPage` | Pages Ferrum draws itself: `Settings` (`ferrum://settings`), `Shortcuts` (`ferrum://shortcuts`), `Source` (view source) |
| `SidePanel` | Which side panel is open |
| `PermissionAsk` | A site asking to show notifications |
| `Browser` | Owns all state (below) |

### `Browser` methods

| Group | Methods |
|---|---|
| Set-up | `new` — window and GL surface (`gl.rs`), the engine's `WebContext` in the profile, egui, profile, downloads, ad-block rules; opens the URL, the restored session, or the home page |
| Tabs | `create_webview` (wry builder with all handlers), `new_tab`, `open_tab`, `open_home_tab`, `open_background_tab`, `activate`, `close_tab`, `duplicate_tab`, `active_tab`, `active_webview` |
| Navigation | `navigate_input`, `navigate`, `go_home`, `go_back`, `go_forward`, `can_go_back`, `reload`, `stop`, `sync_location` |
| View | `set_zoom`, `zoom_step` (30%–300%), `toggle_window_fullscreen`, `apply_page_theme`, `layout_pages` (places the active page view) |
| Features | `toggle_bookmark`, `start_download`, `clear_site_data`, `open_private_window`, `toast`, `print`, find (`open_find`, `find_step`, `close_find`) |
| Modern features | `toggle_reader`, `view_source`, `toggle_devtools` (the engine's own inspector), `toggle_mute`, `toggle_pin`, `move_tab`, `check_for_updates`, `import_bookmarks` / `export_bookmarks`, `toggle_site_blocking`, `clear_this_site`, `open_internal`, `open_settings` |
| Pages | `handle_page_message` (page-script messages), `answer_notifications`, `answer_permission_ask` |
| Event loop | `pump`, `handle_window_event`, `handle_shortcut`, `next_wake`, `save_if_due`, `request_quit` |
| Popups | `request_popups`, `draw_overlay` (overlay_ui.rs), `close_popups`, `toggle_panel` |

`Drop for Browser` saves the session, flushes the profile, closes the popups and tears down egui's GL resources.

## `gl.rs` — the OpenGL surface

`GlWindow` creates the main window with an OpenGL context (glutin), and `create_popup` gives popup windows their own surface on the same context. `make_current(_on)`, `swap`, `resize(_surface)`.

## `overlay.rs`, `overlay_ui.rs` — popups

`OverlayKind` (Menu, Suggestions, SiteInfo, TabMenu, Toast, Notification, SearchBlocked, Permission) and `Overlays` (`sync`, `draw`, `handle_event`, `destroy`). `overlay_ui.rs` has `Browser::request_popups` and `draw_overlay` plus one `…_contents` function per popup. See [Pages and popups](Pages-and-Popups).

## `pagescript.rs`, `adblock.rs`, `notify.rs`

`pagescript::script` builds the script added to every page; `mute` returns the script that mutes or unmutes a page's media. `adblock::rules_json` builds the rule list; `prepare` / `attach` install it in each platform's engine. `notify::system_notification` shows a desktop notification. See [Page script](Page-Script).

## `ui.rs` — the interface

All functions are `impl Browser` methods called from `draw`, in this order:

| Function | Draws |
|---|---|
| `draw` | Entry point: theme, favicons, then everything below |
| `draw_tab_strip` | Tabs (favicon/spinner/crash icon, title, close), private badge, new-tab button |
| `draw_toolbar` | Back, forward, reload, home, address bar, zoom chip, bookmark star, downloads (with badge), menu; the loading bar |
| `draw_bookmarks_bar` | Bookmarks bar |
| `draw_side_panel` | Frame for `draw_history`, `draw_bookmarks`, `draw_downloads`, `draw_settings` |
| `draw_content` | Reserves the page area (the engine's view is placed there), or draws the start page, reader mode or an internal page |
| `draw_start_page` | Logo, search box, bookmark tiles, frequently-visited tiles |
| `draw_find_bar` | Find bar (query, n of m, up/down, match case) |
| `draw_vertical_tabs` | Tab list down the side (when tabs are set to "Down the side") |

Helpers: `style` (applies theme, accent, interface size, density and rounding from `Settings`; only re-run when `style_key` changes) / `set_theme`, `letter_icon` (coloured letter tile when there's no favicon), `link_row` (row with hover ✕ used by history and bookmarks), `truncate`, `host_of`.

## `settings_page.rs` — the settings page

`ferrum://settings` (aliases `about:settings`, `about:preferences`) is an *internal page*: a tab with `internal: Some(InternalPage::Settings)` and no webview, drawn by `Browser::draw_settings_page`. `Browser::open_settings` switches to it or opens it.

Every setting is one entry in `ITEMS`: its `Section`, a `Setting` id, label, description, search keywords, and whether its control sits beside the text or under it (`wide`). The page shows the selected section, or, while the search box has text, every item whose label, description, keywords or section name contains all the search words. `Browser::setting_control` draws each control (a `match` on `Setting`); `visible` hides settings that depend on another (e.g. the custom search address only for the Custom engine). Helpers: `toggle` (animated switch), `segmented`, `card`, `row`.

After drawing, if any setting changed the profile is marked for saving and `Browser::apply_engine_settings` applies the ones that affect pages (theme, ad blocking).

**Adding a setting:** add the field to `Settings`, a `Setting` variant, an `ITEMS` entry, and an arm in `setting_control`.

## `features_ui.rs` — reader mode and shortcuts

`draw_reader` (reader mode in place of the page) and `draw_shortcuts_page` (`ferrum://shortcuts`, from the `SHORTCUTS` table).

## `blocker.rs` — the block list

`Blocker` (`new`, `load_list`, `should_block`, `domains`, `domain_count`) and `base_domain`. The list is built in plus Peter Lowe's list, cached in the profile and refreshed weekly. See [Page script](Page-Script#ad-and-tracker-blocking).

## `reader.rs` — reader mode

`SCRIPT` runs in the page and returns the article as JSON: it takes `<article>`, `<main>` or the element with the most paragraph text, skips navigation, asides, footers and ad/share/comment blocks, and lists headings, paragraphs, quotes, list items and code blocks. `ReaderDoc::parse` reads it; `draw` lays it out natively in a centred column with its own toolbar (text size, theme, width, reading time).

## `devtools.rs` — page source

`Source` and `draw_source` (line-numbered, colour-highlighted HTML; only the visible lines are laid out). Developer tools are the engine's own inspector (`WebView::open_devtools`).

## `browser_import.rs` — import from other browsers

`find_sources` lists browser profiles on this computer (Chrome, Edge, Brave, Vivaldi, Opera, Chromium, Arc, Firefox, Safari); `import(source, bookmarks, history)` reads them: Chromium's `Bookmarks` JSON and `History` database, Firefox's `places.sqlite`, Safari's `Bookmarks.plist` and `History.db` (macOS asks for Full Disk Access for Safari). Databases are copied before reading, because the other browser may have them locked. Shown in Settings → Advanced → Import from another browser.

## `bookmarks_io.rs`, `updates.rs`

`export` / `import` of Netscape bookmark files (folders are flattened; only http/https/file links are imported). `updates::check` asks the GitHub releases API on a background thread, compares versions numerically (`parse_version`) and picks this computer's package. `updates::install` downloads it and swaps it in for the running copy (macOS: mounts the DMG and replaces `Ferrum.app`; Windows: unpacks the zip and renames the running `ferrum.exe` out of the way; Linux: replaces the binary), then `relaunch_after_exit` starts the new version once Ferrum has quit, reopening the same tabs (`resume_once`). `clean_up_after_update` removes leftovers at start-up.

## `theme.rs` — colour themes

`ThemePreset` lists the themes. `Ferrum` is the standard egui look and follows the light/dark setting; every other preset has a `Palette` (dark or light, panel, surface, text, suggested accent). `apply` paints an egui `Visuals` from a palette: panels and dialogs use `panel`, the toolbar, text fields and cards use `surface`, and widget fills are blends of panel and text (`mix`). `ui::style` resets both egui styles, applies the preset's palette to the matching one, and switches egui to that mode; `style_key` makes this run only when a look-related setting changes.

## `profile.rs` — settings and data

`ThemeChoice`, `SearchEngine` (with `name`, `search_url`), `Settings`, `Bookmark`, `HistoryEntry`, and `Profile` with `load`, `flush`, `mark_dirty`, `load_session`/`save_session`, bookmark and history helpers, and `suggestions`. See [Profile and data](Profile-and-Data).

## `downloads.rs` — downloads

`Status`, `Progress`, `Download` (`snapshot`, `cancel`), `Downloads` (`start`, `active_count`, `clear_finished`), and helpers `looks_like_download`, `human_size`, `open_path`. See [Downloads](Downloads).

## `find.rs` — find in page

`FindState` (`open`, `query`, `match_case`, `result`) with `search_if_changed`, `step`, `close`, `reset`. A script (`SCRIPT`) runs in the page: it walks text nodes with a `TreeWalker`, wraps matches in `<mark data-ferrum-find>`, colours the current one orange, scrolls it into view and returns `[current, total]`. Closing the bar runs it with an empty query, which unwraps all marks. `reset` is called on navigation and tab switches.

## Printing

`Browser::print` opens the engine's own print dialog (`WebView::print`), which also offers Save as PDF.

## `location.rs` — address bar input

`input_to_url(text, engine)` decides, in order: full URL (`https://…`, `about:`, `data:`, `file:`) → local file path → host name (`example.com`, `localhost:3000`; local addresses get `http`) → search.

## `icons.rs` — icons

`Icon` (Back, Forward, Reload, Stop, Home, Plus, Close, Menu, Star, StarFilled, Download, Up, Down, Warning, History, Gear, Lock, Info, Speaker, Muted, Pin, Shield, Reader, Code), `icon_button(ui, icon, size, enabled, tooltip)`, `icon` and `paint_icon`; `tab_close_button` (Chrome-style ✕ with a round hover) and `window_buttons` (minimise / maximise-restore / close, close red on hover).

**Custom title bar.** With `custom_title_bar` on, the window is created without system decorations (on macOS: transparent, full-size title bar, so the traffic lights float over the tab strip). `Browser::title_bar_area` turns the empty strip right of the tabs into a drag handle (`Window::drag_window`, double-click maximises, right-click opens the system window menu) and draws `window_buttons`; with tabs down the side, `draw_title_bar` adds a slim bar for them. `Browser::handle_window_edges` provides resizing from the edges, which undecorated windows otherwise lack. Icons are drawn from line segments and polygons, so they're crisp at any scale and need no image or icon-font files.
