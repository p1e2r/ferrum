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
use crate::icons::{Icon, icon_button};
use crate::profile::{SearchEngine, ThemeChoice};

const ACCENT: Color32 = Color32::from_rgb(222, 98, 52);

pub fn style(ctx: &egui::Context, theme: ThemeChoice) {
    ctx.options_mut(|o| {
        o.zoom_with_keyboard = false;
        o.fallback_theme = egui::Theme::Light;
    });
    set_theme(ctx, theme);
    ctx.all_styles_mut(|style| {
        style.spacing.button_padding = vec2(10.0, 5.0);
        style.spacing.item_spacing = vec2(6.0, 6.0);
        style.visuals.selection.bg_fill = ACCENT;
        style.visuals.hyperlink_color = ACCENT;
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

/// Letter tile used where a site has no favicon.
fn letter_icon(ui: &mut Ui, text: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let letter = text
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .chars()
        .find(|c| c.is_alphanumeric())
        .unwrap_or('•')
        .to_uppercase()
        .to_string();
    let hash = text.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    let hue = (hash % 360) as f32 / 360.0;
    let fill: Color32 = egui::ecolor::Hsva::new(hue, 0.45, 0.75, 1.0).into();
    ui.painter().rect_filled(rect, size * 0.25, fill);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, letter, FontId::proportional(size * 0.6), Color32::WHITE);
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
    pub(crate) fn draw(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.ppp = ctx.pixels_per_point();
        self.apply_theme(&ctx);
        for i in 0..self.tabs.len() {
            self.refresh_favicon(&ctx, i);
        }

        let chrome = !self.page_fullscreen && !self.window_fullscreen;
        if chrome {
            self.draw_tab_strip(ui);
            self.draw_toolbar(ui);
            if self.profile.settings.show_bookmarks_bar && !self.profile.bookmarks.is_empty() {
                self.draw_bookmarks_bar(ui);
            }
        }
        if chrome && self.panel != SidePanel::None {
            self.draw_side_panel(ui);
        }

        self.content_rect = ui.available_rect_before_wrap();
        self.draw_content(ui);
        self.draw_page_controls(&ctx);
        if self.menu_open {
            self.draw_menu(&ctx);
        }
        self.draw_toast(&ctx);
    }

    // ----- tab strip --------------------------------------------------------

    fn draw_tab_strip(&mut self, ui: &mut Ui) {
        let mut select = None;
        let mut close = None;
        let mut new_tab = false;
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
                                    let frame = Frame::new()
                                        .fill(if selected { tab_fill } else { Color32::TRANSPARENT })
                                        .corner_radius(CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 })
                                        .inner_margin(Margin { left: 10, right: 4, top: 6, bottom: 6 });
                                    let resp = frame.show(ui, |ui| {
                                        ui.set_width(width - 14.0);
                                        ui.horizontal(|ui| {
                                            if tab.crashed.is_some() {
                                                ui.label(RichText::new("⚠").color(Color32::from_rgb(220, 60, 60)));
                                            } else if tab.loading() {
                                                ui.add(egui::Spinner::new().size(14.0));
                                            } else if let Some(icon) = &tab.favicon {
                                                ui.add(egui::Image::new(icon).fit_to_exact_size(Vec2::splat(16.0)));
                                            } else {
                                                letter_icon(ui, &tab.url().map(|u| u.to_string()).unwrap_or_else(|| "F".into()), 16.0);
                                            }
                                            let title = tab.title();
                                            let text = RichText::new(title.clone()).size(13.0);
                                            let text = if selected { text.strong() } else { text };
                                            ui.add(egui::Label::new(text).truncate().selectable(false));
                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                if width > 90.0 || selected {
                                                    if icon_button(ui, Icon::Close, 18.0, true, "Close tab (Ctrl+W)").clicked() {
                                                        close = Some(i);
                                                    }
                                                }
                                            });
                                        });
                                    });
                                    let resp = ui.interact(resp.response.rect, Id::new(("tab", i)), Sense::click());
                                    let resp = resp.on_hover_text(tab.title());
                                    if resp.clicked() {
                                        select = Some(i);
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

        if let Some(i) = close {
            self.close_tab(i);
        } else if let Some(i) = select {
            self.activate(i);
        }
        if new_tab {
            self.open_home_tab();
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

        let mut url_rect = egui::Rect::NOTHING;
        let mut url_focused = false;

        egui::Panel::top("toolbar")
            .frame(Frame::new().fill(fill).inner_margin(Margin::symmetric(8, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if icon_button(ui, Icon::Back, 30.0, can_back, "Back (Alt+←)").clicked() {
                        self.go_back();
                    }
                    if icon_button(ui, Icon::Forward, 30.0, can_forward, "Forward (Alt+→)").clicked() {
                        self.go_forward();
                    }
                    let (icon, tip) = if loading { (Icon::Stop, "Loading…") } else { (Icon::Reload, "Reload (Ctrl+R)") };
                    if icon_button(ui, icon, 30.0, webview.is_some(), tip).clicked() && !loading {
                        self.reload();
                    }
                    if icon_button(ui, Icon::Home, 30.0, true, "Home").clicked() {
                        self.go_home();
                    }

                    // Right-hand buttons are laid out first so the address bar can fill the rest.
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let menu = icon_button(ui, Icon::Menu, 30.0, true, "Menu");
                        if menu.clicked() {
                            self.menu_open = !self.menu_open;
                        }
                        ui.memory_mut(|m| m.data.insert_temp(Id::new("menu_button"), menu.rect));
                        let dl = icon_button(ui, Icon::Download, 30.0, true, "Downloads (Ctrl+J)");
                        if active_downloads > 0 {
                            let badge = egui::Rect::from_center_size(dl.rect.right_top() + vec2(-6.0, 6.0), Vec2::splat(14.0));
                            ui.painter().circle_filled(badge.center(), 7.0, ACCENT);
                            ui.painter().text(badge.center(), Align2::CENTER_CENTER, active_downloads.to_string(), FontId::proportional(9.0), Color32::WHITE);
                        }
                        if dl.clicked() {
                            self.toggle_panel(SidePanel::Downloads);
                        }
                        if url.is_some() {
                            let star = if bookmarked { Icon::StarFilled } else { Icon::Star };
                            if icon_button(ui, star, 30.0, true, "Bookmark this page (Ctrl+D)").clicked() {
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
            ui.painter().rect_filled(bar, 1.0, ACCENT);
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        }

        if url_focused && self.editing {
            self.draw_suggestions(ui.ctx(), url_rect);
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
        egui::Area::new(Id::new("suggestions"))
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
                                    ui.label(RichText::new("★").color(Color32::from_rgb(232, 160, 32)));
                                } else {
                                    ui.label(RichText::new("↺").weak());
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
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for b in &self.profile.bookmarks {
                        let resp = ui
                            .add(egui::Button::new(RichText::new(truncate(&b.title, 22)).size(12.0)).frame(false))
                            .on_hover_text(&b.url);
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

    fn draw_start_page(&mut self, ui: &mut Ui, rect: egui::Rect) {
        ui.painter().rect_filled(rect, 0.0, ui.visuals().window_fill);
        let width = (rect.width() * 0.7).clamp(280.0, 720.0);
        let area = egui::Rect::from_min_size(pos2(rect.center().x - width / 2.0, rect.top() + rect.height() * 0.16), vec2(width, rect.height() * 0.84));
        let mut go: Option<(String, bool)> = None;

        ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(APP_NAME).size(52.0).strong().color(ACCENT));
                let sub = if self.profile.is_private() {
                    "Private window — history, cookies and site data are forgotten when it closes"
                } else {
                    "A native Rust browser · Servo engine"
                };
                ui.label(RichText::new(sub).size(14.0).weak());
                ui.add_space(22.0);
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.start_query)
                        .hint_text(format!("Search with {} or type an address", self.profile.settings.search_engine.name()))
                        .font(FontId::proportional(17.0))
                        .margin(Margin::symmetric(14, 10))
                        .desired_width(width),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(EKey::Enter)) && !self.start_query.trim().is_empty() {
                    go = Some((std::mem::take(&mut self.start_query), false));
                }
                ui.add_space(26.0);

                let tile = |ui: &mut Ui, title: &str, url: &str, go: &mut Option<(String, bool)>| {
                    let (r, resp) = ui.allocate_exact_size(vec2(104.0, 92.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 10.0, ui.visuals().widgets.hovered.weak_bg_fill);
                    }
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink(8.0)).layout(Layout::top_down(Align::Center)));
                    letter_icon(&mut child, url, 40.0);
                    child.add(egui::Label::new(RichText::new(truncate(title, 14)).size(12.0)).selectable(false));
                    let resp = resp.on_hover_text(url);
                    if resp.clicked() {
                        *go = Some((url.to_string(), false));
                    }
                    if resp.middle_clicked() {
                        *go = Some((url.to_string(), true));
                    }
                };

                if !self.profile.bookmarks.is_empty() {
                    ui.label(RichText::new("Bookmarks").strong().weak());
                    ui.horizontal_wrapped(|ui| {
                        for b in self.profile.bookmarks.iter().take(12) {
                            tile(ui, &b.title, &b.url, &mut go);
                        }
                    });
                }
                let mut top: Vec<_> = self.profile.history.iter().filter(|h| !self.profile.is_bookmarked(&h.url)).collect();
                top.sort_by(|a, b| b.visits.cmp(&a.visits).then(b.last_visit.cmp(&a.last_visit)));
                if !top.is_empty() {
                    ui.add_space(10.0);
                    ui.label(RichText::new("Frequently visited").strong().weak());
                    ui.horizontal_wrapped(|ui| {
                        for h in top.iter().take(8) {
                            let title = if h.title.is_empty() { h.url.as_str() } else { h.title.as_str() };
                            tile(ui, title, &h.url, &mut go);
                        }
                    });
                }
            });
        });

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
                                if ui.add(egui::Button::new(RichText::new("Allow").color(Color32::WHITE)).fill(ACCENT)).clicked() {
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
                        self.open_home_tab();
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
                letter_icon(ui, url, 20.0);
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
            ui.painter().text(close.center(), Align2::CENTER_CENTER, "✕", FontId::proportional(12.0), ui.visuals().weak_text_color());
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
            if !self.profile.is_bookmarked(url.as_str()) && ui.button("★  Bookmark this page").clicked() {
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
                        if ui.add_enabled(i > 0, egui::Button::new("▲").small().frame(false)).clicked() {
                            swap = Some((i, i - 1));
                        }
                        if ui.add_enabled(i + 1 < n, egui::Button::new("▼").small().frame(false)).clicked() {
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
        let ctx = ui.ctx().clone();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let s = &mut self.profile.settings;
            ui.label(RichText::new("Search").strong());
            egui::ComboBox::from_id_salt("engine")
                .selected_text(s.search_engine.name())
                .show_ui(ui, |ui| {
                    for e in SearchEngine::ALL {
                        ui.selectable_value(&mut s.search_engine, e, e.name());
                    }
                });
            ui.add_space(10.0);

            ui.label(RichText::new("Appearance").strong());
            ui.horizontal(|ui| {
                let old = s.theme;
                ui.selectable_value(&mut s.theme, ThemeChoice::System, "System");
                ui.selectable_value(&mut s.theme, ThemeChoice::Light, "Light");
                ui.selectable_value(&mut s.theme, ThemeChoice::Dark, "Dark");
                if s.theme != old {
                    set_theme(&ctx, s.theme);
                }
            });
            ui.checkbox(&mut s.show_bookmarks_bar, "Show bookmarks bar");
            ui.horizontal(|ui| {
                ui.label("Default zoom");
                ui.add(egui::Slider::new(&mut s.default_zoom, 0.5..=2.0).step_by(0.05).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
            });
            ui.add_space(10.0);

            ui.label(RichText::new("Start-up").strong());
            ui.checkbox(&mut s.restore_session, "Reopen tabs from last time");
            ui.label("Home page (leave empty for the start page)");
            ui.add(egui::TextEdit::singleline(&mut s.home_page).hint_text("https://…").desired_width(f32::INFINITY));
            ui.add_space(10.0);

            ui.label(RichText::new("Downloads").strong());
            ui.checkbox(&mut s.ask_where_to_save, "Ask where to save each file");
            ui.add_space(10.0);
        });
        let mut clear_data = false;
        ui.label(RichText::new("Privacy").strong());
        ui.horizontal_wrapped(|ui| {
            if ui.button("Clear history").clicked() {
                self.profile.clear_history();
                self.toast("History cleared");
            }
            if ui.button("Clear cookies & site data").clicked() {
                clear_data = true;
            }
        });
        if clear_data {
            self.clear_site_data();
        }
        ui.add_space(12.0);
        ui.label(RichText::new(format!("{APP_NAME} {} · Servo engine · MIT licensed", env!("CARGO_PKG_VERSION"))).weak().size(11.0));
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
