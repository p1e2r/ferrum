# Contributing

## Ground rules

- **Rust only** in Ferrum's own code. No HTML, CSS or JavaScript in the interface.
- **Stay fast.** Don't add per-frame work that runs when nothing changed. If something animates, request repaints only while it is visible (see the progress bar and toast for the pattern).
- **Never block the main thread on the network or disk.** Use a thread plus a `WakeUp` (see downloads), or batch writes (see `Profile::flush`).
- **Engine callbacks only queue.** Add an `Event` variant (or a page-script message) and handle it in `pump` / `handle_page_message`, instead of doing work inside a wry handler.
- Keep files formatted with `cargo fmt` and free of `cargo clippy` warnings.

## Adding a keyboard shortcut

Add an arm to `Browser::handle_shortcut` and a row to the README's shortcut table:

```rust
(_, "p") if cmd => self.print_page(),
```

## Adding a setting

1. Add the field to `Settings` in `profile.rs` with a default in `impl Default` (old settings files keep working thanks to `#[serde(default)]`).
2. Add a control in `Browser::draw_settings` (ui.rs). The panel compares settings before and after drawing and calls `mark_dirty()` itself.
3. Use it wherever it applies.

## Handling something new from the engine

1. For wry's own callbacks (title, page load, new window, download), add the handler in `Browser::create_webview`; it should only push an `Event`.
2. For anything a page does, report it from the page script (`pagescript.rs`) with `post({ t: "…" })` and handle it in `Browser::handle_page_message`.
3. Anything shown over the page goes in a popup: add an `OverlayKind`, request it in `request_popups` and draw it in `draw_overlay` (overlay_ui.rs).

## Adding a side panel

Add a `SidePanel` variant, a title in `draw_side_panel`, a `draw_*` function, a menu row in `draw_menu`, and a shortcut.

## Testing

- `cargo test --bins` for unit tests.
- For interface changes, run `cargo run` and try it; the start page, file links (downloads), `alert()`/`prompt()`, `<select>` and right-click are quick to check with any local HTML page:

```sh
python3 -m http.server 8000   # in a folder with a test page
cargo run -- localhost:8000
```

## Licence

Ferrum is MIT-licensed. By contributing you agree your contribution is released under the same licence. Start new source files with:

```rust
// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
```
