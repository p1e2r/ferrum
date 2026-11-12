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

/// Cache settings for a new page view. In Lite mode, caches are kept
/// small and pages left aren't kept in memory.
#[cfg(target_os = "linux")]
pub fn tune(webview: &wry::WebView, lite: bool) {
    use webkit2gtk::{CacheModel, SettingsExt, WebContextExt, WebViewExt};
    use wry::WebViewExtUnix;
    let view = webview.webview();
    if let Some(context) = view.context() {
        context.set_cache_model(if lite { CacheModel::DocumentBrowser } else { CacheModel::WebBrowser });
    }
    if let Some(settings) = WebViewExt::settings(&view) {
        settings.set_enable_page_cache(!lite);
        settings.set_enable_smooth_scrolling(!lite);
    }
}

#[cfg(not(target_os = "linux"))]
pub fn tune(_webview: &wry::WebView, _lite: bool) {}

/// Ask the engine to use less memory for a page that isn't shown
/// (WebView2 only; WebKit manages hidden pages itself).
#[cfg(target_os = "windows")]
pub fn set_low_memory(webview: &wry::WebView, low: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL, ICoreWebView2_19,
    };
    use windows::core::Interface;
    use wry::WebViewExtWindows;
    if let Ok(core) = webview.webview().cast::<ICoreWebView2_19>() {
        let level = if low { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL };
        let _ = unsafe { core.SetMemoryUsageTargetLevel(level) };
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_low_memory(_webview: &wry::WebView, _low: bool) {}

/// Redraw a page view (after a popup over it closed).
#[cfg(target_os = "linux")]
pub fn repaint(webview: &wry::WebView) {
    use gtk::prelude::WidgetExt;
    use wry::WebViewExtUnix;
    // Asking for a redraw isn't always enough after the view was hidden;
    // a size change makes WebKitGTK lay out and draw everything again.
    if let Ok(bounds) = webview.bounds() {
        let size = bounds.size.to_physical::<u32>(1.0);
        if size.height > 1 {
            let mut smaller = bounds;
            smaller.size = dpi::PhysicalSize::new(size.width, size.height - 1).into();
            let _ = webview.set_bounds(smaller);
            let _ = webview.set_bounds(bounds);
        }
    }
    webview.webview().queue_draw();
}

#[cfg(not(target_os = "linux"))]
pub fn repaint(_webview: &wry::WebView) {}
