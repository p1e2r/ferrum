# Architecture

Ferrum is one process with one main thread.

```
winit event loop (main.rs)
 └─ Browser (browser.rs)
     ├─ GlWindow (gl.rs)        the window + OpenGL surface; egui draws the interface here
     ├─ tabs: each Tab owns a wry::WebView, a native child view placed over the page area
     ├─ Overlays (overlay.rs)   small borderless windows above the page for popups
     ├─ page script (pagescript.rs) added to every page; talks back with window.ipc.postMessage
     └─ adblock.rs              engine-level blocking (WebKit content rules / WebView2 filter)
```

## A frame

1. `Browser::redraw` runs egui over the main window: tab strip, toolbar, side panels, and either an internal page (settings, shortcuts, source, reader mode, start page) or an empty page area.
2. While drawing, the interface lists the popups it wants (`overlay_requests`), e.g. the menu under the ☰ button.
3. `layout_pages` moves the active tab's web view over the page area (`set_bounds`) and hides the others.
4. `Overlays::sync` creates, moves or closes popup windows to match, and `draw` paints each one with its own egui context, sharing the main OpenGL context.

Nothing is drawn unless something changed; the loop sleeps until input, a wake-up from another thread, or the next scheduled repaint. On Linux it also wakes every 16 ms to run GTK's main loop, which WebKitGTK needs.

## Talking to the engine

wry's callbacks (title changed, page loaded, new window, download, IPC message) only queue an `Event`; `Browser::pump` handles them. The page script reports keyboard shortcuts pressed in the page (the page's native view has the keyboard), mouse back/forward buttons, clicks (to close popups), the page icon, whether media is playing, blocked trackers, and web notifications (the script provides `window.Notification`).

## Platforms

| | Engine | Popups | Ad blocking |
|---|---|---|---|
| macOS | WKWebView | child NSWindows | WKContentRuleList |
| Windows | WebView2 | child windows | WebResourceRequested filter |
| Linux (X11) | WebKitGTK | X11 child windows | WebKitUserContentFilter |
