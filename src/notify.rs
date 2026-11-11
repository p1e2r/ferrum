// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Web notifications in the system's notification centre, for when Ferrum
//! isn't the front window (inside Ferrum they're shown as a card).

pub fn system_notification(site: &str, title: &str, body: &str, window_focused: bool) {
    if window_focused {
        return;
    }
    let (site, title, body) = (site.to_string(), title.to_string(), body.to_string());
    // Sending can block briefly (D-Bus, Notification Center), so do it off the UI thread.
    let _ = std::thread::Builder::new().name("notification".into()).spawn(move || {
        let summary = if title.is_empty() { site.clone() } else { title };
        let text = if body.is_empty() { site } else { format!("{body}\n{site}") };
        if let Err(e) = notify_rust::Notification::new().appname("Ferrum").summary(&summary).body(&text).show() {
            log::info!("Could not show a system notification: {e}");
        }
    });
}
