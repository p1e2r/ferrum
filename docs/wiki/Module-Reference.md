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
| `draw_page_controls` | The active tab's dialog/popup and its outcome |
| `draw_menu` | Main menu |
| `draw_toast` | Short fading notices ("Bookmarked", "Zoom 110%") |

Helpers: `style` / `set_theme` (egui look), `letter_icon` (coloured letter tile when there's no favicon), `link_row` (row with hover ✕ used by history and bookmarks), `truncate`, `host_of`.

## `profile.rs` — settings and data

`ThemeChoice`, `SearchEngine` (with `name`, `search_url`), `Settings`, `Bookmark`, `HistoryEntry`, and `Profile` with `load`, `flush`, `mark_dirty`, `load_session`/`save_session`, bookmark and history helpers, and `suggestions`. See [Profile and data](Profile-and-Data).

## `downloads.rs` — downloads

`Status`, `Progress`, `Download` (`snapshot`, `cancel`), `Downloads` (`start`, `active_count`, `clear_finished`), and helpers `looks_like_download`, `human_size`, `open_path`. See [Downloads](Downloads).

## `location.rs` — address bar input

`input_to_url(text, engine)` decides, in order: full URL (`https://…`, `about:`, `data:`, `file:`) → local file path → host name (`example.com`, `localhost:3000`; local addresses get `http`) → search.

## `keys.rs` — keyboard conversion

`keyboard_event(&KeyEvent, ModifiersState)` and `modifiers(ModifiersState)`. See [Input handling](Input-Handling#keyboard-conversion-keysrs).

## `icons.rs` — icons

`Icon` (Back, Forward, Reload, Stop, Home, Plus, Close, Menu, Star, StarFilled, Download) and `icon_button(ui, icon, size, enabled, tooltip)`. Icons are drawn from line segments and polygons, so they're crisp at any scale and need no image or icon-font files.
