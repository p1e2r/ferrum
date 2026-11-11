# Ferrum developer wiki

Ferrum is a desktop web browser with a native Rust interface. Web pages are shown by the **system's own web engine** through [wry](https://github.com/tauri-apps/wry) — WebKit on macOS, WebView2 (Chromium) on Windows, WebKitGTK on Linux. Everything around the page — tabs, toolbar, address bar, menus, side panels and popups — is drawn by Ferrum itself with [egui](https://github.com/emilk/egui). There is no HTML, CSS or JavaScript in the application's interface.

These pages explain how the code works, so you can find your way around and change it.

## Pages

| Page | What it covers |
|---|---|
| [Architecture](Architecture) | The big picture: event loop, a frame, platforms |
| [Module reference](Module-Reference) | Every source file and its main types and functions |
| [Pages and popups](Pages-and-Popups) | How the engine's page view and Ferrum's interface share one window, and how popups float over the page |
| [Page script](Page-Script) | The script added to every page: shortcuts, notifications, ad blocking, messages to Ferrum |
| [Input handling](Input-Handling) | Mouse, keyboard, shortcuts, and who gets each event |
| [Profile and data](Profile-and-Data) | Settings, bookmarks, history, session, cookies — file formats and saving |
| [Downloads](Downloads) | How downloads are detected, run and shown |
| [Building and releasing](Building) | Build scripts, cross-compiling from a Mac, CI and nightlies |
| [Contributing](Contributing) | Code style, adding a feature, testing |

## Thirty-second tour

```
main.rs          creates the winit event loop and hands every event to Browser
browser.rs       Browser: the window, tabs (each with a wry WebView), engine messages, shortcuts
gl.rs            the window's OpenGL surface (glutin), shared with popup windows
ui.rs            Browser::draw(): tab strip, toolbar, panels, start page, with egui
overlay.rs       small borderless windows that float over the page
overlay_ui.rs    what they show: menu, suggestions, site info, tab menu, toasts, notifications
pagescript.rs    the script added to every page
adblock.rs       engine-level ad blocking (per platform); blocker.rs keeps the list
settings_page.rs ferrum://settings      features_ui.rs  reader mode, ferrum://shortcuts
profile.rs       settings, bookmarks, history, session    downloads.rs  download threads
find.rs          find in page            reader.rs  reader mode     devtools.rs  page source
location.rs      address-bar text → URL or search         icons.rs  vector icons
notify.rs        system notifications    updates.rs  update check   bookmarks_io.rs  import/export
```

## Key ideas

- **One main thread.** The engine runs its own processes; download and favicon threads wake the main thread with a `WakeUp` event and never touch Ferrum's state.
- **Nothing is drawn unless something changed.** The loop sleeps until input, a wake-up or the next scheduled repaint (on Linux it also wakes every 16 ms for GTK's main loop).
- **Engine callbacks only queue work.** wry's handlers push an `Event`; `Browser::pump` processes the queue.
- **The page is a native view.** Ferrum moves it over the page area each frame; anything that must appear on top of it is a separate popup window.
