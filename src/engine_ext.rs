// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Things the system engines can do that wry doesn't expose.
//!
//! - `hard_reload`: reload the page without the cache (Ctrl/⌘+Shift+R,
//!   Shift+F5): WebKit's `reloadFromOrigin` on macOS, WebKitGTK's
//!   `reload_bypass_cache` on Linux, and the DevTools `Page.reload` command
//!   with `ignoreCache` on Windows (WebView2 has no direct call for it).
//! - `tune`: cache settings for a new page view. Every engine keeps a
//!   persistent disk cache of pages, scripts and images in the profile
//!   (macOS: WebKit's own store), and a back/forward cache that keeps
//!   recently visited pages ready so Back and Forward are instant.

#[cfg(target_os = "macos")]
pub fn hard_reload(webview: &wry::WebView) {
    use wry::WebViewExtMacOS;
    let view = webview.webview();
    unsafe {
        let _ = view.reloadFromOrigin();
    }
}

#[cfg(target_os = "linux")]
pub fn hard_reload(webview: &wry::WebView) {
    use webkit2gtk::WebViewExt;
    use wry::WebViewExtUnix;
    webview.webview().reload_bypass_cache();
}

#[cfg(target_os = "windows")]
pub fn hard_reload(webview: &wry::WebView) {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::HSTRING;
    use wry::WebViewExtWindows;
    let core = webview.webview();
    let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(())));
    let ok = unsafe { core.CallDevToolsProtocolMethod(&HSTRING::from("Page.reload"), &HSTRING::from(r#"{"ignoreCache":true}"#), &handler) };
    if ok.is_err() {
        let _ = webview.reload();
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn hard_reload(webview: &wry::WebView) {
    let _ = webview.reload();
}

/// Cache settings for a new page view.
#[cfg(target_os = "linux")]
pub fn tune(webview: &wry::WebView) {
    use webkit2gtk::{CacheModel, SettingsExt, WebContextExt, WebViewExt};
    use wry::WebViewExtUnix;
    let view = webview.webview();
    // Keep as much of pages in memory and on disk as a browser should.
    if let Some(context) = view.context() {
        context.set_cache_model(CacheModel::WebBrowser);
    }
    if let Some(settings) = WebViewExt::settings(&view) {
        settings.set_enable_page_cache(true);
        settings.set_enable_dns_prefetching(true);
    }
}

#[cfg(not(target_os = "linux"))]
pub fn tune(_webview: &wry::WebView) {}
