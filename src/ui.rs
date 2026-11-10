// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Everything the user sees around the page, drawn natively with egui.

use std::sync::Arc;
use std::time::Duration;

use egui::text::{CCursor, CCursorRange};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Id, Key as EKey, Layout, Margin,
    RichText, Sense, Ui, Vec2, pos2, vec2,
};
use egui_glow::CallbackFn;
use euclid::{Point2D, Rect as ERect, Size2D};
use servo::{ContextMenuAction, ContextMenuItem, PermissionFeature, RgbColor, SelectElementOptionOrOptgroup, SimpleDialog};
use url::Url;

use crate::browser::{APP_NAME, Browser, PageControl, SidePanel};
use crate::downloads::{Status, human_size, open_path};
use crate::icons::{Icon, icon, icon_button};
use crate::profile::{DEFAULT_ACCENT, NewTabPage, SearchEngine, Settings, StartBackground, TabLayout, ThemeChoice};

pub fn accent(settings: &Settings) -> Color32 {
    let [r, g, b] = settings.accent;
    Color32::from_rgb(r, g, b)
}

/// A hash of every setting that affects the interface's look, so the style is
/// only rebuilt when one of them changes.
fn style_key(s: &Settings) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (s.theme as u8, s.accent, (s.ui_scale * 100.0) as i32, s.compact, s.roundness).hash(&mut h);
    h.finish().max(1)
}

/// Apply theme, accent colour, size, density and rounding to egui.
pub fn style(ctx: &egui::Context, settings: &Settings) {
    ctx.options_mut(|o| {
        o.zoom_with_keyboard = false;
        o.fallback_theme = egui::Theme::Light;
    });
    set_theme(ctx, settings.theme);
    ctx.set_zoom_factor(settings.ui_scale.clamp(0.7, 1.6));
    let accent = accent(settings);
    let r = CornerRadius::same(settings.roundness);
    let (pad, gap) = if settings.compact { (vec2(7.0, 3.0), 4.0) } else { (vec2(10.0, 5.0), 6.0) };
    ctx.all_styles_mut(|style| {
        style.spacing.button_padding = pad;
        style.spacing.item_spacing = vec2(gap, gap);
        style.spacing.interact_size.y = if settings.compact { 20.0 } else { 24.0 };
        style.visuals.selection.bg_fill = accent;
        style.visuals.selection.stroke.color = Color32::WHITE;
        style.visuals.hyperlink_color = accent;
        style.visuals.text_edit_bg_color = None;
        for w in [
            &mut style.visuals.widgets.noninteractive,
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.open,
        ] {
            w.corner_radius = r;
        }
        style.visuals.widgets.active.bg_stroke.color = accent;
        style.visuals.widgets.hovered.bg_stroke.color = accent.gamma_multiply(0.6);
        style.visuals.window_corner_radius = CornerRadius::same(settings.roundness.saturating_add(4));
        style.visuals.menu_corner_radius = CornerRadius::same(settings.roundness.saturating_add(2));
        style.interaction.tooltip_delay = 0.4;
    });
}

fn set_theme(ctx: &egui::Context, theme: ThemeChoice) {
    ctx.set_theme(match theme {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Light => egui::ThemePreference::Light,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
    });
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

fn host_of(url: &Option<Url>) -> String {
    url.as_ref().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_else(|| "This page".into())
}

/// Letter tile used where a site has no favicon: the first letter of `label`,
/// on a colour derived from `seed` (usually the URL) so each site keeps its colour.
fn letter_icon(ui: &mut Ui, label: &str, seed: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let label = label.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.");
    let letter = label.chars().find(|c| c.is_alphanumeric()).unwrap_or('•').to_uppercase().to_string();
    let hash = seed.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    let hue = (hash % 360) as f32 / 360.0;
    let fill: Color32 = egui::ecolor::Hsva::new(hue, 0.45, 0.75, 1.0).into();
    ui.painter().rect_filled(rect, size * 0.25, fill);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, letter, FontId::proportional(size * 0.6), Color32::WHITE);
}

#[derive(Clone, Copy)]
enum TabAction {
    Reload,
    Duplicate,
    Bookmark,
    Close,
    CloseOthers,
    CloseRight,
}

/// Right-click menu shared by both tab layouts.
fn tab_menu(resp: &egui::Response, index: usize, count: usize, out: &mut Option<(usize, TabAction)>) {
    resp.context_menu(|ui| {
        ui.set_min_width(180.0);
        let mut item = |ui: &mut Ui, label: &str, action: TabAction, enabled: bool| {
            if ui.add_enabled(enabled, egui::Button::new(label).frame(false)).clicked() {
                *out = Some((index, action));
                ui.close();
            }
        };
        item(ui, "Reload", TabAction::Reload, true);
        item(ui, "Duplicate", TabAction::Duplicate, true);
        item(ui, "Bookmark", TabAction::Bookmark, true);
        ui.separator();
        item(ui, "Close tab", TabAction::Close, true);
        item(ui, "Close other tabs", TabAction::CloseOthers, count > 1);
        item(ui, "Close tabs to the right", TabAction::CloseRight, index + 1 < count);
    });
}

enum ControlOutcome {
    Keep,
    Confirm,
    Dismiss,
    Select(Vec<usize>),
    Menu(ContextMenuAction),
    OpenInNewTab(Url),
    Download(Url),
    Copy(String),
    Bookmark,
}

impl Browser {
    pub(crate) fn accent(&self) -> Color32 {
        accent(&self.profile.settings)
    }

    pub(crate) fn draw(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let key = style_key(&self.profile.settings);
        if key != self.style_key {
            self.style_key = key;
            style(&ctx, &self.profile.settings);
            ctx.request_repaint();
        }
        self.ppp = ctx.pixels_per_point();
        self.apply_theme(&ctx);
        for i in 0..self.tabs.len() {
            self.refresh_favicon(&ctx, i);
        }

        let chrome = !self.page_fullscreen && !self.window_fullscreen;
        let vertical = self.profile.settings.tab_layout == TabLayout::Vertical;
        if chrome {
            if !vertical {
                self.draw_tab_strip(ui);
            }
            self.draw_toolbar(ui);
            if self.profile.settings.show_bookmarks_bar && !self.profile.bookmarks.is_empty() {
                self.draw_bookmarks_bar(ui);
            }
            if self.find.open {
                self.draw_find_bar(ui);
            }
            if vertical {
                self.draw_vertical_tabs(ui);
            }
            if self.panel != SidePanel::None {
                self.draw_side_panel(ui);
            }
        }

        if self.find.open {
            if let Some(webview) = self.active_webview().cloned() {
                self.find.search_if_changed(&webview, &self.window);
            }
        }

        self.content_rect = ui.available_rect_before_wrap();
        self.draw_content(ui);
        self.draw_page_controls(&ctx);
        if self.print_job.is_some() {
            self.draw_print_dialog(&ctx);
        }
        if self.menu_open {
            self.draw_menu(&ctx);
        }
        self.draw_toast(&ctx);
    }

    // ----- find in page -----------------------------------------------------

    fn draw_find_bar(&mut self, ui: &mut Ui) {
        enum Act {
            None,
            Step(i32),
            Close,
        }
        let mut act = Act::None;
        let (current, total) = *self.find.result.borrow();
        egui::Panel::top("find_bar")
            .frame(Frame::new().fill(ui.visuals().extreme_bg_color).inner_margin(Margin::symmetric(8, 4)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.find.query)
                            .id(Id::new("find_query"))
                            .hint_text("Find in page")
                            .desired_width(260.0),
                    );
                    if self.find.focus {
                        resp.request_focus();
                        self.find.focus = false;
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(EKey::Enter)) {
                        act = Act::Step(if ui.input(|i| i.modifiers.shift) { -1 } else { 1 });
                        resp.request_focus();
                    }
                    if resp.has_focus() && ui.input(|i| i.key_pressed(EKey::Escape)) {
                        act = Act::Close;
                    }
                    if icon_button(ui, Icon::Up, 24.0, total > 0, "Previous (Shift+Enter)").clicked() {
                        act = Act::Step(-1);
                    }
                    if icon_button(ui, Icon::Down, 24.0, total > 0, "Next (Enter)").clicked() {
                        act = Act::Step(1);
                    }
                    let status = if self.find.query.is_empty() {
                        String::new()
                    } else if total == 0 {
                        "No matches".into()
                    } else {
                        format!("{current} of {total}")
                    };
                    let color = if !self.find.query.is_empty() && total == 0 { Color32::from_rgb(210, 80, 70) } else { ui.visuals().weak_text_color() };
                    ui.label(RichText::new(status).color(color));
                    ui.checkbox(&mut self.find.match_case, "Match case");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, 22.0, true, "Close (Esc)").clicked() {
                            act = Act::Close;
                        }
                    });
                });
            });
        match act {
            Act::Step(d) => self.find_step(d),
            Act::Close => self.close_find(),
            Act::None => {},
        }
    }

    // ----- printing ---------------------------------------------------------

    fn draw_print_dialog(&mut self, ctx: &egui::Context) {
        let Some(job) = self.print_job.clone() else { return };
        let accent = self.accent();
        enum Act {
            None,
            Cancel,
            Print,
            Save,
            Open,
        }
        let mut act = Act::None;
        {
            let mut j = job.borrow_mut();
            let modal = egui::Modal::new(Id::new("print_dialog")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(RichText::new("Print").size(18.0).strong());
                ui.label(RichText::new(truncate(&j.title, 60)).weak());
                ui.add_space(8.0);
                match j.stage.clone() {
                    crate::print::Stage::Measuring | crate::print::Stage::Capturing => {
                        let (done, of) = j.progress();
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().color(accent));
                            ui.label(format!("Preparing pages… ({done} of {of} screens)"));
                        });
                        ui.add(egui::ProgressBar::new(done as f32 / of as f32).fill(accent));
                        ui.add_space(8.0);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Cancel").clicked() {
                                act = Act::Cancel;
                            }
                        });
                    },
                    crate::print::Stage::Failed(e) => {
                        ui.label(RichText::new(e).color(Color32::from_rgb(210, 80, 70)));
                        if ui.button("Close").clicked() {
                            act = Act::Cancel;
                        }
                    },
                    crate::print::Stage::Ready => {
                        ui.label(format!("{} page{} (A4)", j.pages, if j.pages == 1 { "" } else { "s" }));
                        ui.add_space(6.0);
                        if j.printers.is_empty() {
                            ui.label(RichText::new("No printers found. Save as PDF, or open the PDF and print from your PDF viewer.").weak());
                        } else {
                            egui::Grid::new("print_opts").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                                ui.label("Printer");
                                let selected = j.printers.get(j.printer).cloned().unwrap_or_default();
                                egui::ComboBox::from_id_salt("printer").selected_text(selected).width(240.0).show_ui(ui, |ui| {
                                    let names = j.printers.clone();
                                    for (i, name) in names.iter().enumerate() {
                                        ui.selectable_value(&mut j.printer, i, name);
                                    }
                                });
                                ui.end_row();
                                ui.label("Copies");
                                ui.add(egui::DragValue::new(&mut j.copies).range(1..=99));
                                ui.end_row();
                            });
                        }
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.button("Save as PDF…").clicked() {
                                act = Act::Save;
                            }
                            if ui.button("Open PDF").clicked() {
                                act = Act::Open;
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let can_print = !j.printers.is_empty();
                                let print = if can_print {
                                    egui::Button::new(RichText::new("Print").color(Color32::WHITE)).fill(accent)
                                } else {
                                    egui::Button::new("Print")
                                };
                                if ui.add_enabled(can_print, print).clicked() {
                                    act = Act::Print;
                                }
                                if ui.button("Cancel").clicked() {
                                    act = Act::Cancel;
                                }
                            });
                        });
                    },
                }
            });
            if modal.should_close() && matches!(act, Act::None) {
                act = Act::Cancel;
            }
        }

        let (pdf, printer, copies, title) = {
            let j = job.borrow();
            (j.pdf.clone(), j.printers.get(j.printer).cloned(), j.copies, j.title.clone())
        };
        match act {
            Act::None => return,
            Act::Cancel => {},
            Act::Print => {
                if let (Some(pdf), Some(printer)) = (&pdf, &printer) {
                    match crate::print::send_to_printer(pdf, printer, copies) {
                        Ok(()) => self.toast(format!("Sent to {printer}")),
                        Err(e) => self.toast(format!("Printing failed: {e}")),
                    }
                }
            },
            Act::Save => {
                if let Some(pdf) = &pdf {
                    let name: String = title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
                    let dialog = rfd::FileDialog::new().set_file_name(format!("{}.pdf", name.trim())).add_filter("PDF", &["pdf"]);
                    match dialog.save_file() {
                        Some(target) => match std::fs::copy(pdf, &target) {
                            Ok(_) => self.toast("Saved PDF"),
                            Err(e) => self.toast(format!("Couldn't save: {e}")),
                        },
                        None => return,
                    }
                }
            },
            Act::Open => {
                if let Some(pdf) = &pdf {
                    open_path(pdf);
                }
                return;
            },
        }
        self.print_job = None;
    }

    // ----- vertical tabs ----------------------------------------------------

    fn draw_vertical_tabs(&mut self, ui: &mut Ui) {
        let mut select = None;
        let mut close = None;
        let mut new_tab = false;
        let mut tab_action: Option<(usize, TabAction)> = None;
        let count = self.tabs.len();
        let accent = self.accent();
        let private = self.profile.is_private();
        egui::Panel::left("vertical_tabs")
            .resizable(true)
            .default_size(220.0)
            .size_range(140.0..=360.0)
            .frame(Frame::new().fill(ui.visuals().panel_fill).inner_margin(Margin::same(6)))
            .show_inside(ui, |ui| {
                if private {
                    ui.label(RichText::new(" Private window ").size(11.0).strong().color(Color32::WHITE).background_color(Color32::from_rgb(110, 60, 160)));
                }
                if ui.add(egui::Button::new("+  New tab").frame(false).min_size(vec2(ui.available_width(), 28.0))).clicked() {
                    new_tab = true;
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    for (i, tab) in self.tabs.iter().enumerate() {
                        let selected = i == self.active;
                        let mut close_rect: Option<egui::Rect> = None;
                        let frame = Frame::new()
                            .fill(if selected { ui.visuals().extreme_bg_color } else { Color32::TRANSPARENT })
                            .stroke(if selected { egui::Stroke::new(1.0_f32, accent.gamma_multiply(0.5)) } else { egui::Stroke::NONE })
                            .corner_radius(CornerRadius::same(self.profile.settings.roundness))
                            .inner_margin(Margin::symmetric(8, 5));
                        let resp = frame.show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                self.tab_icon(ui, tab);
                                let text = RichText::new(tab.title()).size(13.0);
                                ui.add(egui::Label::new(if selected { text.strong() } else { text }).truncate().selectable(false));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    close_rect = Some(icon_button(ui, Icon::Close, 18.0, true, "Close tab").rect);
                                });
                            });
                        });
                        let resp = ui.interact(resp.response.rect, Id::new(("vtab", i)), Sense::click()).on_hover_text(tab.title());
                        tab_menu(&resp, i, count, &mut tab_action);
                        // The whole-tab click area sits on top of the ✕, so tell them apart by position.
                        if resp.clicked() {
                            let on_close = close_rect.zip(resp.interact_pointer_pos()).is_some_and(|(r, p)| r.contains(p));
                            if on_close { close = Some(i) } else { select = Some(i) }
                        }
                        if resp.middle_clicked() {
                            close = Some(i);
                        }
                    }
                });
            });
        if let Some((i, action)) = tab_action {
            self.tab_action(i, action);
        } else if let Some(i) = close {
            self.close_tab(i);
        } else if let Some(i) = select {
            self.activate(i);
        }
        if new_tab {
            self.new_tab();
        }
    }

    fn tab_action(&mut self, i: usize, action: TabAction) {
        if i >= self.tabs.len() {
            return;
        }
        match action {
            TabAction::Reload => {
                self.activate(i);
                self.reload();
            },
            TabAction::Duplicate => {
                if let Some(url) = self.tabs[i].url() {
                    let webview = self.create_webview(url);
                    let zoom = self.tabs[i].zoom;
                    self.insert_tab_after(i, webview, zoom);
                }
            },
            TabAction::Bookmark => {
                if let Some(url) = self.tabs[i].url() {
                    let title = self.tabs[i].title();
                    if !self.profile.is_bookmarked(url.as_str()) {
                        self.profile.toggle_bookmark(url.as_str(), &title);
                    }
                    self.toast("Bookmarked");
                }
            },
            TabAction::Close => self.close_tab(i),
            TabAction::CloseOthers => {
                for j in (0..self.tabs.len()).rev() {
                    if j != i {
                        self.close_tab_quietly(j);
                    }
                }
                self.activate(0);
            },
            TabAction::CloseRight => {
                for j in (i + 1..self.tabs.len()).rev() {
                    self.close_tab_quietly(j);
                }
                self.activate(self.active.min(i));
            },
        }
    }

    /// Favicon, spinner, crash marker or letter tile for a tab.
    fn tab_icon(&self, ui: &mut Ui, tab: &crate::browser::Tab) {
        if tab.crashed.is_some() {
            icon(ui, Icon::Warning, 16.0, Color32::from_rgb(220, 60, 60));
        } else if tab.loading() {
            ui.add(egui::Spinner::new().size(14.0).color(self.accent()));
        } else if let Some(icon) = &tab.favicon {
            ui.add(egui::Image::new(icon).fit_to_exact_size(Vec2::splat(16.0)));
        } else {
            let url = tab.url().map(|u| u.to_string()).unwrap_or_default();
            letter_icon(ui, &tab.title(), &url, 16.0);
        }
    }

    // ----- tab strip --------------------------------------------------------

    fn draw_tab_strip(&mut self, ui: &mut Ui) {
        let mut select = None;
        let mut close = None;
        let mut new_tab = false;
        let mut tab_action: Option<(usize, TabAction)> = None;
        let count = self.tabs.len();
        let fill = ui.visuals().panel_fill;
        let tab_fill = ui.visuals().extreme_bg_color;
        let private = self.profile.is_private();

        egui::Panel::top("tabs")
            .frame(Frame::new().fill(fill).inner_margin(Margin { left: 6, right: 6, top: 6, bottom: 0 }))
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if private {
                        ui.label(RichText::new(" Private ").size(11.0).strong().color(Color32::WHITE).background_color(Color32::from_rgb(110, 60, 160)));
                    }
                    let available = ui.available_width() - 40.0;
                    let width = (available / self.tabs.len().max(1) as f32).clamp(60.0, 220.0);
                    egui::ScrollArea::horizontal()
                        .max_width(available)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for (i, tab) in self.tabs.iter().enumerate() {
                                    let selected = i == self.active;
                                    let mut close_rect: Option<egui::Rect> = None;
                                    let frame = Frame::new()
                                        .fill(if selected { tab_fill } else { Color32::TRANSPARENT })
                                        .corner_radius(CornerRadius { nw: self.profile.settings.roundness + 2, ne: self.profile.settings.roundness + 2, sw: 0, se: 0 })
                                        .inner_margin(Margin { left: 10, right: 4, top: 6, bottom: 6 });
                                    let resp = frame.show(ui, |ui| {
                                        ui.set_width(width - 14.0);
                                        ui.horizontal(|ui| {
                                            self.tab_icon(ui, tab);
                                            let title = tab.title();
                                            let text = RichText::new(title.clone()).size(13.0);
                                            let text = if selected { text.strong() } else { text };
                                            ui.add(egui::Label::new(text).truncate().selectable(false));
                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                if width > 90.0 || selected {
                                                    close_rect = Some(icon_button(ui, Icon::Close, 18.0, true, "Close tab (Ctrl+W)").rect);
                                                }
                                            });
                                        });
                                    });
                                    let resp = ui.interact(resp.response.rect, Id::new(("tab", i)), Sense::click());
                                    let resp = resp.on_hover_text(tab.title());
                                    tab_menu(&resp, i, count, &mut tab_action);
                                    // The whole-tab click area sits on top of the ✕, so tell them apart by position.
                                    if resp.clicked() {
                                        let on_close = close_rect.zip(resp.interact_pointer_pos()).is_some_and(|(r, p)| r.contains(p));
                                        if on_close { close = Some(i) } else { select = Some(i) }
                                    }
                                    if resp.middle_clicked() {
                                        close = Some(i);
                                    }
                                }
                            });
                        });
                    if icon_button(ui, Icon::Plus, 26.0, true, "New tab (Ctrl+T)").clicked() {
                        new_tab = true;
                    }
                });
            });

        if let Some((i, action)) = tab_action {
            self.tab_action(i, action);
        } else if let Some(i) = close {
            self.close_tab(i);
        } else if let Some(i) = select {
            self.activate(i);
        }
        if new_tab {
            self.new_tab();
        }
    }

    // ----- toolbar ----------------------------------------------------------

    fn draw_toolbar(&mut self, ui: &mut Ui) {
        let fill = ui.visuals().extreme_bg_color;
        let webview = self.active_webview().cloned();
        let can_back = webview.as_ref().is_some_and(|w| w.can_go_back());
        let can_forward = webview.as_ref().is_some_and(|w| w.can_go_forward());
        let loading = self.active_tab().loading();
        let url = self.active_tab().url();
        let bookmarked = url.as_ref().is_some_and(|u| self.profile.is_bookmarked(u.as_str()));
        let zoom = self.active_tab().zoom;
        let active_downloads = self.downloads.active_count();
        let accent = self.accent();
        let (show_home, show_star, show_dl) = (
            self.profile.settings.show_home_button,
            self.profile.settings.show_bookmark_button,
            self.profile.settings.show_downloads_button,
        );
        let button = if self.profile.settings.compact { 26.0 } else { 30.0 };

        let mut url_rect = egui::Rect::NOTHING;
        let mut url_focused = false;

        egui::Panel::top("toolbar")
            .frame(Frame::new().fill(fill).inner_margin(Margin::symmetric(8, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if icon_button(ui, Icon::Back, button, can_back, "Back (Alt+←)").clicked() {
                        self.go_back();
                    }
                    if icon_button(ui, Icon::Forward, button, can_forward, "Forward (Alt+→)").clicked() {
                        self.go_forward();
                    }
                    let (icon, tip) = if loading { (Icon::Stop, "Loading…") } else { (Icon::Reload, "Reload (Ctrl+R)") };
                    if icon_button(ui, icon, button, webview.is_some(), tip).clicked() && !loading {
                        self.reload();
                    }
                    if show_home && icon_button(ui, Icon::Home, button, true, "Home").clicked() {
                        self.go_home();
                    }

                    // Right-hand buttons are laid out first so the address bar can fill the rest.
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let menu = icon_button(ui, Icon::Menu, button, true, "Menu");
                        if menu.clicked() {
                            self.menu_open = !self.menu_open;
                        }
                        ui.memory_mut(|m| m.data.insert_temp(Id::new("menu_button"), menu.rect));
                        let dl = if show_dl || active_downloads > 0 {
                            icon_button(ui, Icon::Download, button, true, "Downloads (Ctrl+J)")
                        } else {
                            ui.allocate_response(Vec2::ZERO, Sense::hover())
                        };
                        if active_downloads > 0 {
                            let badge = egui::Rect::from_center_size(dl.rect.right_top() + vec2(-6.0, 6.0), Vec2::splat(14.0));
                            ui.painter().circle_filled(badge.center(), 7.0, accent);
                            ui.painter().text(badge.center(), Align2::CENTER_CENTER, active_downloads.to_string(), FontId::proportional(9.0), Color32::WHITE);
                        }
                        if dl.clicked() {
                            self.toggle_panel(SidePanel::Downloads);
                        }
                        if url.is_some() && show_star {
                            let star = if bookmarked { Icon::StarFilled } else { Icon::Star };
                            if icon_button(ui, star, button, true, "Bookmark this page (Ctrl+D)").clicked() {
                                self.toggle_bookmark();
                            }
                        }
                        if (zoom - 1.0).abs() > 0.001 && ui.small_button(format!("{}%", (zoom * 100.0).round())).on_hover_text("Reset zoom (Ctrl+0)").clicked() {
                            self.set_zoom(1.0);
                        }

                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let id = Id::new("location");
                            let output = egui::TextEdit::singleline(&mut self.location)
                                .id(id)
                                .hint_text(format!("Search with {} or enter address", self.profile.settings.search_engine.name()))
                                .font(FontId::proportional(15.0))
                                .margin(Margin::symmetric(10, 6))
                                .desired_width(f32::INFINITY)
                                .show(ui);
                            let response = output.response.response;
                            url_rect = response.rect;
                            url_focused = response.has_focus();

                            let select_all = self.focus_location || response.gained_focus();
                            if self.focus_location {
                                response.request_focus();
                                self.focus_location = false;
                            }
                            if select_all {
                                let mut state = output.state;
                                let len = self.location.chars().count();
                                state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(0), CCursor::new(len))));
                                state.store(ui.ctx(), id);
                            }
                            if response.changed() {
                                self.editing = true;
                                self.suggestion_index = None;
                            }
                            if response.lost_focus() {
                                if ui.input(|i| i.key_pressed(EKey::Enter)) {
                                    let suggestions = self.profile.suggestions(&self.location, 7);
                                    match self.suggestion_index.and_then(|i| suggestions.get(i)) {
                                        Some((_, url, _)) => {
                                            let url = url.clone();
                                            self.navigate_input(&url);
                                        },
                                        None => {
                                            let input = self.location.clone();
                                            self.navigate_input(&input);
                                        },
                                    }
                                } else if ui.input(|i| i.key_pressed(EKey::Escape)) {
                                    self.editing = false;
                                    self.sync_location();
                                }
                            }
                        });
                    });
                });
            });

        // Page-load progress line under the toolbar.
        if loading {
            let rect = ui.available_rect_before_wrap();
            let t = ui.input(|i| i.time) as f32;
            let w = rect.width();
            let x = rect.left() + ((t * 0.9).fract() * 1.4 - 0.4) * w;
            let bar = egui::Rect::from_min_max(pos2(x.max(rect.left()), rect.top()), pos2((x + w * 0.4).min(rect.right()), rect.top() + 2.5));
            ui.painter().rect_filled(bar, 1.0, accent);
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        }

        let pointer_on_list = ui.ctx().input(|i| {
            i.pointer.interact_pos().is_some_and(|p| self.suggestion_rect.contains(p))
                && (i.pointer.any_down() || i.pointer.any_released())
        });
        if self.editing && (url_focused || pointer_on_list) {
            self.draw_suggestions(ui.ctx(), url_rect);
        } else {
            self.suggestion_rect = egui::Rect::NOTHING;
        }
    }

    fn draw_suggestions(&mut self, ctx: &egui::Context, url_rect: egui::Rect) {
        let suggestions = self.profile.suggestions(&self.location, 7);
        if suggestions.is_empty() {
            return;
        }
        let n = suggestions.len();
        ctx.input(|i| {
            if i.key_pressed(EKey::ArrowDown) {
                self.suggestion_index = Some(self.suggestion_index.map_or(0, |s| (s + 1).min(n - 1)));
            }
            if i.key_pressed(EKey::ArrowUp) {
                self.suggestion_index = self.suggestion_index.and_then(|s| s.checked_sub(1));
            }
        });
        let mut chosen = None;
        let area = egui::Area::new(Id::new("suggestions"))
            .order(egui::Order::Foreground)
            .fixed_pos(url_rect.left_bottom() + vec2(0.0, 4.0))
            .show(ctx, |ui| {
                Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(url_rect.width() - 12.0);
                    for (i, (title, url, bookmark)) in suggestions.iter().enumerate() {
                        let selected = self.suggestion_index == Some(i);
                        let resp = ui
                            .horizontal(|ui| {
                                ui.set_min_height(26.0);
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
                        let resp = ui.interact(resp.rect, Id::new(("sugg", i)), Sense::click());
                        if selected || resp.hovered() {
                            ui.painter().rect_filled(resp.rect.expand(2.0), 4.0, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.6));
                        }
                        if resp.clicked() {
                            chosen = Some(url.clone());
                        }
                    }
                });
            });
        self.suggestion_rect = area.response.rect;
        if let Some(url) = chosen {
            self.navigate_input(&url);
        }
    }

    fn draw_bookmarks_bar(&mut self, ui: &mut Ui) {
        let mut open: Option<(String, bool)> = None;
        egui::Panel::top("bookmarks_bar")
            .frame(Frame::new().fill(ui.visuals().extreme_bg_color).inner_margin(Margin { left: 8, right: 8, top: 0, bottom: 4 }))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for b in &self.profile.bookmarks {
                        let resp = ui
                            .scope(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                let inner = ui.horizontal(|ui| {
                                    letter_icon(ui, &b.title, &b.url, 14.0);
                                    ui.add(egui::Label::new(RichText::new(truncate(&b.title, 22)).size(12.0)).selectable(false));
                                });
                                ui.interact(inner.response.rect.expand(3.0), Id::new(("bm", &b.url)), Sense::click())
                            })
                            .inner
                            .on_hover_text(&b.url);
                        if resp.hovered() {
                            ui.painter().rect_filled(resp.rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.5));
                        }
                        ui.add_space(6.0);
                        if resp.clicked() {
                            open = Some((b.url.clone(), false));
                        }
                        if resp.middle_clicked() {
                            open = Some((b.url.clone(), true));
                        }
                    }
                });
            });
        if let Some((url, new_tab)) = open {
            match Url::parse(&url) {
                Ok(u) if new_tab => self.open_background_tab(u),
                _ => self.navigate_input(&url),
            }
        }
    }

    // ----- page area --------------------------------------------------------

    fn draw_content(&mut self, ui: &mut Ui) {
        let rect = self.content_rect;
        let ppp = self.ppp;

        if let Some(reason) = self.active_tab().crashed.clone() {
            ui.painter().rect_filled(rect, 0.0, ui.visuals().window_fill);
            let mut reload = false;
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(40.0)), |ui| {
                ui.add_space(rect.height() * 0.25);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("This tab crashed").size(26.0).strong());
                    ui.label(RichText::new(truncate(&reason, 200)).weak());
                    ui.add_space(12.0);
                    reload = ui.button("Reload tab").clicked();
                });
            });
            if reload {
                self.reload();
            }
            return;
        }

        let Some(webview) = self.active_webview().cloned() else {
            self.draw_start_page(ui, rect);
            return;
        };

        // Keep the page sized to exactly the area it is shown in.
        let size = dpi::PhysicalSize::new((rect.width() * ppp).max(1.0) as u32, (rect.height() * ppp).max(1.0) as u32);
        if webview.size() != Size2D::new(size.width as f32, size.height as f32) {
            webview.resize(size);
        }
        // Render into the offscreen framebuffer, then have egui blit it into place.
        webview.paint();
        if let Some(render) = self.page_context.render_to_parent_callback() {
            let render = Arc::new(render);
            ui.ctx().layer_painter(egui::LayerId::background()).add(egui::PaintCallback {
                rect,
                callback: Arc::new(CallbackFn::new(move |info, painter| {
                    let clip = info.viewport_in_pixels();
                    let target = ERect::new(Point2D::new(clip.left_px, clip.from_bottom_px), Size2D::new(clip.width_px, clip.height_px));
                    render(painter.gl(), target);
                })),
            });
        }

        if let Some(text) = webview.status_text().filter(|s| !s.is_empty()) {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("status")));
            let galley = painter.layout_no_wrap(truncate(&text, 120), FontId::proportional(12.0), ui.visuals().text_color());
            let r = egui::Rect::from_min_size(pos2(rect.left(), rect.bottom() - galley.size().y - 8.0), galley.size() + vec2(12.0, 8.0));
            painter.rect_filled(r, CornerRadius { ne: 6, ..Default::default() }, ui.visuals().extreme_bg_color);
            painter.galley(r.min + vec2(6.0, 4.0), galley, Color32::PLACEHOLDER);
        }
    }

    /// Load (once) the start-page wallpaper chosen in settings.
    fn start_image_texture(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let path = self.profile.settings.start_image.clone();
        if path.is_empty() {
            return None;
        }
        if self.start_image.as_ref().map(|(p, _)| p) != Some(&path) {
            let texture = image::open(&path).ok().map(|img| {
                // Large photos are scaled down: the start page never needs more than ~2560px.
                let img = if img.width() > 2560 || img.height() > 1600 { img.thumbnail(2560, 1600) } else { img };
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                ctx.load_texture("start-wallpaper", color, egui::TextureOptions::LINEAR)
            });
            if texture.is_none() {
                log::warn!("Could not load start page image {path}");
            }
            self.start_image = Some((path, texture));
        }
        self.start_image.as_ref().and_then(|(_, t)| t.clone())
    }

    fn draw_start_page(&mut self, ui: &mut Ui, rect: egui::Rect) {
        let settings = self.profile.settings.clone();
        let accent = self.accent();
        let painter = ui.painter().clone();

        // Background, and whether text on it should be light.
        let mut light_text = false;
        match settings.start_background {
            StartBackground::Plain => {
                painter.rect_filled(rect, 0.0, ui.visuals().window_fill);
            },
            StartBackground::Color => {
                let [r, g, b] = settings.start_color;
                painter.rect_filled(rect, 0.0, Color32::from_rgb(r, g, b));
                light_text = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000 < 140;
            },
            StartBackground::Gradient => {
                let top = accent;
                let bottom = Color32::from_rgb(
                    (accent.r() as f32 * 0.25) as u8 + 10,
                    (accent.g() as f32 * 0.25) as u8 + 10,
                    (accent.b() as f32 * 0.25) as u8 + 30,
                );
                let mut mesh = egui::Mesh::default();
                mesh.colored_vertex(rect.left_top(), top);
                mesh.colored_vertex(rect.right_top(), top);
                mesh.colored_vertex(rect.left_bottom(), bottom);
                mesh.colored_vertex(rect.right_bottom(), bottom);
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(1, 2, 3);
                painter.add(egui::Shape::mesh(mesh));
                light_text = true;
            },
            StartBackground::Image => match self.start_image_texture(ui.ctx()) {
                Some(tex) => {
                    // Cover the area, cropping the image rather than stretching it.
                    let [w, h] = tex.size().map(|v| v as f32);
                    let scale = (rect.width() / w).max(rect.height() / h);
                    let (uw, uh) = (rect.width() / (w * scale), rect.height() / (h * scale));
                    let uv = egui::Rect::from_center_size(pos2(0.5, 0.5), vec2(uw, uh));
                    painter.image(tex.id(), rect, uv, Color32::WHITE);
                    painter.rect_filled(rect, 0.0, Color32::from_black_alpha(70));
                    light_text = true;
                },
                None => {
                    painter.rect_filled(rect, 0.0, ui.visuals().window_fill);
                },
            },
        }
        let text_color = if light_text { Color32::WHITE } else { ui.visuals().text_color() };
        let weak_color = if light_text { Color32::from_white_alpha(200) } else { ui.visuals().weak_text_color() };

        let width = (rect.width() * 0.7).clamp(280.0, 760.0);
        let top = rect.top() + rect.height() * if settings.start_show_clock { 0.10 } else { 0.16 };
        let area = egui::Rect::from_min_size(pos2(rect.center().x - width / 2.0, top), vec2(width, rect.bottom() - top));
        let mut go: Option<(String, bool)> = None;

        ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
            ui.vertical_centered(|ui| {
                if settings.start_show_clock {
                    let now = chrono::Local::now();
                    ui.label(RichText::new(now.format("%H:%M").to_string()).size(64.0).color(text_color));
                    ui.label(RichText::new(now.format("%A, %e %B").to_string()).size(15.0).color(weak_color));
                    ui.add_space(14.0);
                    // Repaint when the minute changes, not continuously.
                    let secs_left = 60 - now.timestamp() % 60;
                    ui.ctx().request_repaint_after(Duration::from_secs(secs_left as u64));
                }
                let title = if settings.start_title.trim().is_empty() { APP_NAME } else { settings.start_title.as_str() };
                let title_color = if light_text { Color32::WHITE } else { accent };
                ui.label(RichText::new(title).size(if settings.start_show_clock { 34.0 } else { 52.0 }).strong().color(title_color));
                if self.profile.is_private() {
                    ui.label(RichText::new("Private window — history, cookies and site data are forgotten when it closes").size(14.0).color(weak_color));
                }
                ui.add_space(22.0);
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.start_query)
                        .hint_text(format!("Search with {} or type an address", settings.search_engine.name()))
                        .font(FontId::proportional(17.0))
                        .margin(Margin::symmetric(14, 10))
                        .background_color(ui.visuals().extreme_bg_color)
                        .desired_width(width),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(EKey::Enter)) && !self.start_query.trim().is_empty() {
                    go = Some((std::mem::take(&mut self.start_query), false));
                }
                ui.add_space(26.0);

                // A centred grid of site tiles.
                let tiles = |ui: &mut Ui, heading: &str, items: Vec<(String, String)>, go: &mut Option<(String, bool)>| {
                    if items.is_empty() {
                        return;
                    }
                    ui.label(RichText::new(heading).strong().color(weak_color));
                    let tile = vec2(104.0, 92.0);
                    let per_row = ((ui.available_width() / tile.x) as usize).clamp(1, 8);
                    for row in items.chunks(per_row) {
                        let row_width = row.len() as f32 * tile.x;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.add_space((ui.available_width() - row_width).max(0.0) / 2.0);
                            for (title, url) in row {
                                let (r, resp) = ui.allocate_exact_size(tile, Sense::click());
                                if resp.hovered() {
                                    let fill = if light_text { Color32::from_white_alpha(40) } else { ui.visuals().widgets.hovered.weak_bg_fill };
                                    ui.painter().rect_filled(r.shrink(2.0), 10.0, fill);
                                }
                                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink(8.0)).layout(Layout::top_down(Align::Center)));
                                letter_icon(&mut child, title, url, 40.0);
                                child.add(egui::Label::new(RichText::new(truncate(title, 14)).size(12.0).color(text_color)).selectable(false));
                                let resp = resp.on_hover_text(url);
                                if resp.clicked() {
                                    *go = Some((url.clone(), false));
                                }
                                if resp.middle_clicked() {
                                    *go = Some((url.clone(), true));
                                }
                            }
                        });
                    }
                    ui.add_space(12.0);
                };

                if settings.start_show_bookmarks {
                    let items = self.profile.bookmarks.iter().take(16).map(|b| (b.title.clone(), b.url.clone())).collect();
                    tiles(ui, "Bookmarks", items, &mut go);
                }
                if settings.start_show_frequent {
                    let mut top: Vec<_> = self.profile.history.iter().filter(|h| !self.profile.is_bookmarked(&h.url)).collect();
                    top.sort_by(|a, b| b.visits.cmp(&a.visits).then(b.last_visit.cmp(&a.last_visit)));
                    let items = top
                        .iter()
                        .take(8)
                        .map(|h| (if h.title.is_empty() { h.url.clone() } else { h.title.clone() }, h.url.clone()))
                        .collect();
                    tiles(ui, "Frequently visited", items, &mut go);
                }
            });
        });

        // Quick link to the look settings.
        let btn = egui::Rect::from_min_size(rect.right_bottom() - vec2(120.0, 40.0), vec2(110.0, 30.0));
        let resp = ui.put(btn, egui::Button::new(RichText::new("Customize").size(12.0)));
        if resp.clicked() {
            self.panel = SidePanel::Settings;
        }

        if let Some((target, new_tab)) = go {
            match Url::parse(&target) {
                Ok(u) if new_tab => self.open_background_tab(u),
                _ => self.navigate_input(&target),
            }
        }
    }

    // ----- page-requested dialogs and popups ---------------------------------

    fn draw_page_controls(&mut self, ctx: &egui::Context) {
        let Some(active_id) = self.active_webview().map(|w| w.id()) else { return };
        let Some(index) = self.controls.iter().position(|c| c.webview == active_id) else { return };

        let rect = self.content_rect;
        let ppp = self.ppp;
        let accent = self.accent();
        let host = host_of(&self.active_tab().url());
        let mouse = self.mouse_position().unwrap_or(rect.center());
        let to_screen = |x: i32, y: i32| rect.min + vec2(x as f32 / ppp, y as f32 / ppp);
        let click_outside = |area: egui::Rect| {
            ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.contains(p)))
        };

        let mut outcome = ControlOutcome::Keep;
        let control = &mut self.controls[index].control;
        match control {
            PageControl::Dialog { dialog, input } => {
                let modal = egui::Modal::new(Id::new("page_dialog")).show(ctx, |ui| {
                    ui.set_width(420.0);
                    ui.label(RichText::new(format!("{host} says")).strong());
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| ui.label(dialog.message()));
                    if matches!(dialog, SimpleDialog::Prompt(_)) {
                        let r = ui.add(egui::TextEdit::singleline(input).desired_width(f32::INFINITY));
                        r.request_focus();
                    }
                    ui.add_space(10.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let enter = ui.input(|i| i.key_pressed(EKey::Enter));
                        if ui.button("  OK  ").clicked() || enter {
                            outcome = ControlOutcome::Confirm;
                        }
                        if !matches!(dialog, SimpleDialog::Alert(_)) && ui.button("Cancel").clicked() {
                            outcome = ControlOutcome::Dismiss;
                        }
                    });
                });
                if modal.should_close() && matches!(outcome, ControlOutcome::Keep) {
                    outcome = ControlOutcome::Dismiss;
                }
                if let (ControlOutcome::Confirm, SimpleDialog::Prompt(p)) = (&outcome, &mut *dialog) {
                    p.set_current_value(input);
                }
            },
            PageControl::Auth { request, user, pass } => {
                let modal = egui::Modal::new(Id::new("page_auth")).show(ctx, |ui| {
                    ui.set_width(380.0);
                    let who = if request.for_proxy() { "The proxy".to_string() } else { host_of(&Some(request.url().clone())) };
                    ui.label(RichText::new(format!("{who} is asking you to sign in")).strong());
                    ui.add_space(6.0);
                    egui::Grid::new("auth").num_columns(2).show(ui, |ui| {
                        ui.label("Username");
                        ui.add(egui::TextEdit::singleline(user).desired_width(240.0));
                        ui.end_row();
                        ui.label("Password");
                        ui.add(egui::TextEdit::singleline(pass).password(true).desired_width(240.0));
                        ui.end_row();
                    });
                    ui.add_space(8.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Sign in").clicked() || ui.input(|i| i.key_pressed(EKey::Enter)) {
                            outcome = ControlOutcome::Confirm;
                        }
                        if ui.button("Cancel").clicked() {
                            outcome = ControlOutcome::Dismiss;
                        }
                    });
                });
                if modal.should_close() && matches!(outcome, ControlOutcome::Keep) {
                    outcome = ControlOutcome::Dismiss;
                }
            },
            PageControl::Permission(request) => {
                let what = match request.feature() {
                    PermissionFeature::Geolocation => "know your location",
                    PermissionFeature::Notifications | PermissionFeature::Push => "show notifications",
                    PermissionFeature::Camera => "use your camera",
                    PermissionFeature::Microphone => "use your microphone",
                    PermissionFeature::Speaker => "choose audio outputs",
                    PermissionFeature::Midi => "access MIDI devices",
                    PermissionFeature::Bluetooth => "use Bluetooth",
                    PermissionFeature::PersistentStorage => "store data permanently",
                    PermissionFeature::ScreenWakeLock(_) => "keep your screen awake",
                    PermissionFeature::Gamepad => "use your gamepad",
                    _ => "use a device feature",
                };
                egui::Area::new(Id::new("permission"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(rect.left_top() + vec2(12.0, 10.0))
                    .show(ctx, |ui| {
                        Frame::popup(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("{host} wants to {what}")).strong());
                                ui.add_space(12.0);
                                if ui.button("Block").clicked() {
                                    outcome = ControlOutcome::Dismiss;
                                }
                                if ui.add(egui::Button::new(RichText::new("Allow").color(Color32::WHITE)).fill(accent)).clicked() {
                                    outcome = ControlOutcome::Confirm;
                                }
                            });
                        });
                    });
            },
            PageControl::Select(select) => {
                let pos = select.position();
                let anchor = to_screen(pos.min.x, pos.max.y);
                let min_width = ((pos.max.x - pos.min.x) as f32 / ppp).max(140.0);
                let multiple = select.allow_select_multiple();
                let mut chosen = select.selected_options();
                let area = egui::Area::new(Id::new("select"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(anchor)
                    .constrain(true)
                    .show(ctx, |ui| {
                        Frame::menu(ui.style()).show(ui, |ui| {
                            ui.set_min_width(min_width);
                            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                                let mut option = |ui: &mut Ui, label: &str, id: usize, disabled: bool, indent: bool| {
                                    let selected = chosen.contains(&id);
                                    let text = if indent { format!("    {label}") } else { label.to_string() };
                                    let resp = ui.add_enabled(!disabled, egui::Button::selectable(selected, text));
                                    if resp.clicked() {
                                        if multiple {
                                            if selected { chosen.retain(|c| *c != id) } else { chosen.push(id) }
                                        } else {
                                            outcome = ControlOutcome::Select(vec![id]);
                                        }
                                    }
                                };
                                for item in select.options() {
                                    match item {
                                        SelectElementOptionOrOptgroup::Option(o) => option(ui, &o.label, o.id, o.is_disabled, false),
                                        SelectElementOptionOrOptgroup::Optgroup { label, options } => {
                                            ui.label(RichText::new(label).strong().weak());
                                            for o in options {
                                                option(ui, &o.label, o.id, o.is_disabled, true);
                                            }
                                        },
                                    }
                                }
                            });
                            if multiple {
                                ui.separator();
                                if ui.button("Done").clicked() {
                                    outcome = ControlOutcome::Select(chosen.clone());
                                }
                            }
                        });
                    });
                if multiple && matches!(outcome, ControlOutcome::Keep) {
                    select.select(chosen);
                }
                if matches!(outcome, ControlOutcome::Keep) && click_outside(area.response.rect) {
                    outcome = if multiple { ControlOutcome::Confirm } else { ControlOutcome::Dismiss };
                }
            },
            PageControl::Color { picker, color } => {
                let pos = picker.position();
                let area = egui::Area::new(Id::new("color"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(to_screen(pos.min.x, pos.max.y))
                    .constrain(true)
                    .show(ctx, |ui| {
                        Frame::menu(ui.style()).show(ui, |ui| {
                            egui::widgets::color_picker::color_picker_color32(ui, color, egui::widgets::color_picker::Alpha::Opaque);
                            ui.horizontal(|ui| {
                                if ui.button("Cancel").clicked() {
                                    outcome = ControlOutcome::Dismiss;
                                }
                                if ui.button("Choose").clicked() {
                                    outcome = ControlOutcome::Confirm;
                                }
                            });
                        });
                    });
                if matches!(outcome, ControlOutcome::Keep) && click_outside(area.response.rect) {
                    outcome = ControlOutcome::Dismiss;
                }
            },
            PageControl::ContextMenu(menu) => {
                let info = menu.element_info().clone();
                let items: Vec<ContextMenuItem> = menu.items().to_vec();
                let area = egui::Area::new(Id::new("context_menu"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(mouse)
                    .constrain(true)
                    .show(ctx, |ui| {
                        Frame::menu(ui.style()).show(ui, |ui| {
                            ui.set_min_width(200.0);
                            let item = |ui: &mut Ui, label: &str, enabled: bool| {
                                ui.add_enabled(enabled, egui::Button::new(label).frame(false).min_size(vec2(190.0, 24.0))).clicked()
                            };
                            for entry in &items {
                                match entry {
                                    ContextMenuItem::Separator => {
                                        ui.separator();
                                    },
                                    ContextMenuItem::Item { label, action, enabled } => {
                                        let label = match action {
                                            ContextMenuAction::OpenLinkInNewWebView => "Open link in new tab",
                                            ContextMenuAction::OpenImageInNewView => "Open image in new tab",
                                            _ => label.as_str(),
                                        };
                                        if item(ui, label, *enabled) {
                                            outcome = match action {
                                                ContextMenuAction::OpenLinkInNewWebView => info.link_url.clone().map_or(ControlOutcome::Dismiss, ControlOutcome::OpenInNewTab),
                                                ContextMenuAction::OpenImageInNewView => info.image_url.clone().map_or(ControlOutcome::Dismiss, ControlOutcome::OpenInNewTab),
                                                ContextMenuAction::CopyLink => info.link_url.clone().map_or(ControlOutcome::Dismiss, |u| ControlOutcome::Copy(u.to_string())),
                                                ContextMenuAction::CopyImageLink => info.image_url.clone().map_or(ControlOutcome::Dismiss, |u| ControlOutcome::Copy(u.to_string())),
                                                other => ControlOutcome::Menu(*other),
                                            };
                                        }
                                    },
                                }
                            }
                            // Browser additions.
                            let has_extras = info.link_url.is_some() || info.image_url.is_some();
                            if has_extras || !items.is_empty() {
                                ui.separator();
                            }
                            if let Some(link) = &info.link_url {
                                if item(ui, "Save link as…", true) {
                                    outcome = ControlOutcome::Download(link.clone());
                                }
                            }
                            if let Some(image) = &info.image_url {
                                if item(ui, "Save image as…", true) {
                                    outcome = ControlOutcome::Download(image.clone());
                                }
                            }
                            if item(ui, "Bookmark this page", true) {
                                outcome = ControlOutcome::Bookmark;
                            }
                        });
                    });
                if matches!(outcome, ControlOutcome::Keep) && click_outside(area.response.rect) {
                    outcome = ControlOutcome::Dismiss;
                }
            },
        }

        if matches!(outcome, ControlOutcome::Keep) {
            return;
        }
        let pending = self.controls.remove(index);
        match (pending.control, outcome) {
            (PageControl::Dialog { dialog, .. }, ControlOutcome::Confirm) => match dialog {
                SimpleDialog::Alert(d) => d.confirm(),
                SimpleDialog::Confirm(d) => d.confirm(),
                SimpleDialog::Prompt(d) => d.confirm(),
            },
            (PageControl::Dialog { dialog, .. }, _) => match dialog {
                SimpleDialog::Alert(d) => d.confirm(),
                SimpleDialog::Confirm(d) => d.dismiss(),
                SimpleDialog::Prompt(d) => d.dismiss(),
            },
            (PageControl::Auth { request, user, pass }, ControlOutcome::Confirm) => request.authenticate(user, pass),
            (PageControl::Permission(request), ControlOutcome::Confirm) => request.allow(),
            (PageControl::Permission(request), _) => request.deny(),
            (PageControl::Select(mut select), ControlOutcome::Select(ids)) => {
                select.select(ids);
                select.submit();
            },
            (PageControl::Select(select), ControlOutcome::Confirm) => select.submit(),
            (PageControl::Color { mut picker, color }, ControlOutcome::Confirm) => {
                picker.select(Some(RgbColor { red: color.r(), green: color.g(), blue: color.b() }));
                picker.submit();
            },
            (PageControl::ContextMenu(menu), ControlOutcome::Menu(action)) => menu.select(action),
            (PageControl::ContextMenu(menu), other) => {
                menu.dismiss();
                match other {
                    ControlOutcome::OpenInNewTab(url) => self.open_background_tab(url),
                    ControlOutcome::Download(url) => {
                        let referer = self.active_tab().url();
                        self.start_download(url, referer);
                    },
                    ControlOutcome::Copy(text) => {
                        ctx.copy_text(text);
                        self.toast("Copied");
                    },
                    ControlOutcome::Bookmark => self.toggle_bookmark(),
                    _ => {},
                }
            },
            // Anything else (cancel, click outside): dropping the request cancels it.
            _ => {},
        }
        ctx.request_repaint();
    }

    // ----- menu -------------------------------------------------------------

    fn draw_menu(&mut self, ctx: &egui::Context) {
        let button: egui::Rect = ctx.memory(|m| m.data.get_temp(Id::new("menu_button"))).unwrap_or(egui::Rect::NOTHING);
        let zoom = self.active_tab().zoom;
        let mut close = false;
        let area = egui::Area::new(Id::new("main_menu"))
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(button.right_bottom() + vec2(0.0, 4.0))
            .show(ctx, |ui| {
                Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_width(250.0);
                    let row = |ui: &mut Ui, label: &str, shortcut: &str| {
                        let resp = ui.add(egui::Button::new(label).shortcut_text(shortcut).frame(false).min_size(vec2(240.0, 26.0)));
                        resp.clicked()
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
                    }
                    if row(ui, "History", "Ctrl+H") {
                        self.toggle_panel(SidePanel::History);
                    }
                    if row(ui, "Downloads", "Ctrl+J") {
                        self.toggle_panel(SidePanel::Downloads);
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
                    if row(ui, "Print…", "Ctrl+P") {
                        self.start_print();
                        close = true;
                    }
                    if row(ui, "Full screen", "F11") {
                        self.toggle_window_fullscreen();
                        close = true;
                    }
                    ui.separator();
                    if row(ui, "Settings", "Ctrl+,") {
                        self.toggle_panel(SidePanel::Settings);
                    }
                    if row(ui, "Quit", "") {
                        self.window.request_redraw();
                        ctx.memory_mut(|m| m.data.insert_temp(Id::new("quit"), true));
                        close = true;
                    }
                });
            });
        let clicked_outside = ctx.input(|i| {
            i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !button.contains(p))
        });
        if close || clicked_outside {
            self.menu_open = false;
        }
    }

    fn draw_toast(&mut self, ctx: &egui::Context) {
        let Some((text, since)) = &self.toast else { return };
        let elapsed = since.elapsed().as_secs_f32();
        if elapsed > 2.2 {
            self.toast = None;
            return;
        }
        let alpha = (2.2 - elapsed).clamp(0.0, 0.4) / 0.4;
        let rect = self.content_rect;
        egui::Area::new(Id::new("toast"))
            .order(egui::Order::Tooltip)
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(pos2(rect.center().x, rect.bottom() - 24.0))
            .interactable(false)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(Color32::from_black_alpha((220.0 * alpha) as u8))
                    .corner_radius(8.0)
                    .inner_margin(Margin::symmetric(14, 8))
                    .show(ui, |ui| ui.label(RichText::new(text.as_str()).color(Color32::WHITE.gamma_multiply(alpha))));
            });
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    // ----- side panels ------------------------------------------------------

    fn draw_side_panel(&mut self, ui: &mut Ui) {
        let panel = self.panel;
        egui::Panel::right("side_panel")
            .resizable(true)
            .default_size(360.0)
            .size_range(280.0..=600.0)
            .frame(Frame::new().fill(ui.visuals().panel_fill).inner_margin(Margin::same(12)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let title = match panel {
                        SidePanel::History => "History",
                        SidePanel::Bookmarks => "Bookmarks",
                        SidePanel::Downloads => "Downloads",
                        SidePanel::Settings => "Settings",
                        SidePanel::None => "",
                    };
                    ui.label(RichText::new(title).size(18.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, 24.0, true, "Close").clicked() {
                            self.panel = SidePanel::None;
                        }
                    });
                });
                ui.separator();
                match panel {
                    SidePanel::History => self.draw_history(ui),
                    SidePanel::Bookmarks => self.draw_bookmarks(ui),
                    SidePanel::Downloads => self.draw_downloads(ui),
                    SidePanel::Settings => self.draw_settings(ui),
                    SidePanel::None => {},
                }
            });
    }

    fn link_row(ui: &mut Ui, id: impl std::hash::Hash, title: &str, url: &str) -> (bool, bool, bool) {
        let mut remove = false;
        let resp = ui
            .horizontal(|ui| {
                letter_icon(ui, if title.is_empty() { url } else { title }, url, 20.0);
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(RichText::new(if title.is_empty() { url } else { title }).size(13.0)).truncate().selectable(false));
                    ui.add(egui::Label::new(RichText::new(url).size(11.0).weak()).truncate().selectable(false));
                });
            })
            .response;
        let resp = ui.interact(resp.rect, Id::new(("row", id)), Sense::click()).on_hover_text(url);
        if resp.hovered() {
            let r = resp.rect;
            let close = egui::Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), Vec2::splat(20.0));
            let c = ui.interact(close, Id::new(("row-x", r.min.x as i32, r.min.y as i32)), Sense::click());
            let color = if c.hovered() { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
            let d = vec2(4.0, 4.0);
            let k = close.center();
            ui.painter().line_segment([k - d, k + d], egui::Stroke::new(1.5_f32, color));
            ui.painter().line_segment([k + vec2(-d.x, d.y), k + vec2(d.x, -d.y)], egui::Stroke::new(1.5_f32, color));
            remove = c.clicked();
        }
        (resp.clicked() && !remove, resp.middle_clicked(), remove)
    }

    fn draw_history(&mut self, ui: &mut Ui) {
        if self.profile.is_private() {
            ui.label("History isn't saved in private windows.");
            return;
        }
        ui.add(egui::TextEdit::singleline(&mut self.history_filter).hint_text("Search history").desired_width(f32::INFINITY));
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} pages", self.profile.history.len())).weak());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Clear history").clicked() {
                    self.profile.clear_history();
                }
            });
        });
        ui.add_space(4.0);
        let needle = self.history_filter.to_lowercase();
        let entries: Vec<usize> = (0..self.profile.history.len())
            .rev()
            .filter(|&i| {
                let h = &self.profile.history[i];
                needle.is_empty() || h.url.to_lowercase().contains(&needle) || h.title.to_lowercase().contains(&needle)
            })
            .collect();
        let mut open = None;
        let mut remove = None;
        // Only the visible rows are laid out, so long histories stay fast.
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, 40.0, entries.len(), |ui, range| {
            for &i in &entries[range] {
                let h = &self.profile.history[i];
                let (click, middle, x) = Self::link_row(ui, ("h", i), &h.title, &h.url);
                if click {
                    open = Some((h.url.clone(), false));
                }
                if middle {
                    open = Some((h.url.clone(), true));
                }
                if x {
                    remove = Some(i);
                }
            }
        });
        if let Some(i) = remove {
            self.profile.history.remove(i);
            self.profile.mark_dirty();
        }
        self.open_from_panel(open);
    }

    fn draw_bookmarks(&mut self, ui: &mut Ui) {
        if let Some(url) = self.active_tab().url() {
            if !self.profile.is_bookmarked(url.as_str()) && ui.button("Bookmark this page").clicked() {
                self.toggle_bookmark();
            }
        }
        ui.checkbox(&mut self.profile.settings.show_bookmarks_bar, "Show bookmarks bar");
        ui.add_space(6.0);
        let mut open = None;
        let mut remove = None;
        let mut swap = None;
        let n = self.profile.bookmarks.len();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (i, b) in self.profile.bookmarks.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        if icon_button(ui, Icon::Up, 16.0, i > 0, "Move up").clicked() {
                            swap = Some((i, i - 1));
                        }
                        if icon_button(ui, Icon::Down, 16.0, i + 1 < n, "Move down").clicked() {
                            swap = Some((i, i + 1));
                        }
                    });
                    let (click, middle, x) = Self::link_row(ui, ("b", i), &b.title, &b.url);
                    if click {
                        open = Some((b.url.clone(), false));
                    }
                    if middle {
                        open = Some((b.url.clone(), true));
                    }
                    if x {
                        remove = Some(i);
                    }
                });
            }
            if n == 0 {
                ui.label(RichText::new("No bookmarks yet. Press Ctrl+D on any page.").weak());
            }
        });
        if let Some(i) = remove {
            self.profile.bookmarks.remove(i);
        }
        if let Some((a, b)) = swap {
            self.profile.bookmarks.swap(a, b);
        }
        if remove.is_some() || swap.is_some() {
            self.profile.mark_dirty();
        }
        self.open_from_panel(open);
    }

    fn open_from_panel(&mut self, open: Option<(String, bool)>) {
        if let Some((url, new_tab)) = open {
            match Url::parse(&url) {
                Ok(u) if new_tab => self.open_background_tab(u),
                Ok(u) => self.navigate(u),
                Err(_) => {},
            }
        }
    }

    fn draw_downloads(&mut self, ui: &mut Ui) {
        if self.downloads.items.is_empty() {
            ui.label(RichText::new("No downloads yet.").weak());
            ui.label(RichText::new("Links to files download automatically, or right-click a link → Save link as…").weak().size(12.0));
            return;
        }
        ui.horizontal(|ui| {
            if ui.button("Clear finished").clicked() {
                self.downloads.clear_finished();
            }
            if let Some(dir) = dirs::download_dir() {
                if ui.button("Open downloads folder").clicked() {
                    open_path(&dir);
                }
            }
        });
        ui.add_space(6.0);
        let mut running = false;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for d in self.downloads.items.iter().rev() {
                let p = d.snapshot();
                Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(RichText::new(&p.file_name).strong()).truncate());
                    match &p.status {
                        Status::Running => {
                            running = true;
                            let frac = p.total.filter(|t| *t > 0).map(|t| p.received as f32 / t as f32);
                            let text = match p.total {
                                Some(t) => format!("{} of {}", human_size(p.received), human_size(t)),
                                None => human_size(p.received),
                            };
                            match frac {
                                Some(f) => ui.add(egui::ProgressBar::new(f).text(text)),
                                None => ui.add(egui::ProgressBar::new(0.0).animate(true).text(text)),
                            };
                            if ui.small_button("Cancel").clicked() {
                                d.cancel();
                            }
                        },
                        Status::Done => {
                            ui.label(RichText::new(format!("Done · {}", human_size(p.received))).weak());
                            ui.horizontal(|ui| {
                                if let Some(path) = &p.path {
                                    if ui.small_button("Open").clicked() {
                                        open_path(path);
                                    }
                                    if let Some(parent) = path.parent() {
                                        if ui.small_button("Show in folder").clicked() {
                                            open_path(parent);
                                        }
                                    }
                                }
                            });
                        },
                        Status::Failed(e) => {
                            ui.label(RichText::new(format!("Failed: {}", truncate(e, 120))).color(Color32::from_rgb(210, 70, 70)));
                        },
                        Status::Cancelled => {
                            ui.label(RichText::new("Cancelled").weak());
                        },
                    }
                    ui.add(egui::Label::new(RichText::new(&d.url).size(11.0).weak()).truncate());
                });
            }
        });
        if running {
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
    }

    fn draw_settings(&mut self, ui: &mut Ui) {
        let before = format!("{:?}", self.profile.settings);
        let mut clear_history = false;
        let mut clear_data = false;
        let accent_now = self.accent();

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let s = &mut self.profile.settings;
            let section = |ui: &mut Ui, title: &str, open: bool, body: &mut dyn FnMut(&mut Ui)| {
                egui::CollapsingHeader::new(RichText::new(title).strong()).default_open(open).show(ui, |ui| {
                    body(ui);
                    ui.add_space(4.0);
                });
            };

            section(ui, "Look", true, &mut |ui| {
                ui.horizontal(|ui| {
                    ui.label("Theme");
                    ui.selectable_value(&mut s.theme, ThemeChoice::System, "System");
                    ui.selectable_value(&mut s.theme, ThemeChoice::Light, "Light");
                    ui.selectable_value(&mut s.theme, ThemeChoice::Dark, "Dark");
                });
                ui.checkbox(&mut s.dark_websites, "Dark websites too")
                    .on_hover_text("Ask websites for their dark style when the interface is dark. Some sites that don't support dark mode may become hard to read.");
                ui.label("Accent colour");
                ui.horizontal_wrapped(|ui| {
                    const PRESETS: [[u8; 3]; 9] = [
                        DEFAULT_ACCENT,
                        [214, 62, 62],
                        [226, 160, 30],
                        [64, 160, 90],
                        [32, 150, 160],
                        [52, 110, 220],
                        [110, 80, 210],
                        [196, 70, 160],
                        [110, 115, 125],
                    ];
                    for preset in PRESETS {
                        let color = Color32::from_rgb(preset[0], preset[1], preset[2]);
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                        ui.painter().circle_filled(r.center(), 10.0, color);
                        if s.accent == preset {
                            ui.painter().circle_stroke(r.center(), 11.5, egui::Stroke::new(2.0_f32, ui.visuals().strong_text_color()));
                        }
                        if resp.clicked() {
                            s.accent = preset;
                        }
                    }
                    let mut c = Color32::from_rgb(s.accent[0], s.accent[1], s.accent[2]);
                    if ui.color_edit_button_srgba(&mut c).on_hover_text("Custom colour").changed() {
                        s.accent = [c.r(), c.g(), c.b()];
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Interface size");
                    ui.add(egui::Slider::new(&mut s.ui_scale, 0.8..=1.5).step_by(0.05).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                });
                ui.horizontal(|ui| {
                    ui.label("Corner rounding");
                    ui.add(egui::Slider::new(&mut s.roundness, 0..=14));
                });
                ui.horizontal(|ui| {
                    ui.label("Density");
                    ui.selectable_value(&mut s.compact, false, "Comfortable");
                    ui.selectable_value(&mut s.compact, true, "Compact");
                });
            });

            section(ui, "Tabs & toolbar", true, &mut |ui| {
                ui.horizontal(|ui| {
                    ui.label("Tabs");
                    ui.selectable_value(&mut s.tab_layout, TabLayout::Top, "Along the top");
                    ui.selectable_value(&mut s.tab_layout, TabLayout::Vertical, "Down the side");
                });
                ui.checkbox(&mut s.show_bookmarks_bar, "Bookmarks bar");
                ui.checkbox(&mut s.show_home_button, "Home button");
                ui.checkbox(&mut s.show_bookmark_button, "Bookmark star");
                ui.checkbox(&mut s.show_downloads_button, "Downloads button (always shown while downloading)");
            });

            section(ui, "Start page", true, &mut |ui| {
                ui.horizontal(|ui| {
                    ui.label("Title");
                    ui.add(egui::TextEdit::singleline(&mut s.start_title).desired_width(180.0));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Background");
                    ui.selectable_value(&mut s.start_background, StartBackground::Plain, "Plain");
                    ui.selectable_value(&mut s.start_background, StartBackground::Gradient, "Accent gradient");
                    ui.selectable_value(&mut s.start_background, StartBackground::Color, "Colour");
                    ui.selectable_value(&mut s.start_background, StartBackground::Image, "Image");
                });
                match s.start_background {
                    StartBackground::Color => {
                        ui.horizontal(|ui| {
                            ui.label("Colour");
                            let mut c = Color32::from_rgb(s.start_color[0], s.start_color[1], s.start_color[2]);
                            if ui.color_edit_button_srgba(&mut c).changed() {
                                s.start_color = [c.r(), c.g(), c.b()];
                            }
                        });
                    },
                    StartBackground::Image => {
                        ui.horizontal(|ui| {
                            if ui.button("Choose image…").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
                                    .pick_file()
                                {
                                    s.start_image = path.to_string_lossy().into_owned();
                                }
                            }
                            if !s.start_image.is_empty() && ui.button("Remove").clicked() {
                                s.start_image.clear();
                            }
                        });
                        let name = std::path::Path::new(&s.start_image)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "No image chosen".into());
                        ui.label(RichText::new(name).weak().size(12.0));
                    },
                    _ => {},
                }
                ui.checkbox(&mut s.start_show_clock, "Clock and date");
                ui.checkbox(&mut s.start_show_bookmarks, "Bookmark tiles");
                ui.checkbox(&mut s.start_show_frequent, "Frequently visited tiles");
            });

            section(ui, "Search", false, &mut |ui| {
                egui::ComboBox::from_id_salt("engine").selected_text(s.search_engine.name()).show_ui(ui, |ui| {
                    for e in SearchEngine::ALL {
                        ui.selectable_value(&mut s.search_engine, e, e.name());
                    }
                });
                if s.search_engine == SearchEngine::Custom {
                    ui.label(RichText::new("Search URL — use %s where the search words go").size(12.0));
                    ui.add(egui::TextEdit::singleline(&mut s.custom_search_url).hint_text("https://example.com/search?q=%s").desired_width(f32::INFINITY));
                    if !s.custom_search_url.contains("%s") {
                        ui.label(RichText::new("Needs %s — using DuckDuckGo until then").size(11.0).color(Color32::from_rgb(210, 120, 40)));
                    }
                }
            });

            section(ui, "Start-up & new tabs", false, &mut |ui| {
                ui.checkbox(&mut s.restore_session, "Reopen tabs from last time");
                ui.horizontal_wrapped(|ui| {
                    ui.label("New tabs open");
                    ui.selectable_value(&mut s.new_tab_page, NewTabPage::StartPage, "Start page");
                    ui.selectable_value(&mut s.new_tab_page, NewTabPage::HomePage, "Home page");
                    ui.selectable_value(&mut s.new_tab_page, NewTabPage::Blank, "Blank");
                });
                ui.label("Home page (empty = start page)");
                ui.add(egui::TextEdit::singleline(&mut s.home_page).hint_text("https://…").desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    ui.label("Default page zoom");
                    ui.add(egui::Slider::new(&mut s.default_zoom, 0.5..=2.0).step_by(0.05).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                });
            });

            section(ui, "Downloads", false, &mut |ui| {
                ui.checkbox(&mut s.ask_where_to_save, "Ask where to save each file");
            });

            section(ui, "Privacy", false, &mut |ui| {
                ui.horizontal_wrapped(|ui| {
                    clear_history = ui.button("Clear history").clicked();
                    clear_data = ui.button("Clear cookies & site data").clicked();
                });
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Reset look to defaults").clicked() {
                    let d = crate::profile::Settings::default();
                    s.theme = d.theme;
                    s.accent = d.accent;
                    s.ui_scale = d.ui_scale;
                    s.compact = d.compact;
                    s.roundness = d.roundness;
                    s.tab_layout = d.tab_layout;
                    s.start_background = d.start_background;
                    s.start_title = d.start_title;
                }
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("{APP_NAME} {} · Servo engine · MIT licensed", env!("CARGO_PKG_VERSION")))
                    .color(accent_now)
                    .size(11.0),
            );
        });

        if clear_history {
            self.profile.clear_history();
            self.toast("History cleared");
        }
        if clear_data {
            self.clear_site_data();
        }
        if format!("{:?}", self.profile.settings) != before {
            self.profile.mark_dirty();
        }
    }

    pub(crate) fn mouse_position(&self) -> Option<egui::Pos2> {
        self.mouse_physical().map(|(x, y)| pos2(x / self.ppp, y / self.ppp))
    }

    pub(crate) fn wants_quit(&self) -> bool {
        self.egui_ctx().is_some_and(|ctx| ctx.memory(|m| m.data.get_temp::<bool>(Id::new("quit")).unwrap_or(false)))
    }
}
