# Pages and popups

## The page view

Each tab owns a `wry::WebView`, created with `WebViewBuilder::build_as_child` on the main window: a native view of the system engine (WKWebView, WebView2, WebKitGTK) placed inside Ferrum's window.

Each frame:

1. egui draws the interface. The area left for the page is stored as `Browser::content_rect` (`draw_content` paints it white so resizing never flashes).
2. `Browser::layout_pages` converts that rect to physical pixels and, for the **active** tab only, calls `set_bounds` (when it changed) and `set_visible(true)`. Every other tab's view is hidden, which also lets the engine throttle it.
3. Internal pages (settings, shortcuts, page source), reader mode and the start page have no visible view; egui draws them in the page area.

Because the page is a native view, it sits **above** anything egui paints in the main window. That's why the loading bar runs along the bottom of the toolbar, and why popups are windows of their own.

## Popups that float over the page

`overlay.rs` gives each popup a small borderless window, created as a child of the main window (Windows, X11) or a child `NSWindow` (macOS), sharing the main window's OpenGL context (`GlWindow::create_popup`). Each has its own egui context.

Per frame:

1. While drawing, `Browser::request_popups` (overlay_ui.rs) lists the popups that should be open and where, in main-window points: menu, address-bar suggestions, site info, a tab's menu, toasts, web notifications, the "allow notifications?" prompt and the blocked-search-engine bar.
2. `Overlays::sync` closes popups no longer wanted, creates new ones, and moves/resizes the rest. A new popup starts as a 1×1 window and is sized to its contents after its first draw — never with `set_visible`, which on macOS would make it the key window and take the keyboard from the address bar.
3. `Overlays::draw` paints each popup with `Browser::draw_overlay`.

Input to a popup window goes to its own egui context (`Overlays::handle_event`). A click in the main window or on the page (reported by the page script) closes the menus.

## Light and dark pages

`Browser::apply_page_theme` makes pages' `prefers-color-scheme` follow the interface: the window appearance on macOS, `set_theme` on WebView2, GTK's dark preference on Linux. With the Ferrum theme on "System", it follows the OS.
