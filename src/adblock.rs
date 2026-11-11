// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Ad and tracker blocking inside the web engine, so blocked requests are
//! never sent:
//!
//! * **macOS** — WebKit content rule lists (Safari's content blockers),
//! * **Linux** — WebKitGTK user content filters (the same rule format),
//! * **Windows** — WebView2's request filter: every request is checked and
//!   blocked ones answered with an empty 403.
//!
//! Rules match requests to listed servers made by pages on *other* sites
//! ("third-party"), and are skipped for sites where blocking is switched off.
//! The page script (pagescript.rs) also blocks and counts what it sees.

/// Safari / WebKit content-blocker rules for the domains.
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub fn rules_json(domains: &[String], exceptions: &[String]) -> String {
    let unless: Vec<String> = exceptions.iter().map(|e| format!("*{e}")).collect();
    let mut rules = Vec::with_capacity(domains.len());
    for domain in domains {
        let escaped: String = domain
            .chars()
            .flat_map(|c| if c == '.' { vec!['\\', '.'] } else if c.is_ascii_alphanumeric() || c == '-' { vec![c] } else { vec![] })
            .collect();
        let mut trigger = serde_json::json!({
            "url-filter": format!("^[a-z]+://([^/:]*\\.)?{escaped}[:/]"),
            "load-type": ["third-party"],
        });
        if !unless.is_empty() {
            trigger["unless-domain"] = serde_json::json!(unless);
        }
        rules.push(serde_json::json!({ "trigger": trigger, "action": { "type": "block" } }));
    }
    serde_json::Value::Array(rules).to_string()
}

#[cfg(target_os = "linux")]
mod imp {
    use std::cell::RefCell;
    use std::ffi::CString;

    use webkit2gtk::WebViewExt;
    use webkit2gtk::glib::translate::ToGlibPtr;
    use wry::WebViewExtUnix;

    type Filter = *mut webkit2gtk::ffi::WebKitUserContentFilter;
    type Manager = webkit2gtk::UserContentManager;

    thread_local! {
        /// The compiled filter (null until ready), web views waiting for it, and whether blocking is on.
        static STATE: RefCell<(Filter, Vec<Manager>, bool)> = const { RefCell::new((std::ptr::null_mut(), Vec::new(), true)) };
    }

    pub fn prepare(domains: &[String], exceptions: &[String], enabled: bool) {
        STATE.with(|s| s.borrow_mut().2 = enabled);
        if !enabled {
            return;
        }
        let json = super::rules_json(domains, exceptions);
        let dir = std::env::temp_dir().join(format!("ferrum-content-filters-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let Ok(path) = CString::new(dir.to_string_lossy().as_bytes()) else { return };
        let id = CString::new("ferrum-blocker").expect("identifier");
        unsafe extern "C" fn done(
            store: *mut webkit2gtk::glib::gobject_ffi::GObject,
            result: *mut webkit2gtk::gio::ffi::GAsyncResult,
            _data: webkit2gtk::glib::ffi::gpointer,
        ) {
            let mut error = std::ptr::null_mut();
            let filter = unsafe {
                webkit2gtk::ffi::webkit_user_content_filter_store_save_finish(store as *mut _, result, &mut error)
            };
            if filter.is_null() {
                log::warn!("Could not compile the content blocker");
                return;
            }
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                if !s.0.is_null() {
                    unsafe { webkit2gtk::ffi::webkit_user_content_filter_unref(s.0) };
                }
                s.0 = filter;
                for manager in s.1.drain(..) {
                    let ptr: *mut webkit2gtk::ffi::WebKitUserContentManager = manager.to_glib_none().0;
                    unsafe { webkit2gtk::ffi::webkit_user_content_manager_add_filter(ptr, filter) };
                }
            });
            log::info!("Content blocker ready");
        }
        unsafe {
            let store = webkit2gtk::ffi::webkit_user_content_filter_store_new(path.as_ptr());
            let bytes = webkit2gtk::glib::ffi::g_bytes_new(json.as_ptr() as *const _, json.len());
            webkit2gtk::ffi::webkit_user_content_filter_store_save(store, id.as_ptr(), bytes, std::ptr::null_mut(), Some(done), std::ptr::null_mut());
            webkit2gtk::glib::ffi::g_bytes_unref(bytes);
        }
    }

    pub fn attach(webview: &wry::WebView) {
        let Some(manager) = webview.webview().user_content_manager() else { return };
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if !s.2 {
                return;
            }
            if s.0.is_null() {
                s.1.push(manager);
            } else {
                let ptr: *mut webkit2gtk::ffi::WebKitUserContentManager = manager.to_glib_none().0;
                unsafe { webkit2gtk::ffi::webkit_user_content_manager_add_filter(ptr, s.0) };
            }
        });
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::cell::RefCell;

    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_foundation::{NSError, NSString};
    use objc2_web_kit::{WKContentRuleList, WKContentRuleListStore, WKUserContentController};
    use wry::WebViewExtMacOS;

    thread_local! {
        static STATE: RefCell<(Option<Retained<WKContentRuleList>>, Vec<Retained<WKUserContentController>>, bool)> =
            const { RefCell::new((None, Vec::new(), true)) };
    }

    pub fn prepare(domains: &[String], exceptions: &[String], enabled: bool) {
        STATE.with(|s| s.borrow_mut().2 = enabled);
        if !enabled {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else { return };
        let json = NSString::from_str(&super::rules_json(domains, exceptions));
        let id = NSString::from_str("ferrum-blocker");
        let block = block2::RcBlock::new(|list: *mut WKContentRuleList, _error: *mut NSError| {
            let Some(list) = (unsafe { Retained::retain(list) }) else {
                log::warn!("Could not compile the content blocker");
                return;
            };
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                for manager in s.1.drain(..) {
                    unsafe { manager.addContentRuleList(&list) };
                }
                s.0 = Some(list);
            });
            log::info!("Content blocker ready");
        });
        unsafe {
            if let Some(store) = WKContentRuleListStore::defaultStore(mtm) {
                store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(Some(&id), Some(&json), Some(&block));
            }
        }
    }

    pub fn attach(webview: &wry::WebView) {
        let manager = webview.manager();
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if !s.2 {
                return;
            }
            match &s.0 {
                Some(list) => unsafe { manager.addContentRuleList(list) },
                None => s.1.push(manager),
            }
        });
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::rc::Rc;

    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::{WebResourceRequestedEventHandler, take_pwstr};
    use windows::core::{HSTRING, PWSTR};
    use wry::WebViewExtWindows;

    thread_local! {
        static STATE: RefCell<(Rc<HashSet<String>>, Rc<Vec<String>>, bool)> = RefCell::new(Default::default());
    }

    pub fn prepare(domains: &[String], exceptions: &[String], enabled: bool) {
        STATE.with(|s| *s.borrow_mut() = (Rc::new(domains.iter().cloned().collect()), Rc::new(exceptions.to_vec()), enabled));
    }

    fn host(url: &str) -> String {
        url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_ascii_lowercase)).unwrap_or_default()
    }

    fn listed(domains: &HashSet<String>, host: &str) -> bool {
        let mut rest = host;
        loop {
            if domains.contains(rest) {
                return true;
            }
            match rest.find('.') {
                Some(i) if rest[i + 1..].contains('.') => rest = &rest[i + 1..],
                _ => return false,
            }
        }
    }

    pub fn attach(webview: &wry::WebView) {
        let (domains, exceptions, enabled) = STATE.with(|s| s.borrow().clone());
        if !enabled || domains.is_empty() {
            return;
        }
        let core = webview.webview();
        let env = webview.environment();
        unsafe {
            if core.AddWebResourceRequestedFilter(&HSTRING::from("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL).is_err() {
                return;
            }
            let mut token = Default::default();
            let _ = core.add_WebResourceRequested(
                &WebResourceRequestedEventHandler::create(Box::new(move |sender, args| {
                    let (Some(sender), Some(args)) = (sender, args) else { return Ok(()) };
                    let page = {
                        let mut uri = PWSTR::null();
                        sender.Source(&mut uri)?;
                        take_pwstr(uri)
                    };
                    let request = {
                        let mut uri = PWSTR::null();
                        args.Request()?.Uri(&mut uri)?;
                        take_pwstr(uri)
                    };
                    let (page_host, request_host) = (host(&page), host(&request));
                    let third_party = crate::blocker::base_domain(&page_host) != crate::blocker::base_domain(&request_host);
                    if third_party && !page_host.is_empty() && !exceptions.iter().any(|e| *e == page_host) && listed(&domains, &request_host) {
                        let response = env.CreateWebResourceResponse(None, 403, &HSTRING::from("Blocked"), &HSTRING::from(""))?;
                        args.SetResponse(&response)?;
                    }
                    Ok(())
                })),
                &mut token,
            );
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod imp {
    pub fn prepare(_: &[String], _: &[String], _: bool) {}
    pub fn attach(_: &wry::WebView) {}
}

pub use imp::{attach, prepare};

#[cfg(test)]
mod tests {
    #[test]
    fn rules_are_json() {
        let json = super::rules_json(&["ads.example.com".into()], &["news.site".into()]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v[0]["action"]["type"], "block");
        assert_eq!(v[0]["trigger"]["url-filter"], "^[a-z]+://([^/:]*\\.)?ads\\.example\\.com[:/]");
        assert_eq!(v[0]["trigger"]["unless-domain"][0], "*news.site");
    }
}
