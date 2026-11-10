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

**settings.json**
```json
{
  "search_engine": "DuckDuckGo",
  "theme": "System",
  "home_page": "",
  "restore_session": true,
  "show_bookmarks_bar": true,
  "default_zoom": 1.0,
  "ask_where_to_save": false
}
```

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

`SearchEngine` lists DuckDuckGo (default), Google, Bing, Brave Search, Startpage and Ecosia. `search_url(query)` builds the URL; `location::input_to_url` uses it when the address-bar text isn't a URL.
