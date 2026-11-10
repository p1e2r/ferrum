# Contributing

## Ground rules

- **Rust only** in Ferrum's own code. No HTML, CSS or JavaScript in the interface.
- **Stay fast.** Don't add per-frame work that runs when nothing changed. If something animates, request repaints only while it is visible (see the progress bar and toast for the pattern).
- **Never block the main thread on the network or disk.** Use a thread plus a `WakeUp` (see downloads), or batch writes (see `Profile::flush`).
- **Servo callbacks only queue.** Add an `Event` variant and handle it in `pump` instead of doing work inside the delegate.
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

## Handling a new Servo callback

1. Override the method in `impl WebViewDelegate for Delegate`.
2. If it needs more than a redraw, add an `Event` variant, push it, and handle it in `Browser::pump`.
3. If the page needs an answer from the user, add a `PageControl` variant, draw it in `draw_page_controls`, and answer it in the outcome `match` at the end of that function. Remember that dropping Servo's request declines it.

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
