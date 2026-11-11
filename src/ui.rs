// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Everything the user sees around the page, drawn natively with egui.

use std::time::Duration;

use egui::text::{CCursor, CCursorRange};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Id, Key as EKey, Layout, Margin,
    RichText, Sense, Stroke, Ui, Vec2, pos2, vec2,
};
use url::Url;

use crate::browser::{APP_NAME, Browser, LinkMenu, SidePanel};
use crate::downloads::{Status, human_size, open_path};
use crate::icons::{Icon, icon, icon_button};
use crate::profile::{Settings, StartBackground, TabLayout, ThemeChoice};

pub fn accent(settings: &Settings) -> Color32 {
    let [r, g, b] = settings.accent;
    Color32::from_rgb(r, g, b)
}

/// A hash of every setting that affects the interface's look, so the style is
/// only rebuilt when one of them changes.
pub fn style_key(s: &Settings) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (s.theme_preset, s.theme as u8, s.accent, (s.ui_scale * 100.0) as i32, s.compact, s.roundness).hash(&mut h);
    h.finish().max(1)
}

/// Apply theme, accent colour, size, density and rounding to egui.
pub fn style(ctx: &egui::Context, settings: &Settings) {
    ctx.options_mut(|o| {
        o.zoom_with_keyboard = false;
        o.fallback_theme = egui::Theme::Light;
    });
    // Start both light and dark styles from the defaults, then apply the theme.
    ctx.set_style_of(egui::Theme::Light, egui::Style { visuals: egui::Visuals::light(), ..Default::default() });
    ctx.set_style_of(egui::Theme::Dark, egui::Style { visuals: egui::Visuals::dark(), ..Default::default() });
    match settings.theme_preset.palette() {
        Some(palette) => {
            let mode = if palette.dark { egui::Theme::Dark } else { egui::Theme::Light };
            let high_contrast = settings.theme_preset == crate::theme::ThemePreset::HighContrast;
            ctx.style_mut_of(mode, |style| crate::theme::apply(&mut style.visuals, &palette, high_contrast));
            ctx.set_theme(mode);
        },
        None => set_theme(ctx, settings.theme),
    }
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
pub(crate) enum TabAction {
    Reload,
    Duplicate,
    Bookmark,
    Pin,
    Mute,
    Close,
    CloseOthers,
    CloseRight,
}

/// Right-clicking a tab opens its menu (drawn in a popup window, see overlay_ui.rs).
fn tab_menu(resp: &egui::Response, index: usize, open: &mut Option<(usize, egui::Pos2)>) {
    if resp.secondary_clicked() {
        *open = Some((index, resp.interact_pointer_pos().unwrap_or(resp.rect.left_bottom())));
    }
}

/// The items of a tab's right-click menu. Returns the chosen action.
pub(crate) fn tab_menu_items(ui: &mut Ui, index: usize, count: usize, pinned: bool, muted: bool) -> Option<TabAction> {
    let mut out = None;
    let mut item = |ui: &mut Ui, label: &str, action: TabAction, enabled: bool| {
        if ui.add_enabled(enabled, egui::Button::new(label).frame(false).min_size(vec2(ui.available_width(), 24.0))).clicked() {
            out = Some(action);
        }
    };
    item(ui, "Reload", TabAction::Reload, true);
    item(ui, "Duplicate", TabAction::Duplicate, true);
    item(ui, "Bookmark", TabAction::Bookmark, true);
    item(ui, if pinned { "Unpin tab" } else { "Pin tab" }, TabAction::Pin, true);
    item(ui, if muted { "Unmute tab" } else { "Mute tab" }, TabAction::Mute, true);
    ui.separator();
    item(ui, "Close tab", TabAction::Close, true);
    item(ui, "Close other tabs (keeps pinned)", TabAction::CloseOthers, count > 1);
    item(ui, "Close tabs to the right", TabAction::CloseRight, index + 1 < count);
    out
}

/// Where a dragged tab would land: the index of the first tab (other than the
/// dragged one) that `before` says the pointer is in front of.
fn drop_target(rects: &[egui::Rect], from: usize, before: impl Fn(&egui::Rect) -> bool) -> usize {
    let mut target = rects.len() - 1;
    for (i, r) in rects.iter().enumerate() {
        if before(r) {
            target = if i > from { i - 1 } else { i };
            break;
        }
    }
    target
}

/// Pinned tabs stay among the pinned ones, the others after them.
fn clamp_to_group(tabs: &[crate::browser::Tab], from: usize, target: usize) -> usize {
    let pinned = tabs.iter().filter(|t| t.pinned).count();
    if tabs[from].pinned { target.min(pinned.saturating_sub(1)) } else { target.max(pinned) }
}

/// A tab's close button.
const CLOSE_SIZE: f32 = 22.0;

/// A tab title that fades out, instead of ending in "…", where it runs into
/// the close button. `bg` is the colour behind it.
fn faded_title(ui: &mut Ui, title: &str, strong: bool, width: f32, bg: Color32) {
    let width = width.max(0.0);
    let color = if strong { ui.visuals().strong_text_color() } else { ui.visuals().text_color() };
    let galley = ui.painter().layout_no_wrap(title.to_string(), FontId::proportional(13.0), color);
    let height = galley.size().y.max(18.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let pos = pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
    let overflow = galley.size().x > rect.width();
    ui.painter().with_clip_rect(rect.intersect(ui.clip_rect())).galley(pos, galley, color);
    if overflow {
        let fade = 24.0_f32.min(rect.width());
        let r = egui::Rect::from_min_max(pos2(rect.right() - fade, rect.top()), rect.max);
        let clear = Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), 0);
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(r.left_top(), clear);
        mesh.colored_vertex(r.right_top(), bg);
        mesh.colored_vertex(r.left_bottom(), clear);
        mesh.colored_vertex(r.right_bottom(), bg);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(1, 2, 3);
        ui.painter().with_clip_rect(ui.clip_rect()).add(egui::Shape::mesh(mesh));
    }
}

/// Width of the minimise / maximise / close group, as in Chrome on Windows.
const WINDOW_BUTTONS_WIDTH: f32 = 46.0 * 3.0;

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
        self.apply_page_theme(ctx.theme() == egui::Theme::Dark);

        let chrome = !self.window_fullscreen;
        let vertical = self.profile.settings.tab_layout == TabLayout::Vertical;
        if chrome {
            if !vertical {
                self.draw_tab_strip(ui);
            } else if self.custom_title_bar() {
                self.draw_title_bar(ui);
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
            if let Some(webview) = self.tabs.get(self.active).and_then(|t| t.webview.as_ref()) {
                self.find.search_if_changed(webview, &self.proxy);
            }
        }

        self.content_rect = ui.available_rect_before_wrap();
        self.draw_content(ui);
        self.request_popups(&ctx);
    }

    // ----- find in page -----------------------------------------------------

    fn draw_find_bar(&mut self, ui: &mut Ui) {
        enum Act {
            None,
            Step(i32),
            Close,
        }
        let mut act = Act::None;
        let (current, total) = self.find.result.lock().map(|r| *r).unwrap_or((0, 0));
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

    fn draw_vertical_tabs(&mut self, ui: &mut Ui) {
        let mut select = None;
        let mut close = None;
        let mut new_tab = false;
        let mut tab_action: Option<(usize, TabAction)> = None;
        let mut menu_request: Option<(usize, egui::Pos2)> = None;
        let accent = self.accent();
        let private = self.profile.is_private();
        let (mut dragging, mut dropped, mut moved): (Option<usize>, Option<usize>, Option<(usize, usize)>) = (None, None, None);
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
                    let mut rects: Vec<egui::Rect> = Vec::new();
                    for (i, tab) in self.tabs.iter().enumerate() {
                        let selected = i == self.active;
                        let mut close_rect: Option<egui::Rect> = None;
                        let mut speaker_rect: Option<egui::Rect> = None;
                        let frame = Frame::new()
                            .fill(if selected { ui.visuals().extreme_bg_color } else { Color32::TRANSPARENT })
                            .stroke(if selected { egui::Stroke::new(1.0_f32, accent.gamma_multiply(0.5)) } else { egui::Stroke::NONE })
                            .corner_radius(CornerRadius::same(self.profile.settings.roundness))
                            .inner_margin(Margin::symmetric(8, 5));
                        let resp = frame.show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                self.tab_icon(ui, tab);
                                if tab.pinned {
                                    icon(ui, Icon::Pin, 12.0, ui.visuals().weak_text_color());
                                }
                                let gap = ui.spacing().item_spacing.x;
                                let room = CLOSE_SIZE + gap + if tab.playing || tab.muted { 18.0 + gap } else { 0.0 };
                                let bg = if selected { ui.visuals().extreme_bg_color } else { ui.visuals().panel_fill };
                                faded_title(ui, &tab.title(), selected, ui.available_width() - room, bg);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    close_rect = Some(crate::icons::tab_close_button(ui, CLOSE_SIZE, "Close tab (Ctrl+W)").rect);
                                    if tab.playing || tab.muted {
                                        let i = if tab.muted { Icon::Muted } else { Icon::Speaker };
                                        speaker_rect = Some(icon_button(ui, i, 18.0, true, "Mute / unmute tab").rect);
                                    }
                                });
                            });
                        });
                        rects.push(resp.response.rect);
                        let resp = ui.interact(resp.response.rect, Id::new(("vtab", i)), Sense::click_and_drag()).on_hover_text(tab.title());
                        tab_menu(&resp, i, &mut menu_request);
                        // The whole-tab click area sits on top of the buttons, so tell them apart by position.
                        if resp.clicked() {
                            let at = |r: Option<egui::Rect>| r.zip(resp.interact_pointer_pos()).is_some_and(|(r, p)| r.contains(p));
                            if at(close_rect) {
                                close = Some(i)
                            } else if at(speaker_rect) {
                                tab_action = Some((i, TabAction::Mute));
                            } else {
                                select = Some(i)
                            }
                        }
                        if resp.middle_clicked() {
                            close = Some(i);
                        }
                        if resp.dragged() {
                            dragging = Some(i);
                        }
                        if resp.drag_stopped() {
                            dropped = Some(i);
                        }
                    }
                    if let (Some(from), Some(p)) = (dragging.or(dropped), ui.ctx().pointer_latest_pos()) {
                        let target = drop_target(&rects, from, |r| p.y < r.center().y);
                        let target = clamp_to_group(&self.tabs, from, target);
                        let y = if target > from { rects[target].bottom() } else { rects[target].top() };
                        let x = rects[0].x_range();
                        ui.painter().line_segment([pos2(x.min, y), pos2(x.max, y)], egui::Stroke::new(2.0_f32, accent));
                        if dropped.is_some() {
                            moved = Some((from, target));
                        }
                    }
                });
            });
        if menu_request.is_some() {
            self.tab_menu = menu_request;
        }
        if let Some((from, to)) = moved {
            self.move_tab(from, to);
        } else if let Some((i, action)) = tab_action {
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

    pub(crate) fn custom_title_bar(&self) -> bool {
        self.profile.settings.custom_title_bar && self.custom_title_bar_at_start
    }

    /// The empty part of the title bar (drag to move the window, double-click
    /// to maximise) and, except on macOS, the window buttons at its right.
    fn title_bar_area(&mut self, ui: &mut Ui, strip: egui::Rect, buttons_width: f32) -> Option<crate::icons::WindowButton> {
        let buttons = egui::Rect::from_min_max(pos2(strip.right() - buttons_width, strip.top()), pos2(strip.right(), strip.top() + 32.0_f32.min(strip.height())));
        let drag = egui::Rect::from_min_max(pos2(ui.cursor().left() + 4.0, strip.top()), pos2(buttons.left(), strip.bottom()));
        if drag.width() > 0.0 {
            let resp = ui.interact(drag, Id::new("title_bar_drag"), Sense::click_and_drag());
            if resp.drag_started() {
                let _ = self.window.drag_window();
            }
            if resp.double_clicked() {
                self.window.set_maximized(!self.window.is_maximized());
            }
            if resp.secondary_clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    self.window.show_window_menu(winit::dpi::LogicalPosition::new(p.x, p.y));
                }
            }
        }
        if buttons_width > 0.0 {
            crate::icons::window_buttons(ui, buttons, self.window.is_maximized())
        } else {
            None
        }
    }

    fn window_button(&mut self, button: crate::icons::WindowButton) {
        use crate::icons::WindowButton;
        match button {
            WindowButton::Minimize => self.window.set_minimized(true),
            WindowButton::Maximize => self.window.set_maximized(!self.window.is_maximized()),
            WindowButton::Close => self.request_quit(),
        }
    }

    /// A slim title bar for the "tabs down the side" layout, which has no tab strip to use.
    fn draw_title_bar(&mut self, ui: &mut Ui) {
        let buttons_width = if cfg!(target_os = "macos") { 0.0 } else { WINDOW_BUTTONS_WIDTH };
        let mut action = None;
        egui::Panel::top("title_bar")
            .exact_size(30.0)
            .frame(Frame::new().fill(ui.visuals().panel_fill))
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                let strip = ui.max_rect();
                let title = self.last_title.clone();
                ui.painter().text(strip.center(), Align2::CENTER_CENTER, title, FontId::proportional(12.0), ui.visuals().weak_text_color());
                ui.horizontal(|ui| {
                    if cfg!(target_os = "macos") {
                        ui.add_space(70.0);
                    }
                    action = self.title_bar_area(ui, strip, buttons_width);
                });
            });
        if let Some(action) = action {
            self.window_button(action);
        }
    }

    pub(crate) fn tab_action(&mut self, i: usize, action: TabAction) {
        if i >= self.tabs.len() {
            return;
        }
        match action {
            TabAction::Reload => {
                self.activate(i);
                self.reload();
            },
            TabAction::Duplicate => self.duplicate_tab(i),
            TabAction::Bookmark => {
                if let Some(url) = self.tabs[i].url() {
                    let title = self.tabs[i].title();
                    if !self.profile.is_bookmarked(url.as_str()) {
                        self.profile.toggle_bookmark(url.as_str(), &title);
                    }
                    self.toast("Bookmarked");
                }
            },
            TabAction::Pin => self.toggle_pin(i),
            TabAction::Mute => self.toggle_mute(i),
            TabAction::Close => self.close_tab(i),
            TabAction::CloseOthers => {
                let mut keep = i;
                for j in (0..self.tabs.len()).rev() {
                    if j != i && !self.tabs[j].pinned {
                        self.close_tab_quietly(j);
                        if j < keep {
                            keep -= 1;
                        }
                    }
                }
                self.activate(keep);
            },
            TabAction::CloseRight => {
                for j in (i + 1..self.tabs.len()).rev() {
                    if !self.tabs[j].pinned {
                        self.close_tab_quietly(j);
                    }
                }
                self.activate(self.active.min(self.tabs.len() - 1));
            },
        }
    }

    /// Favicon, spinner, crash marker or letter tile for a tab.
    fn tab_icon(&self, ui: &mut Ui, tab: &crate::browser::Tab) {
        if let Some(page) = tab.internal {
            let i = match page {
                crate::browser::InternalPage::Settings => Icon::Gear,
                crate::browser::InternalPage::Shortcuts => Icon::Info,
                crate::browser::InternalPage::Source => Icon::Code,
            };
            icon(ui, i, 16.0, ui.visuals().strong_text_color());
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
        let mut menu_request: Option<(usize, egui::Pos2)> = None;
        let fill = ui.visuals().panel_fill;
        let tab_fill = ui.visuals().extreme_bg_color;
        let private = self.profile.is_private();
        let accent = self.accent();
        let (mut dragging, mut dropped, mut moved): (Option<usize>, Option<usize>, Option<(usize, usize)>) = (None, None, None);

        let custom_bar = self.custom_title_bar();
        let buttons_width = if custom_bar && !cfg!(target_os = "macos") { WINDOW_BUTTONS_WIDTH } else { 0.0 };
        let mut title_bar_action = None;

        egui::Panel::top("tabs")
            .frame(Frame::new().fill(fill).inner_margin(Margin { left: 6, right: 6, top: 6, bottom: 0 }))
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                let strip = ui.max_rect().expand2(vec2(6.0, 0.0)).with_min_y(ui.max_rect().top() - 6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if custom_bar && cfg!(target_os = "macos") {
                        // Room for the traffic-light buttons in the transparent title bar.
                        ui.add_space(70.0);
                    }
                    if private {
                        ui.label(RichText::new(" Private ").size(11.0).strong().color(Color32::WHITE).background_color(Color32::from_rgb(110, 60, 160)));
                    }
                    // Leave room for the new-tab button, the window buttons and a
                    // patch of empty strip to drag the window by.
                    let drag_room = if custom_bar { 48.0 } else { 0.0 };
                    let available = ui.available_width() - 40.0 - buttons_width - drag_room;
                    let pinned_count = self.tabs.iter().filter(|t| t.pinned).count();
                    const PINNED_WIDTH: f32 = 42.0;
                    let unpinned = (self.tabs.len() - pinned_count).max(1) as f32;
                    // Each tab is followed by 2 px of spacing.
                    let width = ((available - pinned_count as f32 * (PINNED_WIDTH + 2.0) - unpinned * 2.0) / unpinned).floor().clamp(60.0, 220.0);
                    egui::ScrollArea::horizontal()
                        .max_width(available)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let mut rects: Vec<egui::Rect> = Vec::new();
                                for (i, tab) in self.tabs.iter().enumerate() {
                                    let selected = i == self.active;
                                    let mut close_rect: Option<egui::Rect> = None;
                                    let mut speaker_rect: Option<egui::Rect> = None;
                                    let frame = Frame::new()
                                        .fill(if selected { tab_fill } else { Color32::TRANSPARENT })
                                        .corner_radius(CornerRadius { nw: self.profile.settings.roundness + 2, ne: self.profile.settings.roundness + 2, sw: 0, se: 0 })
                                        .inner_margin(if tab.pinned { Margin { left: 13, right: 4, top: 6, bottom: 6 } } else { Margin { left: 10, right: 4, top: 6, bottom: 6 } });
                                    let resp = frame.show(ui, |ui| {
                                        if tab.pinned {
                                            // Pinned: just the icon (and a speaker when playing).
                                            ui.set_width(PINNED_WIDTH - 17.0);
                                            ui.horizontal(|ui| {
                                                self.tab_icon(ui, tab);
                                                if tab.playing || tab.muted {
                                                    // A small dot: accent while playing, grey when muted.
                                                    let rect = ui.min_rect();
                                                    let dot = if tab.muted { ui.visuals().weak_text_color() } else { accent };
                                                    ui.painter().circle_filled(rect.right_top() + vec2(1.0, 1.0), 3.5, dot);
                                                }
                                            });
                                            return;
                                        }
                                        ui.set_width(width - 14.0);
                                        ui.horizontal(|ui| {
                                            self.tab_icon(ui, tab);
                                            if tab.playing || tab.muted {
                                                let i = if tab.muted { Icon::Muted } else { Icon::Speaker };
                                                let tip = if tab.muted { "Unmute tab" } else { "Mute tab" };
                                                speaker_rect = Some(icon_button(ui, i, 18.0, true, tip).rect);
                                            }
                                            let show_close = width > 90.0 || selected;
                                            let gap = ui.spacing().item_spacing.x;
                                            let room = if show_close { CLOSE_SIZE + gap } else { 0.0 };
                                            let bg = if selected { tab_fill } else { fill };
                                            faded_title(ui, &tab.title(), selected, ui.available_width() - room, bg);
                                            if show_close {
                                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                    close_rect = Some(crate::icons::tab_close_button(ui, CLOSE_SIZE, "Close tab (Ctrl+W)").rect);
                                                });
                                            }
                                        });
                                    });
                                    rects.push(resp.response.rect);
                                    let resp = ui.interact(resp.response.rect, Id::new(("tab", i)), Sense::click_and_drag());
                                    let resp = resp.on_hover_text(tab.title());
                                    tab_menu(&resp, i, &mut menu_request);
                                    // The whole-tab click area sits on top of the buttons, so tell them apart by position.
                                    if resp.clicked() {
                                        let at = |r: Option<egui::Rect>| r.zip(resp.interact_pointer_pos()).is_some_and(|(r, p)| r.contains(p));
                                        if at(close_rect) {
                                            close = Some(i)
                                        } else if at(speaker_rect) {
                                            tab_action = Some((i, TabAction::Mute));
                                        } else {
                                            select = Some(i)
                                        }
                                    }
                                    if resp.middle_clicked() {
                                        close = Some(i);
                                    }
                                    if resp.dragged() {
                                        dragging = Some(i);
                                    }
                                    if resp.drag_stopped() {
                                        dropped = Some(i);
                                    }
                                }
                                // While dragging a tab, show where it will land.
                                if let (Some(from), Some(p)) = (dragging.or(dropped), ui.ctx().pointer_latest_pos()) {
                                    let target = drop_target(&rects, from, |r| p.x < r.center().x);
                                    let target = clamp_to_group(&self.tabs, from, target);
                                    let x = if target > from { rects[target].right() } else { rects[target].left() };
                                    let y = rects[0].y_range();
                                    ui.painter().line_segment([pos2(x, y.min + 3.0), pos2(x, y.max - 3.0)], egui::Stroke::new(2.0_f32, accent));
                                    if dropped.is_some() {
                                        moved = Some((from, target));
                                    }
                                }
                            });
                        });
                    if icon_button(ui, Icon::Plus, 26.0, true, "New tab (Ctrl+T)").clicked() {
                        new_tab = true;
                    }
                    if custom_bar {
                        title_bar_action = self.title_bar_area(ui, strip, buttons_width);
                    }
                });
            });
        if let Some(action) = title_bar_action {
            self.window_button(action);
        }

        if menu_request.is_some() {
            self.tab_menu = menu_request;
        }
        if let Some((from, to)) = moved {
            self.move_tab(from, to);
        } else if let Some((i, action)) = tab_action {
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
        let has_page = self.active_webview().is_some();
        let can_back = self.can_go_back();
        let can_forward = self.can_go_forward();
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
        let reader_on = self.active_tab().reader.is_some();
        let blocked = self.active_tab().blocked;
        let (site_icon, site_color, site_tip) = match url.as_ref().map(|u| u.scheme()) {
            Some("https") => (Icon::Lock, None, "Connection is secure — site information".to_string()),
            Some("http") => (Icon::Warning, Some(Color32::from_rgb(200, 120, 30)), "Not secure — site information".to_string()),
            Some(_) => (Icon::Info, None, "Page information".to_string()),
            None => (Icon::Info, None, String::new()),
        };

        let mut url_rect = egui::Rect::NOTHING;
        let mut url_focused = false;

        let toolbar = egui::Panel::top("toolbar")
            .frame(Frame::new().fill(fill).inner_margin(Margin::symmetric(8, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if icon_button(ui, Icon::Back, button, can_back, "Back (Alt+←)").clicked() {
                        self.go_back();
                    }
                    if icon_button(ui, Icon::Forward, button, can_forward, "Forward (Alt+→)").clicked() {
                        self.go_forward();
                    }
                    let (icon, tip) = if loading { (Icon::Stop, "Stop loading") } else { (Icon::Reload, "Reload (Ctrl+R)") };
                    if icon_button(ui, icon, button, has_page, tip).clicked() {
                        if loading { self.stop() } else { self.reload() }
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
                        if has_page {
                            let on = reader_on;
                            let resp = icon_button(ui, Icon::Reader, button, true, if on { "Exit reader mode (F9)" } else { "Reader mode (F9)" });
                            if on {
                                ui.painter().rect_stroke(resp.rect.shrink(2.0), 5.0, egui::Stroke::new(1.5_f32, accent), egui::StrokeKind::Inside);
                            }
                            if resp.clicked() {
                                self.toggle_reader();
                            }
                        }
                        if (zoom - 1.0).abs() > 0.001 && ui.small_button(format!("{}%", (zoom * 100.0).round())).on_hover_text("Reset zoom (Ctrl+0)").clicked() {
                            self.set_zoom(1.0);
                        }

                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            if url.is_some() && !self.editing {
                                let tip = if blocked > 0 { format!("{site_tip}\n{blocked} trackers blocked") } else { site_tip.clone() };
                                let resp = match site_color {
                                    Some(c) => {
                                        let r = ui.allocate_response(Vec2::splat(button), Sense::click()).on_hover_text(tip);
                                        if r.hovered() {
                                            ui.painter().rect_filled(r.rect, button * 0.25, ui.visuals().widgets.hovered.weak_bg_fill);
                                        }
                                        crate::icons::paint_icon(ui, site_icon, r.rect.shrink(button * 0.3), c);
                                        r
                                    },
                                    None => icon_button(ui, site_icon, button, true, &tip),
                                };
                                if blocked > 0 {
                                    let badge = resp.rect.right_bottom() + vec2(-4.0, -5.0);
                                    ui.painter().circle_filled(badge, 6.0, accent);
                                    let n = if blocked > 99 { "99+".to_string() } else { blocked.to_string() };
                                    ui.painter().text(badge, Align2::CENTER_CENTER, n, FontId::proportional(if blocked > 9 { 7.0 } else { 9.0 }), Color32::WHITE);
                                }
                                ui.memory_mut(|m| m.data.insert_temp(Id::new("site_button"), resp.rect));
                                if resp.clicked() {
                                    self.site_info_open = !self.site_info_open;
                                }
                            }
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
                            } else if self.blur_location {
                                response.surrender_focus();
                            }
                            self.blur_location = false;
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

        // Page-load progress line along the bottom of the toolbar (the page
        // itself is a native view, so nothing can be drawn on it).
        if loading {
            let rect = toolbar.response.rect;
            let t = ui.input(|i| i.time) as f32;
            let w = rect.width();
            let x = rect.left() + ((t * 0.9).fract() * 1.4 - 0.4) * w;
            let bar = egui::Rect::from_min_max(pos2(x.max(rect.left()), rect.bottom() - 2.5), pos2((x + w * 0.4).min(rect.right()), rect.bottom()));
            ui.painter().rect_filled(bar, 1.0, accent);
            // A modest frame rate keeps this cheap on slow machines.
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
        ui.memory_mut(|m| {
            m.data.insert_temp(Id::new("url_rect"), url_rect);
            m.data.insert_temp(Id::new("url_focused"), url_focused);
        });
    }

    fn draw_bookmarks_bar(&mut self, ui: &mut Ui) {
        let mut open: Option<(String, bool)> = None;
        let mut menu = None;
        egui::Panel::top("bookmarks_bar")
            .frame(Frame::new().fill(ui.visuals().extreme_bg_color).inner_margin(Margin { left: 8, right: 8, top: 0, bottom: 4 }))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if self.profile.bookmarks.is_empty() {
                        let ctrl = if cfg!(target_os = "macos") { "⌘D" } else { "Ctrl+D" };
                        ui.label(RichText::new(format!("Bookmarks appear here. Press {ctrl} on a page to add one.")).size(12.0).weak());
                    }
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
                            .on_hover_text(format!("{}\n{}\nRight-click to edit or remove", b.title, b.url));
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
                        if resp.secondary_clicked() {
                            let at = resp.interact_pointer_pos().unwrap_or(resp.rect.left_bottom());
                            menu = Some((LinkMenu::Bookmark(b.url.clone()), at));
                        }
                    }
                });
            });
        if menu.is_some() {
            self.link_menu = menu;
        }
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
        match self.active_tab().internal {
            Some(crate::browser::InternalPage::Settings) => {
                self.draw_settings_page(ui, rect);
                return;
            },
            Some(crate::browser::InternalPage::Shortcuts) => {
                self.draw_shortcuts_page(ui, rect);
                return;
            },
            Some(crate::browser::InternalPage::Source) => {
                if let Some(source) = self.tabs[self.active].source.as_mut() {
                    crate::devtools::draw_source(ui, rect, source);
                }
                return;
            },
            None => {},
        }
        if self.draw_reader(ui, rect) {
            return;
        }
        if self.active_webview().is_none() {
            self.draw_start_page(ui, rect);
            return;
        }
        // The page itself is the engine's native view, placed over `rect`
        // after this frame (Browser::layout_pages). Paint the area in the
        // page's usual white so resizing never flashes the theme colour.
        ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
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
                // Each tile opens on click, has a right-click menu, and an ✕ on hover.
                let mut menu: Option<(LinkMenu, egui::Pos2)> = None;
                let mut remove: Option<LinkMenu> = None;
                let tiles = |ui: &mut Ui,
                             heading: &str,
                             items: Vec<(String, String)>,
                             bookmarks: bool,
                             go: &mut Option<(String, bool)>,
                             menu: &mut Option<(LinkMenu, egui::Pos2)>,
                             remove: &mut Option<LinkMenu>| {
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
                                let target = if bookmarks { LinkMenu::Bookmark(url.clone()) } else { LinkMenu::Frequent(url.clone()) };
                                let x = egui::Rect::from_center_size(r.right_top() + vec2(-14.0, 14.0), Vec2::splat(18.0));
                                let over_x = resp.hover_pos().is_some_and(|p| x.contains(p));
                                if resp.hovered() {
                                    let color = if over_x { text_color } else { weak_color };
                                    if over_x {
                                        ui.painter().circle_filled(x.center(), 9.0, ui.visuals().widgets.active.weak_bg_fill);
                                    }
                                    let d = vec2(3.5, 3.5);
                                    let k = x.center();
                                    ui.painter().line_segment([k - d, k + d], Stroke::new(1.5_f32, color));
                                    ui.painter().line_segment([k + vec2(-d.x, d.y), k + vec2(d.x, -d.y)], Stroke::new(1.5_f32, color));
                                }
                                let tip = if bookmarks { "Remove bookmark" } else { "Remove from this page" };
                                let resp = if over_x { resp.on_hover_text(tip) } else { resp.on_hover_text(url) };
                                if resp.clicked() && over_x {
                                    *remove = Some(target.clone());
                                } else if resp.clicked() {
                                    *go = Some((url.clone(), false));
                                }
                                if resp.secondary_clicked() {
                                    *menu = Some((target, resp.interact_pointer_pos().unwrap_or(r.center())));
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
                    tiles(ui, "Bookmarks", items, true, &mut go, &mut menu, &mut remove);
                }
                if settings.start_show_frequent {
                    let mut top: Vec<_> = self.profile.history.iter().filter(|h| !self.profile.is_bookmarked(&h.url)).collect();
                    top.sort_by(|a, b| b.visits.cmp(&a.visits).then(b.last_visit.cmp(&a.last_visit)));
                    let items = top
                        .iter()
                        .take(8)
                        .map(|h| (if h.title.is_empty() { h.url.clone() } else { h.title.clone() }, h.url.clone()))
                        .collect();
                    tiles(ui, "Frequently visited", items, false, &mut go, &mut menu, &mut remove);
                }
                if menu.is_some() {
                    self.link_menu = menu;
                }
                if let Some(target) = remove {
                    self.remove_link(&target);
                }
            });
        });

        // Quick link to the look settings.
        let btn = egui::Rect::from_min_size(rect.right_bottom() - vec2(120.0, 40.0), vec2(110.0, 30.0));
        let resp = ui.put(btn, egui::Button::new(RichText::new("Customize").size(12.0)));
        if resp.clicked() {
            self.settings_section = crate::settings_page::Section::Appearance;
            self.settings_query.clear();
            self.open_settings();
        }

        if let Some((target, new_tab)) = go {
            match Url::parse(&target) {
                Ok(u) if new_tab => self.open_background_tab(u),
                _ => self.navigate_input(&target),
            }
        }
    }

    // ----- page-requested dialogs and popups ---------------------------------

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
                    SidePanel::None => {},
                }
            });
    }

    /// A row with an icon, title and address. `buttons` adds buttons at the
    /// right end (right to left; the bool says whether the row is hovered).
    /// They take their own space, so clicking one never opens the link.
    fn link_row(ui: &mut Ui, id: impl std::hash::Hash, title: &str, url: &str, buttons: impl FnOnce(&mut Ui, bool)) -> egui::Response {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        let hovered = ui.rect_contains_pointer(rect);
        if hovered {
            ui.painter().rect_filled(rect, 6.0, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.6));
        }
        let mut right = rect.right();
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(4.0, 0.0))).layout(Layout::right_to_left(Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            buttons(ui, hovered);
            right = ui.available_rect_before_wrap().right();
        });
        let text_rect = egui::Rect::from_min_max(rect.min, pos2(right - 4.0, rect.max.y));
        let resp = ui.interact(text_rect, Id::new(("row", id)), Sense::click()).on_hover_text(url);
        ui.scope_builder(egui::UiBuilder::new().max_rect(text_rect.shrink2(vec2(6.0, 0.0))).layout(Layout::left_to_right(Align::Center)), |ui| {
            let shown = if title.is_empty() { url } else { title };
            letter_icon(ui, shown, url, 20.0);
            ui.vertical(|ui| {
                ui.add_space(3.0);
                ui.add(egui::Label::new(RichText::new(shown).size(13.0)).truncate().selectable(false));
                ui.add(egui::Label::new(RichText::new(url).size(11.0).weak()).truncate().selectable(false));
            });
        });
        resp
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
                let mut x = false;
                let resp = Self::link_row(ui, ("h", i), &h.title, &h.url, |ui, hovered| {
                    if hovered {
                        x = icon_button(ui, Icon::Close, 22.0, true, "Remove from history").clicked();
                    }
                });
                if resp.clicked() {
                    open = Some((h.url.clone(), false));
                }
                if resp.middle_clicked() {
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
        ui.horizontal(|ui| {
            if ui.button("Import from browser…").on_hover_text("Chrome, Edge, Firefox, Safari, Brave…").clicked() {
                self.settings_section = crate::settings_page::Section::Advanced;
                self.settings_query = "import".into();
                self.open_settings();
            }
            ui.menu_button("More", |ui| {
                if ui.button("Import bookmarks file (HTML)…").clicked() {
                    self.import_bookmarks();
                    ui.close();
                }
                if ui.button("Export bookmarks…").clicked() {
                    self.export_bookmarks();
                    ui.close();
                }
            });
        });
        ui.add_space(4.0);
        if let Some(url) = self.active_tab().url().filter(|u| matches!(u.scheme(), "http" | "https" | "file")) {
            if !self.profile.is_bookmarked(url.as_str()) && ui.button("☆  Bookmark this page").clicked() {
                self.toggle_bookmark();
            }
        }
        ui.checkbox(&mut self.profile.settings.show_bookmarks_bar, "Show bookmarks bar");
        ui.add_space(4.0);
        ui.add(egui::TextEdit::singleline(&mut self.bookmark_filter).hint_text("Search bookmarks").desired_width(f32::INFINITY));
        if let Some((_, b)) = &self.removed_bookmark {
            let title = truncate(&b.title, 30);
            let mut undo = false;
            let mut dismiss = false;
            Frame::new().fill(ui.visuals().faint_bg_color).corner_radius(6.0).inner_margin(Margin::symmetric(8, 4)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Removed “{title}”")).size(12.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        dismiss = icon_button(ui, Icon::Close, 18.0, true, "Dismiss").clicked();
                        undo = ui.small_button("Undo").clicked();
                    });
                });
            });
            if undo {
                self.undo_remove_bookmark();
            } else if dismiss {
                self.removed_bookmark = None;
            }
        }
        ui.add_space(6.0);
        let mut open = None;
        let mut remove = None;
        let mut swap = None;
        let mut start_edit = None;
        let mut menu = None;
        let mut finish_edit: Option<bool> = None;
        let n = self.profile.bookmarks.len();
        let needle = self.bookmark_filter.trim().to_lowercase();
        let searching = !needle.is_empty();
        let mut shown = 0;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (i, b) in self.profile.bookmarks.iter().enumerate() {
                if searching && !b.title.to_lowercase().contains(&needle) && !b.url.to_lowercase().contains(&needle) {
                    continue;
                }
                shown += 1;
                if let Some((orig, title, url)) = self.bookmark_edit.as_mut().filter(|(u, ..)| *u == b.url) {
                    let _ = orig;
                    Frame::new().fill(ui.visuals().extreme_bg_color).corner_radius(8.0).inner_margin(Margin::same(8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("Name").size(11.0).weak());
                        let t = ui.add(egui::TextEdit::singleline(title).desired_width(f32::INFINITY));
                        ui.label(RichText::new("Address").size(11.0).weak());
                        let u = ui.add(egui::TextEdit::singleline(url).desired_width(f32::INFINITY));
                        if (t.lost_focus() || u.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            finish_edit = Some(true);
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() {
                                finish_edit = Some(true);
                            }
                            if ui.button("Cancel").clicked() {
                                finish_edit = Some(false);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if icon_button(ui, Icon::Trash, 22.0, true, "Remove bookmark").clicked() {
                                    remove = Some(b.url.clone());
                                }
                            });
                        });
                    });
                    ui.add_space(2.0);
                    continue;
                }
                let resp = Self::link_row(ui, ("b", i), &b.title, &b.url, |ui, hovered| {
                    if icon_button(ui, Icon::Trash, 22.0, true, "Remove bookmark").clicked() {
                        remove = Some(b.url.clone());
                    }
                    if icon_button(ui, Icon::Edit, 22.0, true, "Edit name or address").clicked() {
                        start_edit = Some(b.url.clone());
                    }
                    if hovered && !searching {
                        if icon_button(ui, Icon::Down, 22.0, i + 1 < n, "Move down").clicked() {
                            swap = Some((i, i + 1));
                        }
                        if icon_button(ui, Icon::Up, 22.0, i > 0, "Move up").clicked() {
                            swap = Some((i, i - 1));
                        }
                    }
                });
                if resp.clicked() {
                    open = Some((b.url.clone(), false));
                }
                if resp.middle_clicked() {
                    open = Some((b.url.clone(), true));
                }
                if resp.secondary_clicked() {
                    menu = Some((LinkMenu::Bookmark(b.url.clone()), resp.interact_pointer_pos().unwrap_or(resp.rect.center())));
                }
            }
            if n == 0 {
                let ctrl = if cfg!(target_os = "macos") { "⌘D" } else { "Ctrl+D" };
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No bookmarks yet").strong());
                    ui.label(RichText::new(format!("Press {ctrl} or click the star in the address bar to bookmark a page, or import them from another browser.")).weak());
                });
            } else if shown == 0 {
                ui.label(RichText::new("No bookmarks match.").weak());
            }
        });
        if let Some(save) = finish_edit {
            if let Some((orig, title, url)) = self.bookmark_edit.take() {
                if save {
                    self.save_bookmark_edit(&orig, title, url);
                }
            }
        }
        if let Some(url) = start_edit {
            self.edit_bookmark(&url);
        }
        if let Some(url) = remove {
            self.remove_bookmark(&url);
        }
        if let Some((a, b)) = swap {
            self.profile.bookmarks.swap(a, b);
            self.profile.mark_dirty();
        }
        if menu.is_some() {
            self.link_menu = menu;
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
            if let Some(dir) = self.download_dir() {
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

}
