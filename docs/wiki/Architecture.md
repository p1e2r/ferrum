# Architecture

## Components

```
┌──────────────────────────── Ferrum process ────────────────────────────┐
│                                                                        │
│  main thread                                                           │
│  ┌──────────────┐  WindowEvent   ┌──────────────────────────────────┐  │
│  │ winit        │ ─────────────▶ │ Browser (browser.rs)             │  │
│  │ event loop   │  WakeUp        │  ├─ Servo + WebViews (tabs)      │  │
│  │ (main.rs)    │ ─────────────▶ │  ├─ EguiGlow (interface, ui.rs)  │  │
│  └──────────────┘                │  ├─ Profile (profile.rs)         │  │
│         ▲                        │  ├─ Downloads (downloads.rs)     │  │
│         │ WakeUp                 │  └─ Delegate ◀── Servo callbacks │  │
│         │                        └──────────────────────────────────┘  │
│  ┌──────┴───────────┐   ┌──────────────────────┐                       │
│  │ Servo's threads  │   │ download threads     │                       │
│  │ (layout, script, │   │ (one per download)   │                       │
│  │  network, …)     │   └──────────────────────┘                       │
│  └──────────────────┘                                                  │
└────────────────────────────────────────────────────────────────────────┘
```

- **`App`** (`main.rs`) is the winit `ApplicationHandler`. It creates the `Browser` when the event loop starts and forwards window events and wake-ups to it.
- **`Browser`** (`browser.rs`) owns everything: the window, the OpenGL contexts, the `Servo` instance, the list of `Tab`s, the user profile, downloads and all interface state.
- **`Delegate`** implements Servo's `WebViewDelegate`. Servo calls it on the main thread when something happens in a page. It is shared by every webview.
- **`ui.rs`** adds `Browser::draw` and friends — the interface, rebuilt every frame by egui.

## Threads and waking up

Servo does its work (parsing, script, layout, networking) on its own threads. When it has results for the main thread it calls `EventLoopWaker::wake`, which Ferrum implements (`Waker` in `main.rs`) by sending a `WakeUp` user event to winit. Download threads do the same through the `Notify` callback they are given.

On `WakeUp`, `App::user_event` calls `Browser::pump`, which:

1. calls `servo.spin_event_loop()` so Servo delivers its pending callbacks,
2. drains the `Delegate`'s event queue and acts on each `Event`,
3. writes the profile to disk if changes have been waiting long enough.

`pump` is also called at the start of every window event and every redraw, so state is always fresh before input is routed or the UI is drawn.

## The life of a frame

Nothing redraws on a timer. A redraw happens when something calls `window.request_redraw()`:

- Servo has a new page frame (`notify_new_frame_ready`) or page state changed (title, URL, load status…),
- egui needs a repaint (hover effects, animations, typing),
- the progress bar or toast is animating (they request repaints at ~30 fps only while visible),
- a download made progress (throttled to 4 updates a second).

`WindowEvent::RedrawRequested` → `Browser::redraw`:

1. `pump()` and `sync_location()` (refresh the address bar from the active tab unless the user is editing it),
2. take the `EguiGlow` out of `self` (so the drawing closure can borrow `self` mutably) and run `Browser::draw` inside `egui.run`,
3. `draw` lays out tab strip, toolbar, bookmarks bar, side panel, then the page area; for a live tab it resizes the webview to the page area, calls `webview.paint()` and registers a paint callback that copies Servo's framebuffer into that rectangle (see [Rendering pipeline](Rendering-Pipeline)),
4. dialogs, popups, the menu and toasts are drawn on top,
5. `egui.paint` renders everything to the window's GL surface, then `present()` swaps buffers.

## Tabs

```rust
pub struct Tab {
    pub webview: Option<WebView>,   // None = the native start page
    pub crashed: Option<String>,    // Some = show the crash panel
    pub zoom: f32,
    pub favicon: Option<egui::TextureHandle>,
    pub favicon_dirty: bool,
}
```

A tab with no webview shows the start page, which is drawn by Ferrum, so empty tabs cost no engine resources. Navigating from the start page creates the webview. Only the active tab's webview is `show()`n; the others are `hide()`n, which lets Servo throttle them.

## Why egui

egui is immediate-mode: the UI is a function of state, recomputed each frame. That keeps interface code short and avoids a second copy of state in widget objects. Because Ferrum only redraws on change, the cost of rebuilding the UI is paid only when needed — usually well under a millisecond.

## Error handling philosophy

- Failing to create the window or GL context is fatal (`expect`) — there is nothing useful to do without them.
- Everything else degrades gracefully: unreadable profile files fall back to defaults, failed downloads show "Failed: …", crashed tabs show a reload button, missing favicons fall back to letter tiles.
