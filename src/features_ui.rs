// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Interface for the newer browser features: developer console, site
//! information popup, web notifications, reader mode and the keyboard
//! shortcuts page.

use std::time::Duration;

use egui::{Align, Align2, Color32, Frame, Id, Layout, Margin, RichText, Ui, pos2, vec2};

use crate::browser::Browser;
use crate::devtools::ConsoleAction;
use crate::icons::{Icon, icon};

/// Every shortcut, grouped, as shown on `ferrum://shortcuts`.
pub const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    ("Tabs and windows", &[
        ("Ctrl+T", "New tab"),
        ("Ctrl+W", "Close tab"),
        ("Ctrl+Shift+T", "Reopen closed tab"),
        ("Ctrl+Shift+N", "New private window"),
        ("Ctrl+Tab / Ctrl+Shift+Tab", "Next / previous tab"),
        ("Ctrl+1 … Ctrl+8, Ctrl+9", "Go to tab 1–8, last tab"),
        ("Ctrl+M", "Mute or unmute tab (Windows, Linux)"),
        ("Drag a tab", "Reorder tabs"),
    ]),
    ("Navigation", &[
        ("Ctrl+L, F6", "Address bar"),
        ("Alt+Left / Alt+Right", "Back / forward"),
        ("Ctrl+R, F5", "Reload"),
        ("Esc", "Close menus and popups, leave full screen"),
    ]),
    ("Page", &[
        ("Ctrl+F", "Find in page"),
        ("Ctrl+G, F3 (Shift for previous)", "Next match"),
        ("F9, Ctrl+Alt+R", "Reader mode"),
        ("Ctrl+P", "Print or save as PDF"),
        ("Ctrl+Shift+S", "Screenshot of the visible page"),
        ("Ctrl + / Ctrl − / Ctrl 0", "Zoom in / out / reset"),
        ("Ctrl+scroll, trackpad pinch", "Zoom"),
        ("F11", "Full screen"),
    ]),
    ("Bookmarks, history and downloads", &[
        ("Ctrl+D", "Bookmark this page"),
        ("Ctrl+Shift+B", "Show or hide the bookmarks bar"),
        ("Ctrl+Shift+O", "Bookmarks panel"),
        ("Ctrl+H", "History panel"),
        ("Ctrl+J", "Downloads panel"),
    ]),
    ("Developer", &[
        ("F12, Ctrl+Shift+I, Ctrl+Shift+J", "JavaScript console"),
        ("Ctrl+U", "View page source"),
    ]),
    ("Ferrum", &[
        ("Ctrl+,", "Settings"),
        ("Ctrl+/", "This page"),
    ]),
];

impl Browser {
    /// The console panel along the bottom of the window.
    pub(crate) fn draw_devtools(&mut self, ui: &mut Ui) {
        let mut action = ConsoleAction::None;
        egui::Panel::bottom("devtools")
            .resizable(true)
            .default_size(220.0)
            .size_range(100.0..=600.0)
            .frame(Frame::new().fill(ui.visuals().panel_fill).inner_margin(Margin::symmetric(10, 6)))
            .show_inside(ui, |ui| {
                let console = self.tabs[self.active].webview.as_ref().map(|_| &self.tabs[self.active].console);
                action = crate::devtools::draw_console(ui, &mut self.devtools, console);
            });
        match action {
            ConsoleAction::Run(code) => self.run_in_console(code),
            ConsoleAction::Clear => self.tabs[self.active].console.clear(),
            ConsoleAction::Close => self.devtools.open = false,
            ConsoleAction::None => {},
        }
    }

    /// The popup under the padlock: connection, trackers, permissions, site data.
    pub(crate) fn draw_site_info(&mut self, ctx: &egui::Context) {
        let anchor: egui::Rect = ctx.memory(|m| m.data.get_temp(Id::new("site_button"))).unwrap_or(egui::Rect::NOTHING);
        let Some(url) = self.active_tab().url() else {
            self.site_info_open = false;
            return;
        };
        let host = url.host_str().unwrap_or("").to_string();
        let secure = url.scheme() == "https";
        let blocked = self.active_webview().map_or(0, |w| self.blocker.blocked_on(w.id(), url.as_str()));
        let blocking_on = self.profile.settings.block_trackers && !self.profile.settings.blocker_exceptions.contains(&host);
        let permissions: Vec<(String, bool)> = self
            .profile
            .settings
            .site_permissions
            .get(&host)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
            .unwrap_or_default();
        let accent = self.accent();
        let mut toggle_blocking = false;
        let mut clear = false;
        let mut forget: Option<String> = None;
        let mut settings = false;

        let area = egui::Area::new(Id::new("site_info"))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor.left_bottom() + vec2(0.0, 6.0))
            .constrain(true)
            .show(ctx, |ui| {
                Frame::popup(ui.style()).inner_margin(Margin::same(14)).show(ui, |ui| {
                    ui.set_width(320.0);
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
                            ui.label(RichText::new("Tracker blocking").strong());
                            let text = if !self.profile.settings.block_trackers {
                                "Off for all sites (see Settings → Privacy)".to_string()
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
                });
            });

        if toggle_blocking {
            self.toggle_site_blocking();
        }
        if clear {
            self.clear_this_site();
            self.site_info_open = false;
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
        let outside = ctx.input(|i| {
            i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !anchor.contains(p))
        });
        if outside {
            self.site_info_open = false;
        }
    }

    /// "Your network blocks this search engine" with a one-click switch.
    pub(crate) fn draw_search_blocked(&mut self, ctx: &egui::Context) {
        let Some((id, engine, query)) = self.search_blocked.clone() else { return };
        if self.active_webview().map(|w| w.id()) != Some(id) {
            return;
        }
        let rect = self.content_rect;
        let mut action: Option<Option<crate::profile::SearchEngine>> = None;
        egui::Area::new(Id::new("search_blocked"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(pos2(rect.center().x, rect.top() + 12.0))
            .show(ctx, |ui| {
                Frame::popup(ui.style()).inner_margin(Margin::same(12)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        icon(ui, Icon::Warning, 18.0, Color32::from_rgb(220, 140, 30));
                        ui.label(RichText::new(format!("Your network blocks {}.", engine.name())).strong());
                        ui.label("Search with:");
                        for other in [crate::profile::SearchEngine::Google, crate::profile::SearchEngine::Bing, crate::profile::SearchEngine::Brave] {
                            if other != engine && ui.button(other.name()).clicked() {
                                action = Some(Some(other));
                            }
                        }
                        if crate::icons::icon_button(ui, Icon::Close, 20.0, true, "Dismiss").clicked() {
                            action = Some(None);
                        }
                    });
                });
            });
        match action {
            Some(Some(e)) => self.search_with(e, &query),
            Some(None) => self.search_blocked = None,
            None => {},
        }
    }

    /// A web notification, shown in the top-right corner of the page for a few seconds.
    pub(crate) fn draw_notification(&mut self, ctx: &egui::Context) {
        let Some((site, title, body, since)) = &self.notification else { return };
        if since.elapsed() > Duration::from_secs(8) {
            self.notification = None;
            return;
        }
        let rect = self.content_rect;
        let mut close = false;
        egui::Area::new(Id::new("web_notification"))
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(pos2(rect.right() - 16.0, rect.top() + 16.0))
            .show(ctx, |ui| {
                Frame::popup(ui.style()).inner_margin(Margin::same(12)).show(ui, |ui| {
                    ui.set_width(300.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(site.as_str()).weak().size(11.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if crate::icons::icon_button(ui, Icon::Close, 18.0, true, "Dismiss").clicked() {
                                close = true;
                            }
                        });
                    });
                    ui.label(RichText::new(title.as_str()).strong());
                    if !body.is_empty() {
                        ui.label(body.as_str());
                    }
                });
            });
        if close {
            self.notification = None;
        }
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    /// Reader mode in place of the page. Returns false if the tab isn't in reader mode.
    pub(crate) fn draw_reader(&mut self, ui: &mut Ui, rect: egui::Rect) -> bool {
        let Some(doc) = self.tabs[self.active].reader.clone() else { return false };
        // The page itself stays loaded underneath but isn't drawn.
        let s = &mut self.profile.settings;
        let out = crate::reader::draw(ui, rect, &doc, &mut s.reader_font_size, &mut s.reader_theme, &mut s.reader_width);
        if out.changed {
            self.profile.mark_dirty();
        }
        if out.close {
            self.toggle_reader();
        }
        true
    }

    pub(crate) fn draw_shortcuts_page(&mut self, ui: &mut Ui, rect: egui::Rect) {
        ui.painter().rect_filled(rect, 0.0, ui.visuals().panel_fill);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let width = 640.0_f32.min(ui.available_width() - 40.0);
                let side = ((ui.available_width() - width) / 2.0).max(20.0);
                ui.horizontal(|ui| {
                    ui.add_space(side);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        ui.add_space(28.0);
                        ui.label(RichText::new("Keyboard shortcuts").size(26.0).strong());
                        let mac = if cfg!(target_os = "macos") { "On a Mac, use Cmd (⌘) where it says Ctrl." } else { "" };
                        ui.label(RichText::new(mac).weak());
                        ui.add_space(10.0);
                        for (group, items) in SHORTCUTS {
                            Frame::new()
                                .fill(ui.visuals().extreme_bg_color)
                                .corner_radius(10.0)
                                .inner_margin(Margin::symmetric(16, 12))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(RichText::new(*group).strong().size(15.0));
                                    ui.add_space(4.0);
                                    egui::Grid::new(*group).num_columns(2).min_col_width(250.0).spacing(vec2(24.0, 6.0)).show(ui, |ui| {
                                        for (keys, what) in *items {
                                            ui.label(RichText::new(*keys).monospace().color(ui.visuals().strong_text_color()));
                                            ui.label(*what);
                                            ui.end_row();
                                        }
                                    });
                                });
                            ui.add_space(10.0);
                        }
                    });
                });
            });
        });
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect::<String>().replace('-', " "),
        None => String::new(),
    }
}
