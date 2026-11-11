# Rendering pipeline

> **Note (0.7.0):** Ferrum now uses the system web engine through wry instead of Servo. Parts of this page describe the old Servo design; see [Architecture](Architecture) and the [Module reference](Module-Reference) for the current one.


Ferrum shows two things in one window: Servo's rendering of the page, and egui's interface. Both use OpenGL, and they share one GL context.

## Contexts

Set up in `Browser::new`:

```rust
let window_context = Rc::new(WindowRenderingContext::new(display, window_handle, size)?);
let page_context   = Rc::new(window_context.offscreen_context(size));
```

- **`WindowRenderingContext`** wraps the window's GL surface (via surfman). This is what finally appears on screen.
- **`OffscreenRenderingContext`** is a framebuffer object that belongs to the same GL context. Every webview is built with this context, so Servo/WebRender draws pages into it, not directly to the window.
- **egui** is created on the same GL API (`page_context.glow_gl_api()`), so it can read Servo's framebuffer without copies between contexts.

## Per frame

Inside `Browser::draw_content` (ui.rs), for a tab with a live webview:

```rust
// 1. Size the page to the free area left after the toolbars and side panel.
if webview.size() != wanted { webview.resize(wanted) }

// 2. Let WebRender composite the latest page frame into the offscreen framebuffer.
webview.paint();

// 3. Ask egui to blit that framebuffer into the page rectangle when it paints.
if let Some(render) = page_context.render_to_parent_callback() {
    painter.add(PaintCallback { rect, callback: CallbackFn::new(move |info, p| render(p.gl(), target_rect)) });
}
```

The callback is added on egui's background layer, so all interface elements (popups, dialogs, the status bubble) draw over the page.

`render_to_parent_callback` is fetched **after** `resize`, because resizing recreates the framebuffer.

Then `Browser::redraw` finishes the frame:

```rust
page_context.make_current();
window_context.prepare_for_rendering();
egui.paint(&window);       // draws the interface; runs the blit callback in order
window_context.present();  // swap buffers
```

## Coordinates

Three coordinate spaces are involved:

| Space | Unit | Used by |
|---|---|---|
| egui points | logical pixels | layout (`content_rect`) |
| window physical pixels | device pixels | winit mouse events |
| webview device pixels | device pixels, origin at page top-left | Servo input events, popup positions |

`ppp` (egui's pixels-per-point, which equals the window scale factor) converts between them. `Browser::page_point` turns a window position into a webview point by subtracting the page area's origin; `draw_page_controls` does the reverse to place `<select>` popups under their element.

## Start page, crash panel, fullscreen

- A tab without a webview draws the start page with egui into the page area — no GL blit.
- A crashed tab draws the crash panel instead of blitting stale content.
- In fullscreen (F11 or a page's Fullscreen API request), the tab strip, toolbar and bookmarks bar are skipped, so `content_rect` becomes the whole window and the page is resized to fill it.

## Favicons

`notify_favicon_changed` marks the tab's favicon dirty; on the next frame `refresh_favicon` converts Servo's `Image` (RGBA8, BGRA8, RGB8, K8 or KA8) to RGBA and uploads it as an egui texture once. It is not re-uploaded on later frames.
