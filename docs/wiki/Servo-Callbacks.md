# Servo callbacks and page controls

## The delegate

Servo reports what happens in pages through the `WebViewDelegate` trait. Ferrum implements it once, in `Delegate` (browser.rs), and shares it between all webviews (popups get their parent's delegate).

Callbacks run on the main thread but in the middle of `servo.spin_event_loop()`, while `Browser` is busy. So the delegate never touches `Browser`. It either just requests a redraw, or pushes an `Event` onto its queue:

```rust
enum Event {
    Opened(WebView),            // a page opened a new window → new tab
    Closed(WebViewId),          // window.close()
    Control(Pending),           // a dialog/popup to show (below)
    HideControl(EmbedderControlId),
    FilePicker(FilePicker),
    Crashed(WebViewId, String),
    Fullscreen(bool),           // Fullscreen API
    Loaded(WebView),            // load complete → record history
    Title(WebView),             // title changed → update history entry
    Favicon(WebViewId),
    Download(Url, Option<Url>), // navigation to a file → download
}
```

`Browser::pump` drains the queue after `spin_event_loop` returns.

| Callback | What Ferrum does |
|---|---|
| `notify_new_frame_ready`, `notify_url_changed`, `notify_status_text_changed`, `notify_history_changed` | redraw |
| `notify_load_status_changed` | redraw; on `Complete` record a history visit, or recover a failed load (below) |
| `notify_page_title_changed` | update the history entry's title |
| `notify_favicon_changed` | mark the tab's favicon for re-upload |
| `notify_cursor_changed` | set the window cursor (active tab only) |
| `notify_fullscreen_state_changed` | enter/leave borderless fullscreen, hide the chrome |
| `notify_crashed` | show the tab's crash panel |
| `notify_closed` | close that tab |
| `request_create_new` | build the new webview, open it as a tab next to its opener |
| `request_navigation` | allow, unless it looks like a file → deny and download |
| `request_permission`, `request_authentication` | queue a page control |
| `show_embedder_control` / `hide_embedder_control` | queue / remove a page control |

## Page controls

Some things a page asks for need the user: `alert()`, a `<select>` dropdown, a sign-in prompt. Servo hands Ferrum a request object that must be answered. These are stored as:

```rust
pub enum PageControl {
    Dialog { dialog: SimpleDialog, input: String }, // alert / confirm / prompt
    Auth { request, user, pass },                   // HTTP sign-in
    Permission(PermissionRequest),                  // camera, location, …
    Select(SelectElement),                          // <select>
    Color { picker, color },                        // <input type=color>
    ContextMenu(ContextMenu),                       // right-click
}
pub struct Pending { webview: WebViewId, id: Option<EmbedderControlId>, control: PageControl }
```

`Browser::controls` holds every pending request for every tab. `draw_page_controls` (ui.rs) shows the first one belonging to the **active** tab; others wait until their tab is selected.

### Answering

Each frame the control is drawn and produces a `ControlOutcome` (`Keep`, `Confirm`, `Dismiss`, `Select(ids)`, `Menu(action)`, …). Anything other than `Keep` removes the `Pending` from the list and calls the matching method on Servo's request object — `confirm()`, `dismiss()`, `authenticate()`, `allow()`, `select()` + `submit()`, etc.

**Dropping a request answers it with "cancel".** Servo's request types implement `Drop`, so a request that is simply removed — tab closed, Esc pressed, clicked outside a dropdown — is safely declined. `dismiss_popups` relies on this.

### Where they appear

- `alert`/`confirm`/`prompt` and sign-in use `egui::Modal`, titled "*host* says" / "*host* is asking you to sign in", so page text can't pass itself off as browser UI.
- Permission requests show a bar at the top of the page area with Block / Allow.
- `<select>` and colour pickers appear directly under the element, using the element rectangle Servo provides (converted from webview pixels).
- The context menu opens at the mouse position. It shows Servo's items, with link/image items handled by Ferrum (open in a background tab, copy, save), plus "Save link as…", "Save image as…" and "Bookmark this page".

### File pickers

`<input type=file>` isn't queued: `run_file_picker` opens the system file dialog through [rfd](https://github.com/PolyMeilie/rfd) right away (on Linux, through the XDG desktop portal), applying the page's `accept` extensions as a filter.

### Input methods

`EmbedderControl::InputMethod` (on-screen keyboard requests) is ignored on desktop.

## Failed loads

When Servo can't connect it loads its own error document (title "Error loading page", text "Could not load the requested page: …"). On `Complete`, `Browser::recover_failed_load` checks for that title and:

1. if Ferrum itself upgraded a typed host to `https://` (`Tab::upgraded_to_https`), loads the `http://` version instead — for sites and home servers without HTTPS;
2. otherwise retries once (`Tab::retries_left`), which covers brief network hiccups.

Servo disables "happy eyeballs" and splits `network_connection_timeout` across every address a host resolves to, so Ferrum raises that preference from 15 to 30 seconds to avoid spurious connect errors on networks with slow or broken IPv6.

## Engine features and preferences

`Cargo.toml` enables Servo's `webgl`, `subtlecrypto` and `brotli-compression-stream` features, which aren't in Servo's defaults. `Browser::new` also sets `dom_webgl2_enabled = true` (current three.js needs WebGL 2) and the longer connection timeout.
