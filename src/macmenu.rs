// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! The macOS menu bar: Ferrum, File, Edit, View, History, Bookmarks,
//! Profiles, Window and Help, like other Mac browsers.
//!
//! The Edit menu's Cut, Copy, Paste, Undo and Select All are the system's
//! own items, which is also what makes those keys work inside web pages.
//! Other items carry an id handled by `Browser::menu_command`. Their keys
//! (⌘T, ⌘W…) are shown in the menus; a key handled by a page view and by
//! the menu at the same moment runs once (see `Browser::shortcut_debounce`).
//!
//! Menu clicks arrive on AppKit's thread of events; `install` queues them
//! and wakes Ferrum's event loop, and `Browser::pump` runs them.

use std::sync::Mutex;

use muda::accelerator::Accelerator;
use muda::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

static QUEUE: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Menu commands clicked since the last call.
pub fn take() -> Vec<String> {
    QUEUE.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

fn item(id: &str, text: &str, keys: Option<&str>) -> MenuItem {
    let accel: Option<Accelerator> = keys.and_then(|k| k.parse().ok());
    MenuItem::with_id(id, text, true, accel)
}

/// Build the menu bar and set it as the app's. `wake` is called after each click.
/// `profiles`: (id, name) of the other profiles, for the Profiles menu.
pub fn install(wake: impl Fn() + Send + Sync + 'static, profiles: &[(String, String)]) -> Option<Menu> {
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if let Ok(mut q) = QUEUE.lock() {
            q.push(e.id.0);
        }
        wake();
    }));

    let menu = Menu::new();
    let sep = PredefinedMenuItem::separator;

    let about = AboutMetadata {
        name: Some("Ferrum".into()),
        version: Some(crate::VERSION.into()),
        comments: Some("A fast desktop browser with a native interface.".into()),
        license: Some("MIT".into()),
        ..Default::default()
    };
    let app = Submenu::with_items(
        "Ferrum",
        true,
        &[
            &PredefinedMenuItem::about(Some("About Ferrum"), Some(about)),
            &item("check-updates", "Check for Updates…", None),
            &sep(),
            &item("settings", "Settings…", Some("Cmd+Comma")),
            &sep(),
            &PredefinedMenuItem::services(None),
            &sep(),
            &PredefinedMenuItem::hide(Some("Hide Ferrum")),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &sep(),
            &PredefinedMenuItem::quit(Some("Quit Ferrum")),
        ],
    )
    .ok()?;

    let file = Submenu::with_items(
        "File",
        true,
        &[
            &item("new-tab", "New Tab", Some("Cmd+T")),
            &item("new-private", "New Private Window", Some("Cmd+Shift+N")),
            &item("reopen-tab", "Reopen Closed Tab", Some("Cmd+Shift+T")),
            &sep(),
            &item("open-file", "Open File…", Some("Cmd+O")),
            &item("open-location", "Open Location…", Some("Cmd+L")),
            &sep(),
            &item("close-tab", "Close Tab", Some("Cmd+W")),
            &sep(),
            &item("print", "Print…", Some("Cmd+P")),
        ],
    )
    .ok()?;

    let edit = Submenu::with_items(
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &sep(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::paste_and_match_style(None),
            &PredefinedMenuItem::select_all(None),
            &sep(),
            &item("find", "Find…", Some("Cmd+F")),
            &item("find-next", "Find Next", Some("Cmd+G")),
            &item("find-previous", "Find Previous", Some("Cmd+Shift+G")),
        ],
    )
    .ok()?;

    let view = Submenu::with_items(
        "View",
        true,
        &[
            &item("reload", "Reload Page", Some("Cmd+R")),
            &item("hard-reload", "Reload Without Cache", Some("Cmd+Shift+R")),
            &item("stop", "Stop", Some("Cmd+Period")),
            &sep(),
            &item("zoom-in", "Zoom In", Some("Cmd+Equal")),
            &item("zoom-out", "Zoom Out", Some("Cmd+Minus")),
            &item("zoom-reset", "Actual Size", Some("Cmd+Digit0")),
            &sep(),
            &item("reader", "Reader Mode", Some("Alt+Cmd+R")),
            &item("bookmarks-bar", "Show Bookmarks Bar", Some("Cmd+Shift+B")),
            &item("fullscreen", "Enter Full Screen", Some("Ctrl+Cmd+F")),
            &sep(),
            &item("devtools", "Developer Tools", Some("Alt+Cmd+I")),
            &item("view-source", "View Page Source", Some("Alt+Cmd+U")),
        ],
    )
    .ok()?;

    let history = Submenu::with_items(
        "History",
        true,
        &[
            &item("back", "Back", Some("Cmd+BracketLeft")),
            &item("forward", "Forward", Some("Cmd+BracketRight")),
            &item("home", "Home", Some("Cmd+Shift+H")),
            &sep(),
            &item("history", "Show All History", Some("Cmd+Y")),
            &item("clear-data", "Clear Cookies and Site Data…", None),
        ],
    )
    .ok()?;

    let bookmarks = Submenu::with_items(
        "Bookmarks",
        true,
        &[
            &item("bookmark", "Bookmark This Page", Some("Cmd+D")),
            &item("bookmarks", "Show All Bookmarks", Some("Alt+Cmd+B")),
            &sep(),
            &item("import", "Import from Another Browser…", None),
        ],
    )
    .ok()?;

    let profiles_menu = Submenu::new("Profiles", true);
    for (id, name) in profiles {
        let _ = profiles_menu.append(&item(&format!("profile:{id}"), &format!("Open {name}"), None));
    }
    if !profiles.is_empty() {
        let _ = profiles_menu.append(&sep());
    }
    let _ = profiles_menu.append_items(&[&item("add-profile", "Add Profile", None), &item("manage-profiles", "Manage Profiles…", None)]);

    let window = Submenu::with_items(
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(Some("Zoom")),
            &sep(),
            &item("next-tab", "Show Next Tab", Some("Ctrl+Tab")),
            &item("previous-tab", "Show Previous Tab", Some("Ctrl+Shift+Tab")),
            &sep(),
            &item("downloads", "Downloads", Some("Alt+Cmd+L")),
            &sep(),
            &PredefinedMenuItem::bring_all_to_front(None),
        ],
    )
    .ok()?;

    let help = Submenu::with_items(
        "Help",
        true,
        &[&item("shortcuts", "Keyboard Shortcuts", Some("Cmd+Slash")), &item("github", "Ferrum on GitHub", None)],
    )
    .ok()?;

    menu.append_items(&[&app, &file, &edit, &view, &history, &bookmarks, &profiles_menu, &window, &help]).ok()?;
    menu.init_for_nsapp();
    window.set_as_windows_menu_for_nsapp();
    help.set_as_help_menu_for_nsapp();
    Some(menu)
}
