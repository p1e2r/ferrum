// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Reader mode and the keyboard shortcuts page.

use egui::{Frame, Margin, RichText, Ui, vec2};

use crate::browser::Browser;

/// Every shortcut, grouped, as shown on `ferrum://shortcuts`.
pub const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    ("Tabs and windows", &[
        ("Ctrl+T", "New tab"),
        ("Ctrl+W", "Close tab"),
        ("Ctrl+Shift+T", "Reopen closed tab (instant for 2 minutes)"),
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
        ("Ctrl+Shift+R, Shift+F5, Shift-click ⟳", "Reload without the cache"),
        ("Esc", "Close menus and popups, leave full screen"),
    ]),
    ("Page", &[
        ("Ctrl+F", "Find in page"),
        ("Ctrl+G, F3 (Shift for previous)", "Next match"),
        ("F9, Ctrl+Alt+R", "Reader mode"),
        ("Ctrl+O", "Open a file (pages, pictures, video, audio, PDF)"),
        ("Ctrl+P", "Print or save as PDF (system print dialog)"),
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
        ("F12, Ctrl+Shift+I, Ctrl+Shift+J", "Developer tools (inspector, console, network)"),
        ("Ctrl+U", "View page source"),
    ]),
    ("Ferrum", &[
        ("Ctrl+,", "Settings"),
        ("Ctrl+/", "This page"),
    ]),
];

impl Browser {
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
