# Ferrum developer wiki

Ferrum is a native desktop web browser written in Rust. Web pages are rendered by the [Servo](https://servo.org) engine; everything around the page — tabs, toolbar, address bar, menus, side panels and the dialogs pages ask for — is drawn by Ferrum itself with [egui](https://github.com/emilk/egui). There is no HTML, CSS or JavaScript in the application.

These pages explain how the code works, so you can find your way around and change it.

## Pages

| Page | What it covers |
|---|---|
| [Architecture](Architecture) | The big picture: event loop, threads, how a frame is drawn |
| [Module reference](Module-Reference) | Every source file and its main types and functions |
| [Rendering pipeline](Rendering-Pipeline) | How Servo's page and egui's interface end up in one window |
| [Input handling](Input-Handling) | Mouse, keyboard, shortcuts, and who gets each event |
| [Servo callbacks and page controls](Servo-Callbacks) | `WebViewDelegate`, the event queue, dialogs, dropdowns, context menus |
| [Profile and data](Profile-and-Data) | Settings, bookmarks, history, session, cookies — file formats and saving |
| [Downloads](Downloads) | How downloads are detected, run and shown |
| [Building and releasing](Building) | Build scripts, cross-compiling from a Mac, CI |
| [Contributing](Contributing) | Code style, adding a feature, testing |

## Thirty-second tour

```
main.rs      creates the winit event loop and hands every event to Browser
browser.rs   Browser: owns Servo, the window, tabs and all state
             Delegate: receives Servo's callbacks and queues them
ui.rs        Browser::draw(): builds the whole interface each frame with egui
profile.rs   loads/saves settings, bookmarks, history and the session
downloads.rs background download threads
find.rs      find in page (highlighting script + state)
print.rs     page capture → PDF → printer
location.rs  turns address-bar text into a URL or a search
keys.rs      winit key events → W3C key events for Servo
icons.rs     vector-drawn toolbar icons
settings_page.rs  ferrum://settings
features_ui.rs    console panel, site info, notifications, reader, ferrum://shortcuts
blocker.rs   ad and tracker blocking
reader.rs    reader mode
devtools.rs  console and page source
bookmarks_io.rs   bookmark import/export
updates.rs   update check
```

## Key ideas

- **Everything runs on the main thread** except Servo's own internal threads and download threads. Those threads never touch Ferrum's state directly; they wake the main thread with a `WakeUp` event.
- **Nothing is drawn unless something changed.** Servo, egui and the download threads request redraws; there is no frame loop.
- **Servo callbacks only queue work.** `Delegate` pushes an `Event` and asks for a redraw; `Browser::pump` processes the queue.
- **The interface is immediate-mode.** `Browser::draw` describes the whole UI from current state every frame; there are no widget objects to keep in sync.
