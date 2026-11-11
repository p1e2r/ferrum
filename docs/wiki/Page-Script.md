# Page script

`pagescript.rs` builds a script that wry adds to every page before the page's own scripts run (`with_initialization_script`). It talks to Ferrum with `window.ipc.postMessage(JSON)`; `Browser::handle_page_message` handles the messages.

| Message | Meaning |
|---|---|
| `{t:"key", key, ctrl, meta, shift, alt}` | A Ferrum shortcut was pressed while the page had the keyboard (Ctrl/⌘ + T, W, L, R, F, D, P, U, 1–9…, F3/F5/F6/F9/F11/F12, Alt+←/→, Ctrl+Tab). The script stops the page from seeing it. |
| `{t:"click"}` | The page was clicked — close Ferrum's popups. |
| `{t:"icon", href}` | The page's icon, downloaded on a background thread for the tab. |
| `{t:"media", playing}` | Audio or video started or stopped (speaker icon on the tab). |
| `{t:"blocked", count}` | How many ad/tracker requests the script stopped on this page. |
| `{t:"notify", title, body}` | The page showed a web notification. |
| `{t:"permission"}` | The page asked to show notifications. |

The script also handles the mouse back/forward buttons.

## Web notifications

Not every system engine supports the Notification API for embedded views, so the script defines `window.Notification` itself. `Notification.requestPermission()` asks Ferrum; Ferrum answers from the remembered permissions in settings, or shows the "wants to show notifications" popup, then calls back `window.__ferrumPermission("granted" | "denied")`. A shown notification becomes a card in the window, and a system notification (`notify.rs`) when Ferrum isn't the front window.

## Ad and tracker blocking

Two layers, both using the same list (`blocker.rs`: ~170 built-in domains plus Peter Lowe's list, cached in the profile and refreshed weekly):

1. **In the engine** (`adblock.rs`), so requests are never sent: WebKit content rule lists on macOS, WebKitGTK user content filters on Linux (both Safari's rule format, built by `rules_json`), and a `WebResourceRequested` filter on Windows that answers blocked requests with an empty 403. Rules only match requests to *another* site, and skip sites where blocking is switched off.
2. **In the page script**, which also catches what scripts add later: it neutralises `<script>`, `<iframe>` and `<img>` pointing at listed servers, and blocks `fetch`, `XMLHttpRequest` and `sendBeacon` to them. It counts what it blocks for the padlock badge.
