// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! What the floating popups show (overlay.rs puts each in its own window):
//! the main menu, address-bar suggestions, site information, a tab's menu,
//! toasts, web notifications, "this network blocks your search engine" and
//! "allow notifications?".

use std::time::Duration;

use egui::{Align, Color32, Id, Key as EKey, Layout, RichText, Ui, pos2, vec2};
use url::Url;

use crate::browser::{Browser, InternalPage, LinkMenu, SidePanel};
use crate::icons::{Icon, icon, icon_button};
use crate::overlay::OverlayKind;
use crate::profile::SearchEngine;

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>()) }
}

impl Browser {
    /// Decide which popups are open this frame, and where (main-window points).
    pub(crate) fn request_popups(&mut self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        let content = self.content_rect;
        let fit = |r: egui::Rect| {
            // Popups are child windows on Windows and Linux, so keep them inside the window.
            let w = r.width().min(screen.width() - 8.0);
            let h = r.height().min(screen.height() - r.top() - 4.0).max(24.0);
            let x = r.left().clamp(screen.left() + 4.0, (screen.right() - w - 4.0).max(screen.left()));
            egui::Rect::from_min_size(pos2(x, r.top().max(screen.top())), vec2(w, h))
        };
        let memory_rect = |name: &str| ctx.memory(|m| m.data.get_temp::<egui::Rect>(Id::new(name))).unwrap_or(egui::Rect::NOTHING);
        let pressed_in_window = ctx.input(|i| i.pointer.any_pressed());
        let pressed_at = ctx.input(|i| i.pointer.interact_pos());
        let mut requests = Vec::new();

        // Main menu, under the ☰ button.
        let menu_button = memory_rect("menu_button");
        if self.menu_open && pressed_in_window && pressed_at.is_some_and(|p| !menu_button.contains(p)) {
            self.menu_open = false;
        }
        if self.menu_open {
            let r = egui::Rect::from_min_size(pos2(menu_button.right() - 270.0, menu_button.bottom() + 4.0), vec2(270.0, 2000.0));
            requests.push((OverlayKind::Menu, fit(r)));
        }

        // Site information, under the padlock.
        let site_button = memory_rect("site_button");
        if self.site_info_open && pressed_in_window && pressed_at.is_some_and(|p| !site_button.contains(p)) {
            self.site_info_open = false;
        }
        if self.site_info_open && self.active_tab().url().is_some() {
            let r = egui::Rect::from_min_size(site_button.left_bottom() + vec2(0.0, 6.0), vec2(350.0, 420.0));
            requests.push((OverlayKind::SiteInfo, fit(r)));
        }

        // Address-bar suggestions.
        let url_rect = memory_rect("url_rect");
        let url_focused = ctx.memory(|m| m.data.get_temp::<bool>(Id::new("url_focused"))).unwrap_or(false);
        if self.editing && self.profile.settings.show_suggestions && (url_focused || self.overlays.pointer_inside) {
            let n = self.profile.suggestions(&self.location, 7).len();
            if n > 0 {
                ctx.input(|i| {
                    if i.key_pressed(EKey::ArrowDown) {
                        self.suggestion_index = Some(self.suggestion_index.map_or(0, |s| (s + 1).min(n - 1)));
                    }
                    if i.key_pressed(EKey::ArrowUp) {
                        self.suggestion_index = self.suggestion_index.and_then(|s| s.checked_sub(1));
                    }
                });
                let r = egui::Rect::from_min_size(url_rect.left_bottom() + vec2(0.0, 4.0), vec2(url_rect.width(), n as f32 * 36.0 + 24.0));
                requests.push((OverlayKind::Suggestions, fit(r)));
            }
        }

        // A tab's right-click menu.
        if let Some((_, at)) = self.tab_menu {
            requests.push((OverlayKind::TabMenu, fit(egui::Rect::from_min_size(at, vec2(250.0, 280.0)))));
        }

        // A bookmark's or start-page tile's right-click menu.
        if let Some((_, at)) = &self.link_menu {
            requests.push((OverlayKind::LinkMenu, fit(egui::Rect::from_min_size(*at, vec2(230.0, 200.0)))));
        }

        // "Allow notifications?" for the active tab's site.
        let active_id = self.active_tab().id;
        if self.permission_asks.first().is_some_and(|(tab, _)| *tab == active_id) && content.is_positive() {
            requests.push((OverlayKind::Permission, fit(egui::Rect::from_min_size(content.left_top() + vec2(12.0, 10.0), vec2(470.0, 90.0)))));
        }

        // "Your network blocks this search engine".
        if self.search_blocked.as_ref().is_some_and(|(tab, ..)| *tab == active_id) {
            let r = egui::Rect::from_min_size(pos2(content.center().x - 290.0, content.top() + 12.0), vec2(580.0, 60.0));
            requests.push((OverlayKind::SearchBlocked, fit(r)));
        }

        // A web notification, top-right of the page.
        if let Some((.., since)) = &self.notification {
            if since.elapsed() > Duration::from_secs(8) {
                self.notification = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(500));
                let r = egui::Rect::from_min_size(pos2(content.right() - 340.0, content.top() + 12.0), vec2(328.0, 180.0));
                requests.push((OverlayKind::Notification, fit(r)));
            }
        }

        // Short notices ("Bookmarked", "Zoom 110%").
        if let Some((text, since)) = &self.toast {
            if since.elapsed() > Duration::from_millis(2200) {
                self.toast = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(250));
                let width = ctx.fonts_mut(|f| f.layout_no_wrap(text.clone(), egui::FontId::proportional(14.0), Color32::WHITE).size().x) + 40.0;
                let r = egui::Rect::from_min_size(pos2(content.center().x - width / 2.0, content.bottom() - 60.0), vec2(width, 38.0));
                requests.push((OverlayKind::Toast, fit(r)));
            }
        }

        self.overlay_requests = requests;
    }

    /// Draw one popup's contents into its window.
    pub(crate) fn draw_overlay(&mut self, kind: OverlayKind, ui: &mut Ui) {
        match kind {
            OverlayKind::Menu => self.menu_contents(ui),
            OverlayKind::Suggestions => self.suggestions_contents(ui),
            OverlayKind::SiteInfo => self.site_info_contents(ui),
            OverlayKind::TabMenu => {
                let Some((index, _)) = self.tab_menu else { return };
                let Some(tab) = self.tabs.get(index) else { return };
                let (pinned, muted) = (tab.pinned, tab.muted);
                if let Some(action) = crate::ui::tab_menu_items(ui, index, self.tabs.len(), pinned, muted) {
                    self.tab_menu = None;
                    self.tab_action(index, action);
                }
            },
            OverlayKind::LinkMenu => self.link_menu_contents(ui),
            OverlayKind::Toast => {
                if let Some((text, _)) = &self.toast {
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Label::new(RichText::new(text.as_str()).size(14.0)).wrap_mode(egui::TextWrapMode::Extend));
                    });
                }
            },
            OverlayKind::Notification => self.notification_contents(ui),
            OverlayKind::SearchBlocked => self.search_blocked_contents(ui),
            OverlayKind::Permission => self.permission_contents(ui),
        }
    }

    fn link_menu_contents(&mut self, ui: &mut Ui) {
        let Some((target, _)) = self.link_menu.clone() else { return };
        let url = match &target {
            LinkMenu::Bookmark(u) | LinkMenu::Frequent(u) => u.clone(),
        };
        let mut chosen = None;
        let mut item = |ui: &mut Ui, label: &str, id: u8| {
            if ui.add(egui::Button::new(label).frame(false).min_size(vec2(ui.available_width(), 24.0))).clicked() {
                chosen = Some(id);
            }
        };
        item(ui, "Open", 0);
        item(ui, "Open in new tab", 1);
        match &target {
            LinkMenu::Bookmark(_) => item(ui, "Edit…", 2),
            LinkMenu::Frequent(_) => item(ui, "Bookmark", 3),
        }
        item(ui, "Copy address", 4);
        ui.separator();
        match &target {
            LinkMenu::Bookmark(_) => item(ui, "Remove bookmark", 5),
            LinkMenu::Frequent(_) => item(ui, "Remove from history", 5),
        }
        let Some(id) = chosen else { return };
        self.link_menu = None;
        match id {
            0 => self.navigate_input(&url),
            1 => {
                if let Ok(u) = Url::parse(&url) {
                    self.open_tab(Some(u));
                }
            },
            2 => self.edit_bookmark(&url),
            3 => {
                let title = self.profile.history.iter().find(|h| h.url == url).map(|h| h.title.clone()).unwrap_or_default();
                self.profile.toggle_bookmark(&url, &title);
                self.toast("Bookmarked");
            },
            4 => {
                ui.ctx().copy_text(url);
                self.toast("Address copied");
            },
            _ => self.remove_link(&target),
        }
    }

    fn menu_contents(&mut self, ui: &mut Ui) {
        let zoom = self.active_tab().zoom;
        let mut close = false;
        let width = ui.available_width();
        let row = |ui: &mut Ui, label: &str, shortcut: &str| {
            ui.add(egui::Button::new(label).shortcut_text(shortcut).frame(false).min_size(vec2(width, 26.0))).clicked()
        };
        if row(ui, "New tab", "Ctrl+T") {
            self.new_tab();
            close = true;
        }
        if row(ui, "New private window", "Ctrl+Shift+N") {
            self.open_private_window();
            close = true;
        }
        ui.separator();
        if row(ui, "Bookmarks", "Ctrl+Shift+O") {
            self.toggle_panel(SidePanel::Bookmarks);
            close = true;
        }
        if row(ui, "History", "Ctrl+H") {
            self.toggle_panel(SidePanel::History);
            close = true;
        }
        if row(ui, "Downloads", "Ctrl+J") {
            self.toggle_panel(SidePanel::Downloads);
            close = true;
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Zoom");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(" + ").clicked() {
                    self.zoom_step(true);
                }
                if ui.button(format!("{}%", (zoom * 100.0).round())).clicked() {
                    self.set_zoom(1.0);
                }
                if ui.button(" − ").clicked() {
                    self.zoom_step(false);
                }
            });
        });
        if row(ui, "Find in page…", "Ctrl+F") {
            self.open_find();
            close = true;
        }
        if row(ui, if self.active_tab().reader.is_some() { "Exit reader mode" } else { "Reader mode" }, "F9") {
            self.toggle_reader();
            close = true;
        }
        if row(ui, "Open file…", "Ctrl+O") {
            self.open_file();
            close = true;
        }
        if row(ui, "Print…", "Ctrl+P") {
            self.print();
            close = true;
        }
        if row(ui, "Full screen", "F11") {
            self.toggle_window_fullscreen();
            close = true;
        }
        ui.separator();
        if row(ui, "Developer tools", "F12") {
            self.toggle_devtools();
            close = true;
        }
        if row(ui, "View page source", "Ctrl+U") {
            self.view_source();
            close = true;
        }
        let (muted, pinned) = (self.active_tab().muted, self.active_tab().pinned);
        if row(ui, if muted { "Unmute tab" } else { "Mute tab" }, "") {
            self.toggle_mute(self.active);
            close = true;
        }
        if row(ui, if pinned { "Unpin tab" } else { "Pin tab" }, "") {
            self.toggle_pin(self.active);
            close = true;
        }
        if row(ui, "Clear cookies and site data", "") {
            self.clear_site_data();
            close = true;
        }
        ui.separator();
        match self.update_status() {
            Some(crate::updates::UpdateStatus::Available { version, .. }) => {
                let label = RichText::new(format!("Update to Ferrum {version} and restart")).color(self.accent()).strong();
                if ui.add(egui::Button::new(label).frame(false).min_size(vec2(width, 26.0))).clicked() {
                    self.start_update();
                }
            },
            Some(crate::updates::UpdateStatus::Downloading { received, total, .. }) => {
                let fraction = total.filter(|t| *t > 0).map(|t| received as f32 / t as f32).unwrap_or(0.0);
                ui.add(egui::ProgressBar::new(fraction).desired_width(width).text("Downloading update…"));
            },
            Some(crate::updates::UpdateStatus::Installing { .. }) | Some(crate::updates::UpdateStatus::Installed { .. }) => {
                ui.label(RichText::new("Installing update…").color(self.accent()));
            },
            Some(crate::updates::UpdateStatus::Failed(e)) if e.starts_with("Ferrum can't") || e.contains("update") => {
                ui.label(RichText::new("Update failed — see Settings → About").weak()).on_hover_text(e);
            },
            _ => {},
        }
        if row(ui, "Settings", "Ctrl+,") {
            self.open_settings();
            close = true;
        }
        if row(ui, "Keyboard shortcuts", "Ctrl+/") {
            self.open_internal(InternalPage::Shortcuts);
            close = true;
        }
        if row(ui, "Quit", "") {
            self.request_quit();
            close = true;
        }
        if close {
            self.menu_open = false;
        }
    }

    fn suggestions_contents(&mut self, ui: &mut Ui) {
        let suggestions = self.profile.suggestions(&self.location, 7);
        let mut chosen = None;
        for (i, (title, url, bookmark)) in suggestions.iter().enumerate() {
            let selected = self.suggestion_index == Some(i);
            let resp = ui
                .horizontal(|ui| {
                    ui.set_min_height(26.0);
                    ui.set_width(ui.available_width());
                    if *bookmark {
                        icon(ui, Icon::StarFilled, 16.0, Color32::from_rgb(232, 160, 32));
                    } else {
                        icon(ui, Icon::History, 16.0, ui.visuals().weak_text_color());
                    }
                    if !title.is_empty() {
                        ui.label(truncate(title, 60));
                        ui.label(RichText::new("—").weak());
                    }
                    ui.label(RichText::new(truncate(url, 70)).color(ui.visuals().hyperlink_color).size(12.0));
                })
                .response;
            let resp = ui.interact(resp.rect, Id::new(("sugg", i)), egui::Sense::click());
            if selected || resp.hovered() {
                ui.painter().rect_filled(resp.rect.expand(2.0), ui.visuals().widgets.hovered.corner_radius, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.6));
            }
            if resp.clicked() {
                chosen = Some(url.clone());
            }
        }
        if let Some(url) = chosen {
            self.navigate_input(&url);
        }
    }

    fn site_info_contents(&mut self, ui: &mut Ui) {
        let Some(url) = self.active_tab().url() else { return };
        let host = url.host_str().unwrap_or("").to_string();
        let secure = url.scheme() == "https";
        let blocked = self.active_tab().blocked;
        let blocking_on = self.profile.settings.block_trackers && !self.profile.settings.blocker_exceptions.contains(&host);
        let permissions: Vec<(String, bool)> = self
            .profile
            .settings
            .site_permissions
            .get(&host)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
            .unwrap_or_default();
        let accent = self.accent();
        let (mut toggle_blocking, mut clear, mut settings) = (false, false, false);
        let mut forget: Option<String> = None;

        ui.label(RichText::new(if host.is_empty() { url.as_str() } else { &host }).size(16.0).strong());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if secure {
                icon(ui, Icon::Lock, 16.0, Color32::from_rgb(40, 150, 80));
                ui.label("Connection is secure");
            } else if url.scheme() == "http" {
                icon(ui, Icon::Warning, 16.0, Color32::from_rgb(200, 120, 30));
                ui.label("Connection is not secure");
            } else {
                icon(ui, Icon::Info, 16.0, ui.visuals().text_color());
                ui.label("Local or built-in page");
            }
        });
        if url.scheme() == "http" {
            ui.label(RichText::new("Don't enter passwords or card details on this site; others on the network could see them.").weak().size(12.0));
        }
        ui.separator();
        ui.horizontal(|ui| {
            icon(ui, Icon::Shield, 16.0, if blocking_on { accent } else { ui.visuals().weak_text_color() });
            ui.vertical(|ui| {
                ui.label(RichText::new("Ad and tracker blocking").strong());
                let text = if !self.profile.settings.block_trackers {
                    "Off for all sites (Settings → Privacy)".to_string()
                } else if !blocking_on {
                    "Off for this site".to_string()
                } else {
                    format!("{blocked} blocked on this page")
                };
                ui.label(RichText::new(text).weak().size(12.0));
            });
            if self.profile.settings.block_trackers {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(if blocking_on { "Turn off here" } else { "Turn on" }).clicked() {
                        toggle_blocking = true;
                    }
                });
            }
        });
        if !permissions.is_empty() {
            ui.separator();
            ui.label(RichText::new("Permissions").strong());
            for (feature, allowed) in &permissions {
                ui.horizontal(|ui| {
                    ui.label(capitalise(feature));
                    ui.label(RichText::new(if *allowed { "Allowed" } else { "Blocked" }).weak());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Reset").on_hover_text("Ask again next time").clicked() {
                            forget = Some(feature.clone());
                        }
                    });
                });
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Clear cookies and site data").clicked() {
                clear = true;
            }
            if ui.button("Settings").clicked() {
                settings = true;
            }
        });

        if toggle_blocking {
            self.site_info_open = false;
            self.toggle_site_blocking();
        }
        if clear {
            self.site_info_open = false;
            self.clear_this_site();
        }
        if let Some(feature) = forget {
            if let Some(map) = self.profile.settings.site_permissions.get_mut(&host) {
                map.remove(&feature);
                if map.is_empty() {
                    self.profile.settings.site_permissions.remove(&host);
                }
            }
            self.profile.mark_dirty();
        }
        if settings {
            self.site_info_open = false;
            self.settings_section = crate::settings_page::Section::Privacy;
            self.open_settings();
        }
    }

    fn notification_contents(&mut self, ui: &mut Ui) {
        let Some((site, title, body, _)) = self.notification.clone() else { return };
        let mut close = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(site.as_str()).weak().size(11.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Close, 18.0, true, "Dismiss").clicked() {
                    close = true;
                }
            });
        });
        ui.label(RichText::new(title.as_str()).strong());
        if !body.is_empty() {
            ui.label(body.as_str());
        }
        if close {
            self.notification = None;
        }
    }

    fn search_blocked_contents(&mut self, ui: &mut Ui) {
        let Some((_, engine, query)) = self.search_blocked.clone() else { return };
        let mut action: Option<Option<SearchEngine>> = None;
        ui.horizontal(|ui| {
            icon(ui, Icon::Warning, 18.0, Color32::from_rgb(220, 140, 30));
            ui.label(RichText::new(format!("Your network blocks {}.", engine.name())).strong());
            ui.label("Search with:");
            for other in [SearchEngine::Google, SearchEngine::Bing, SearchEngine::Brave] {
                if other != engine && ui.button(other.name()).clicked() {
                    action = Some(Some(other));
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Close, 20.0, true, "Dismiss").clicked() {
                    action = Some(None);
                }
            });
        });
        match action {
            Some(Some(e)) => self.search_with(e, &query),
            Some(None) => self.search_blocked = None,
            None => {},
        }
    }

    fn permission_contents(&mut self, ui: &mut Ui) {
        let Some((_, host)) = self.permission_asks.first().cloned() else { return };
        let accent = self.accent();
        let mut answer = None;
        ui.label(RichText::new(format!("{host} wants to show notifications")).strong());
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.remember_permission, "Remember");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new("Allow").color(Color32::WHITE)).fill(accent)).clicked() {
                    answer = Some(true);
                }
                if ui.button("Block").clicked() {
                    answer = Some(false);
                }
            });
        });
        if let Some(allowed) = answer {
            self.answer_permission_ask(allowed);
        }
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect::<String>().replace('-', " "),
        None => String::new(),
    }
}
