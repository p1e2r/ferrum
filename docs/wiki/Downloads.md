# Downloads

Ferrum runs downloads itself (`downloads.rs`), using [ureq](https://github.com/algesten/ureq) with rustls, so every platform shows the same progress, cancel and open buttons. When the engine starts a download (`with_download_started_handler`), or a link points at a file (`with_navigation_handler` + `looks_like_download`), Ferrum cancels the engine's download and runs its own.

## How a download starts

1. **Navigation to a file.** `Delegate::request_navigation` checks `looks_like_download(url)` — whether the path ends in a file extension such as `.zip`, `.exe`, `.dmg`, `.pdf`, `.mp4` (list in `downloads.rs`). If so, it denies the navigation (the page stays put) and queues `Event::Download`.
2. **Context menu.** "Save link as…" / "Save image as…" call `Browser::start_download` directly.

`start_download` copies the site's cookies from the engine (`WebView::cookies_for_url`) into a `Cookie` header and sends the page as `Referer`, so downloads behind a login work. If "Ask where to save each file" is on, a system save dialog picks the target.

## Running

`Downloads::start` creates a shared `Progress` (behind a `Mutex`) and a cancel flag, then spawns a thread that:

1. sends the request (20 s connect timeout),
2. picks the file name from `Content-Disposition` (both `filename=` and `filename*=UTF-8''…`), else the URL's last segment,
3. sanitises it (no path separators or reserved characters) and avoids overwriting: `file.zip`, `file (1).zip`, …,
4. streams the body in 256 KB chunks to `name.ext.part`, checking the cancel flag between chunks,
5. renames `.part` to the final name when complete.

Progress is published at most every 250 ms, each time calling `notify`, which sends `WakeUp` to the main thread so the panel redraws. Memory use stays flat regardless of file size.

## Showing

The downloads panel (Ctrl+J, or the toolbar button with its counter badge) lists every download with a progress bar, Cancel while running, and Open / Show in folder when done. `open_path` uses `open` (macOS), `explorer` (Windows) or `xdg-open` (Linux).

## Limitations

- Downloads that need the page's POST data or a one-time token tied to the engine's session may fail when re-requested by Ferrum's downloader.
- Downloads use the page's cookies but not its exact HTTP stack, so sites that check things like TLS fingerprints may behave differently.
