# Profile and data

`profile.rs` owns everything Ferrum remembers between runs.

## Location

`Profile::default_dir()` = `dirs::data_dir()/Ferrum`:

| OS | Folder |
|---|---|
| macOS | `~/Library/Application Support/Ferrum` |
| Windows | `%APPDATA%\Ferrum` |
| Linux | `$XDG_DATA_HOME/Ferrum` (usually `~/.local/share/Ferrum`) |

```
Ferrum/
├── settings.json
├── bookmarks.json
├── history.json
├── session.json
└── engine/          ← Servo's own data: cookies, localStorage, caches
```

The `engine` folder is passed to Servo as `Opts::config_dir`, which makes cookies and site storage persistent.

## Private windows

`ferrum --private` (or the menu / Ctrl+Shift+N, which launches a new process with that flag) loads `Profile::load(None)`:

- `dir` is `None`, so `flush`, `save_session` and `record_visit` do nothing,
- Servo gets `Opts::temporary_storage = true`, so cookies and site data live in memory only.

## Formats

All files are pretty-printed JSON. Unknown or missing fields fall back to defaults (`#[serde(default)]`), so older and newer versions can read each other's files.

**settings.json** (abridged)
```json
{
  "search_engine": "DuckDuckGo",
  "custom_search_url": "",
  "home_page": "",
  "new_tab_page": "StartPage",
  "restore_session": true,
  "theme": "System",
  "dark_websites": false,
  "accent": [222, 98, 52],
  "ui_scale": 1.0,
  "compact": false,
  "roundness": 6,
  "tab_layout": "Top",
  "show_bookmarks_bar": true,
  "start_title": "Ferrum",
  "start_background": "Plain",
  "start_image": "",
  "start_show_clock": false
}
```

Look-related fields (theme, accent, size, density, rounding) are hashed by `style_key` in ui.rs; egui's style is rebuilt only when that hash changes. The start-page image is decoded once per path (and scaled down to at most 2560×1600) and kept as a texture.

**bookmarks.json** — ordered list (the order is the bookmarks bar order):
```json
[{ "title": "Servo", "url": "https://servo.org/" }]
```

**history.json** — one entry per URL, oldest first, capped at 5,000 (`MAX_HISTORY`):
```json
[{ "url": "https://servo.org/", "title": "Servo", "visits": 3, "last_visit": 1762780000 }]
```

**session.json** — open tabs when Ferrum last closed:
```json
{ "tabs": ["https://servo.org/"], "active": 0 }
```

## Saving

Changes call `mark_dirty()`. `flush(false)` — called from every `pump` — writes only once changes are at least 2 seconds old (`SAVE_DELAY`), so a burst of changes becomes one write. `flush(true)` writes immediately; it runs on exit.

Each file is written to `*.json.tmp` and then renamed over the original, so a crash or power cut can't leave a half-written file.

## History

- `record_visit` runs when a page finishes loading (`LoadStatus::Complete`). Only `http`/`https` pages are recorded. A revisit moves the entry to the end and bumps `visits`.
- `update_title` fixes up the title when it arrives after the load completed.

## Suggestions

`Profile::suggestions(input, limit)` powers the address bar:

1. bookmarks whose URL (without scheme/`www.`) starts with, or whose URL/title contains, the input,
2. then history entries matching the same way, ranked by a simple "frecency" score:

```
score = visits / (1 + age_in_days / 7)
```

so often-visited and recently-visited pages rank first.

## Search engines

`SearchEngine` lists DuckDuckGo (default), Google, Bing, Brave Search, Startpage, Ecosia and Custom (a URL template with `%s`, from `custom_search_url`). `Settings::search_url(query)` builds the URL; `location::input_to_url` uses it when the address-bar text isn't a URL.
