# Input handling

> **Note (0.7.0):** Ferrum now uses the system web engine through wry instead of Servo. Parts of this page describe the old Servo design; see [Architecture](Architecture) and the [Module reference](Module-Reference) for the current one.


Every winit `WindowEvent` goes to `Browser::handle_window_event`, which decides who gets it. The order matters:

```
WindowEvent
  │
  ├─ window-level (close, resize, scale change, redraw, modifiers) → handled here
  │
  ├─ key press matching a browser shortcut → handle_shortcut, stop
  │
  ├─ egui.on_window_event  (the interface sees everything)
  │
  └─ forward_to_page  (the page gets what the interface didn't take)
```

## Shortcuts first

`handle_shortcut` runs before egui so shortcuts work even while typing in the address bar or a page. Cmd is used on macOS and Ctrl elsewhere (`cmd` in the code). The full list is in the README; adding one is a single match arm:

```rust
(_, "d") if cmd => self.toggle_bookmark(),
```

## What reaches the page

`forward_to_page` drops page input when:

- the active tab has crashed or has no webview,
- a page dialog or popup for this tab is open, or the main menu is open (`blocked`),
- for the keyboard: egui consumed the key or wants keyboard input (e.g. the address bar has focus),
- for mouse buttons: the pointer is over interface (`egui_is_using_pointer`) or outside the page area.

Mouse positions are converted from window pixels to webview pixels with `page_point` (see [Rendering pipeline](Rendering-Pipeline#coordinates)). When the pointer leaves the page area, Servo gets `MouseLeftViewport`, so hover effects end correctly.

Special cases:

- Mouse buttons 4/5 (Back/Forward) navigate instead of reaching the page.
- Ctrl/Cmd + wheel zooms instead of scrolling.
- Wheel line deltas are scaled to 76 px per line, matching Servo's own shell.
- A mouse press on the page removes focus from any egui text field, so typing goes to the page next.

## Keyboard conversion (keys.rs)

Servo wants W3C UI Events key events (`keyboard_types::KeyboardEvent`). winit's `KeyCode` and `NamedKey` enums use the same names as the W3C `code` and `key` values, so `keys.rs` converts by name:

```rust
Code::from_str(&format!("{code:?}"))      // KeyCode::KeyA  → Code::KeyA
NamedKey::from_str(&format!("{named:?}")) // NamedKey::Enter → NamedKey::Enter
```

and handles the few differences explicitly: winit's `Super*` is W3C `Meta*`, and Space is the character `" "`, not a named key. Dead keys fall back to their composed text when winit provides it. The unit test `names_line_up` guards against future renames in either crate.

## The address bar

The address bar is an egui `TextEdit` with id `"location"`:

- Clicking it (or Ctrl+L / F6) selects all text, done by writing a selection into the `TextEditState`.
- Typing sets `editing = true`, which stops `sync_location` from overwriting the text with the page URL.
- While editing, `draw_suggestions` shows up to 7 matches from `Profile::suggestions`; ↑/↓ move `suggestion_index`.
- Enter navigates to the selected suggestion, or turns the text into a URL/search with `location::input_to_url`. Esc restores the page URL.
