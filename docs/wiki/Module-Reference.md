# Module reference

## `main.rs` — start-up and event loop

| Item | Purpose |
|---|---|
| `WakeUp` | User event sent from other threads to wake the main thread |
| `Waker` | Implements Servo's `EventLoopWaker` by sending `WakeUp` |
| `parse_args()` | `ferrum [--private] [URL]`, plus `--help`, `--version` |
| `App` | winit `ApplicationHandler`: `Starting` → `Running(Browser)` → `Done` |
| `main()` | installs the rustls crypto provider, builds the event loop, runs `App` |

On Windows release builds `windows_subsystem = "windows"` hides the console window.

## `browser.rs` — the browser

| Item | Purpose |
|---|---|
| `PageControl`, `Pending` | A page request awaiting the user (see [Servo callbacks](Servo-Callbacks)) |
| `Event` (private) | Work queued by the delegate for `pump` |
| `Delegate` | Servo's `WebViewDelegate`; queues events, requests redraws |
| `set_cursor()` | Maps Servo cursors to winit cursors |
| `Tab` | One tab: optional webview, crash state, zoom, favicon. `title()`, `url()`, `loading()` |
| `SidePanel` | Which side panel is open |
| `Browser` | Owns all state (below) |
| `run_file_picker()` | Answers `<input type=file>` with the system dialog |

### `Browser` methods

| Group | Methods |
|---|---|
| Set-up | `new(event_loop, proxy, waker, url, private)` — window, GL contexts, Servo (with profile `Opts`), egui, profile, downloads; opens the URL, the restored session, or the home page |
| Tabs | `create_webview`, `open_tab`, `open_home_tab`, `open_background_tab`, `activate`, `close_tab`, `reopen_closed_tab`, `active_tab`, `active_webview`, `tab_index` |
| Navigation | `navigate_input`, `navigate`, `go_home`, `go_back`, `go_forward`, `reload` (also recovers crashed tabs), `sync_location` |
| View | `set_zoom`, `zoom_step` (steps 30%–300%), `toggle_window_fullscreen`, `apply_theme` (UI + `prefers-color-scheme`) |
| Features | `toggle_bookmark`, `start_download`, `clear_site_data`, `open_private_window`, `toast` |
| Event loop | `pump`, `handle_window_event`, `handle_shortcut`, `forward_to_page`, `redraw` |
| Popups | `has_popup`, `dismiss_popups`, `toggle_panel` |
| Helpers | `refresh_favicon`, `in_content`, `page_point`, `mouse_physical`, `egui_ctx` |

`Drop for Browser` saves the session, flushes the profile, declines pending page requests and tears down egui's GL resources.

## `ui.rs` — the interface

All functions are `impl Browser` methods called from `draw`, in this order:

| Function | Draws |
|---|---|
| `draw` | Entry point: theme, favicons, then everything below |
| `draw_tab_strip` | Tabs (favicon/spinner/crash icon, title, close), private badge, new-tab button |
| `draw_toolbar` | Back, forward, reload, home, address bar, zoom chip, bookmark star, downloads (with badge), menu; the loading bar |
| `draw_suggestions` | Address-bar suggestion popup |
| `draw_bookmarks_bar` | Bookmarks bar |
| `draw_side_panel` | Frame for `draw_history`, `draw_bookmarks`, `draw_downloads`, `draw_settings` |
| `draw_content` | The page (resize, paint, blit), the start page, or the crash panel; the link status bubble |
| `draw_start_page` | Logo, search box, bookmark tiles, frequently-visited tiles |
| `draw_find_bar` | Find bar (query, n of m, up/down, match case) |
| `draw_print_dialog` | Print progress, printer and copies, Save as PDF / Open PDF |
| `draw_vertical_tabs` | Tab list down the side (when tabs are set to "Down the side") |
| `draw_page_controls` | The active tab's dialog/popup and its outcome |
| `draw_menu` | Main menu |
| `draw_toast` | Short fading notices ("Bookmarked", "Zoom 110%") |

Helpers: `style` (applies theme, accent, interface size, density and rounding from `Settings`; only re-run when `style_key` changes) / `set_theme`, `letter_icon` (coloured letter tile when there's no favicon), `link_row` (row with hover ✕ used by history and bookmarks), `truncate`, `host_of`.

## `settings_page.rs` — the settings page

`ferrum://settings` (aliases `about:settings`, `about:preferences`) is an *internal page*: a tab with `internal: Some(InternalPage::Settings)` and no webview, drawn by `Browser::draw_settings_page`. `Browser::open_settings` switches to it or opens it.

Every setting is one entry in `ITEMS`: its `Section`, a `Setting` id, label, description, search keywords, and whether its control sits beside the text or under it (`wide`). The page shows the selected section, or, while the search box has text, every item whose label, description, keywords or section name contains all the search words. `Browser::setting_control` draws each control (a `match` on `Setting`); `visible` hides settings that depend on another (e.g. the custom search address only for the Custom engine). Helpers: `toggle` (animated switch), `segmented`, `card`, `row`.

After drawing, if any setting changed the profile is marked for saving and `Browser::apply_engine_settings` sends changed engine preferences to Servo.

**Adding a setting:** add the field to `Settings`, a `Setting` variant, an `ITEMS` entry, and an arm in `setting_control`.

## `theme.rs` — colour themes

`ThemePreset` lists the themes. `Ferrum` is the standard egui look and follows the light/dark setting; every other preset has a `Palette` (dark or light, panel, surface, text, suggested accent). `apply` paints an egui `Visuals` from a palette: panels and dialogs use `panel`, the toolbar, text fields and cards use `surface`, and widget fills are blends of panel and text (`mix`). `ui::style` resets both egui styles, applies the preset's palette to the matching one, and switches egui to that mode; `style_key` makes this run only when a look-related setting changes.

## Engine settings

`browser::engine_prefs` maps Ferrum settings to Servo preferences: HTTPS-only (`network_enforce_tls_enabled`), notifications (`dom_notification_enabled`), location (`dom_geolocation_enabled`), font size (`fonts_default_size`) and user agent (`user_agent`; empty = Servo's default, remembered at start-up). They're set in `Preferences` at start-up and sent with `Servo::set_preference` when changed, taking effect for pages loaded afterwards.

## `profile.rs` — settings and data

`ThemeChoice`, `SearchEngine` (with `name`, `search_url`), `Settings`, `Bookmark`, `HistoryEntry`, and `Profile` with `load`, `flush`, `mark_dirty`, `load_session`/`save_session`, bookmark and history helpers, and `suggestions`. See [Profile and data](Profile-and-Data).

## `downloads.rs` — downloads

`Status`, `Progress`, `Download` (`snapshot`, `cancel`), `Downloads` (`start`, `active_count`, `clear_finished`), and helpers `looks_like_download`, `human_size`, `open_path`. See [Downloads](Downloads).

## `find.rs` — find in page

`FindState` (`open`, `query`, `match_case`, `result`) with `search_if_changed`, `step`, `close`, `reset`. Servo has no find API, so a script (`SCRIPT`) runs in the page: it walks text nodes with a `TreeWalker`, wraps matches in `<mark data-ferrum-find>`, colours the current one orange, scrolls it into view and returns `[current, total]`. Closing the bar runs it with an empty query, which unwraps all marks. `reset` is called on navigation and tab switches.

## `print.rs` — printing

`PrintJob` is a small state machine advanced from `Browser::pump`: **Measuring** (ask the page for its height) → **Capturing** (scroll one screen, `take_screenshot`, repeat, up to 40 screens) → **Ready** (scroll restored, PDF built, printers listed) or **Failed**. `stitch` joins the screenshots and drops the overlap of the last screen; `write_pdf` cuts the result into A4 pages and writes a PDF by hand (one JPEG image per page). `list_printers` uses `lpstat` (macOS/Linux) or `Get-Printer` (Windows); `send_to_printer` uses `lp` or PowerShell's `PrintTo`.

## `location.rs` — address bar input

`input_to_url(text, engine)` decides, in order: full URL (`https://…`, `about:`, `data:`, `file:`) → local file path → host name (`example.com`, `localhost:3000`; local addresses get `http`) → search.

## `keys.rs` — keyboard conversion

`keyboard_event(&KeyEvent, ModifiersState)` and `modifiers(ModifiersState)`. See [Input handling](Input-Handling#keyboard-conversion-keysrs).

## `icons.rs` — icons

`Icon` (Back, Forward, Reload, Stop, Home, Plus, Close, Menu, Star, StarFilled, Download, Up, Down) and `icon_button(ui, icon, size, enabled, tooltip)`. Icons are drawn from line segments and polygons, so they're crisp at any scale and need no image or icon-font files.
