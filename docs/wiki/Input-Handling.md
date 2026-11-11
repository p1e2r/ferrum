# Input handling

## Who gets what

- **Over the interface** (tab strip, toolbar, panels, internal pages): winit delivers the events to `Browser::handle_window_event`, which passes them to egui.
- **Over the page:** the engine's native view gets the mouse and keyboard directly. Ferrum never forwards input to pages.
- **Over a popup:** the popup window's own egui context gets them (`Overlays::handle_event`).

## Shortcuts

Shortcuts are handled by `Browser::handle_shortcut(&Shortcut)` from two sources:

1. Ferrum's window: `shortcut_from` turns a winit `KeyEvent` into a `Shortcut` (key name, Ctrl/⌘/Shift/Alt).
2. A page that has the keyboard: the [page script](Page-Script) catches the same keys and sends `{t:"key"}`; the page never sees them. Ctrl+C/V/X/A/Z and normal typing stay with the page.

On macOS ⌘ is used where other systems use Ctrl.

## Keyboard focus

When egui wants the keyboard (address bar, find bar, settings fields), `Browser::layout_pages` takes it back from the page with `focus_interface`: wry's `focus_parent` on macOS and Windows; on Linux an explicit `XSetInputFocus` on Ferrum's window, because WebKitGTK keeps X11 focus in its child window. Showing a tab's page gives the page the keyboard again.

## Window edges

With the custom title bar (no system frame), `handle_window_edges` turns the 5 px around the window into resize handles (`drag_resize_window`), and the empty strip next to the tabs moves the window (`drag_window`; double-click maximises).
