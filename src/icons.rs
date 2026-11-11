// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Toolbar icons drawn with vector strokes, so the UI needs no icon font or images.

use egui::{Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};

#[derive(Clone, Copy)]
pub enum Icon {
    Back,
    Forward,
    Reload,
    Stop,
    Home,
    Plus,
    Close,
    Menu,
    Star,
    StarFilled,
    Download,
    Up,
    Down,
    Warning,
    History,
    Gear,
    Lock,
    Info,
    Speaker,
    Muted,
    Pin,
    Shield,
    Reader,
    Code,
    Trash,
    Edit,
    Minimize,
    Maximize,
    Restore,
}

/// A square icon button. Returns the click response.
pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32, enabled: bool, tooltip: &str) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), sense);

    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        if enabled && response.hovered() {
            let fill: Color32 = if response.is_pointer_button_down_on() {
                visuals.widgets.active.weak_bg_fill
            } else {
                visuals.widgets.hovered.weak_bg_fill
            };
            ui.painter().rect_filled(rect, (size * 0.25) as u8, fill);
        }
        let color = if enabled {
            visuals.strong_text_color()
        } else {
            visuals.weak_text_color().gamma_multiply(0.6)
        };
        paint_icon(ui, icon, rect.shrink(size * 0.28), color);
    }

    response.on_hover_text(tooltip)
}

/// A tab's close button, drawn like Chrome's: a small, thin ✕ that gets a
/// round grey background on hover and darker when pressed.
pub fn tab_close_button(ui: &mut Ui, size: f32, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let text = visuals.strong_text_color();
        // The tab's own click area lies on top of this button, so work out
        // hovering from the pointer position rather than egui's hover state.
        let hovered = ui.rect_contains_pointer(rect);
        if hovered {
            let pressed = ui.input(|i| i.pointer.primary_down());
            ui.painter().circle_filled(rect.center(), size * 0.5, text.gamma_multiply(if pressed { 0.28 } else { 0.16 }));
        }
        let color = if hovered { text } else { visuals.text_color() };
        let d = size * 0.2;
        let c = rect.center();
        let s = Stroke::new(1.5_f32, color);
        ui.painter().line_segment([c - vec2(d, d), c + vec2(d, d)], s);
        ui.painter().line_segment([c + vec2(-d, d), c + vec2(d, -d)], s);
    }
    response.on_hover_text(tooltip)
}

/// What a click on a window button asks for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WindowButton {
    Minimize,
    Maximize,
    Close,
}

/// Chrome-style minimise / maximise / close buttons, filling `rect` from the
/// right. Close turns red on hover. Returns the button that was clicked.
pub fn window_buttons(ui: &mut Ui, rect: Rect, maximized: bool) -> Option<WindowButton> {
    let width = rect.width() / 3.0;
    let mut clicked = None;
    let buttons = [
        (WindowButton::Minimize, Icon::Minimize, "Minimise"),
        (WindowButton::Maximize, if maximized { Icon::Restore } else { Icon::Maximize }, if maximized { "Restore" } else { "Maximise" }),
        (WindowButton::Close, Icon::Close, "Close"),
    ];
    for (i, (button, icon, tip)) in buttons.into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.left() + width * i as f32, rect.top()), vec2(width, rect.height()));
        let resp = ui.interact(r, ui.id().with(("window_button", i)), Sense::click()).on_hover_text(tip);
        let text = ui.visuals().strong_text_color();
        let mut color = text;
        if resp.hovered() {
            let fill = if button == WindowButton::Close {
                color = Color32::WHITE;
                if resp.is_pointer_button_down_on() { Color32::from_rgb(241, 112, 122) } else { Color32::from_rgb(232, 17, 35) }
            } else {
                text.gamma_multiply(if resp.is_pointer_button_down_on() { 0.2 } else { 0.1 })
            };
            ui.painter().rect_filled(r, 0.0, fill);
        }
        let glyph = Rect::from_center_size(r.center(), Vec2::splat(10.0));
        if matches!(icon, Icon::Close) {
            let s = Stroke::new(1.0_f32, color);
            ui.painter().line_segment([glyph.left_top(), glyph.right_bottom()], s);
            ui.painter().line_segment([glyph.left_bottom(), glyph.right_top()], s);
        } else {
            paint_icon(ui, icon, glyph, color);
        }
        if resp.clicked() {
            clicked = Some(button);
        }
    }
    clicked
}

/// A non-interactive icon of `size`, for use inside rows and labels.
pub fn icon(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_icon(ui, icon, rect.shrink(size * 0.15), color);
    response
}

pub fn paint_icon(ui: &Ui, icon: Icon, r: Rect, color: Color32) {
    let p = ui.painter();
    let s = Stroke::new(1.8_f32, color);
    let c = r.center();
    let w = r.width();
    let h = r.height();
    match icon {
        Icon::Back | Icon::Forward => {
            let dir = if matches!(icon, Icon::Back) { -1.0 } else { 1.0 };
            let tip = pos2(c.x + dir * w * 0.5, c.y);
            p.line_segment([pos2(c.x - dir * w * 0.5, c.y), tip], s);
            p.line_segment([tip, pos2(c.x + dir * w * 0.05, r.top())], s);
            p.line_segment([tip, pos2(c.x + dir * w * 0.05, r.bottom())], s);
        },
        Icon::Reload => {
            let radius = w * 0.5;
            let points: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let a = -0.35 + (i as f32 / 24.0) * std::f32::consts::TAU * 0.82;
                    c + vec2(a.cos(), a.sin()) * radius
                })
                .collect();
            let end = *points.last().unwrap();
            p.add(Shape::line(points, s));
            p.line_segment([end, end + vec2(w * 0.32, -h * 0.02)], s);
            p.line_segment([end, end + vec2(w * 0.04, h * 0.32)], s);
        },
        Icon::Stop | Icon::Close => {
            let k = if matches!(icon, Icon::Close) { 0.38 } else { 0.5 };
            let d = vec2(w * k, h * k);
            p.line_segment([c - d, c + d], s);
            p.line_segment([c + vec2(-d.x, d.y), c + vec2(d.x, -d.y)], s);
        },
        Icon::Plus => {
            p.line_segment([pos2(r.left(), c.y), pos2(r.right(), c.y)], s);
            p.line_segment([pos2(c.x, r.top()), pos2(c.x, r.bottom())], s);
        },
        Icon::Menu => {
            for dy in [-0.32, 0.0, 0.32] {
                let y = c.y + dy * h;
                p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], s);
            }
        },
        Icon::Star | Icon::StarFilled => {
            let outer = w * 0.55;
            let inner = outer * 0.45;
            let pts: Vec<Pos2> = (0..10)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                    let rad = if i % 2 == 0 { outer } else { inner };
                    c + vec2(a.cos(), a.sin()) * rad
                })
                .collect();
            if matches!(icon, Icon::StarFilled) {
                let fill = Color32::from_rgb(232, 160, 32);
                p.add(Shape::convex_polygon(vec![c], fill, Stroke::NONE));
                for i in 0..10 {
                    p.add(Shape::convex_polygon(vec![c, pts[i], pts[(i + 1) % 10]], fill, Stroke::NONE));
                }
                p.add(Shape::closed_line(pts, Stroke::new(1.4_f32, fill)));
            } else {
                p.add(Shape::closed_line(pts, s));
            }
        },
        Icon::Download => {
            p.line_segment([pos2(c.x, r.top()), pos2(c.x, r.bottom() - h * 0.2)], s);
            p.line_segment([pos2(c.x, r.bottom() - h * 0.2), pos2(c.x - w * 0.32, c.y + h * 0.02)], s);
            p.line_segment([pos2(c.x, r.bottom() - h * 0.2), pos2(c.x + w * 0.32, c.y + h * 0.02)], s);
            p.line_segment([pos2(r.left(), r.bottom()), pos2(r.right(), r.bottom())], s);
        },
        Icon::Up | Icon::Down => {
            let d = if matches!(icon, Icon::Up) { -1.0 } else { 1.0 };
            let tip = pos2(c.x, c.y + d * h * 0.28);
            p.line_segment([pos2(r.left() + w * 0.1, c.y - d * h * 0.22), tip], s);
            p.line_segment([tip, pos2(r.right() - w * 0.1, c.y - d * h * 0.22)], s);
        },
        Icon::Warning => {
            let tri = vec![pos2(c.x, r.top()), pos2(r.right(), r.bottom()), pos2(r.left(), r.bottom())];
            p.add(Shape::convex_polygon(tri, color, Stroke::NONE));
            let ink = Color32::WHITE;
            p.line_segment([pos2(c.x, r.top() + h * 0.35), pos2(c.x, r.bottom() - h * 0.32)], Stroke::new(1.6_f32, ink));
            p.circle_filled(pos2(c.x, r.bottom() - h * 0.14), 1.0, ink);
        },
        Icon::Gear => {
            // Eight teeth around a ring.
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let dir = vec2(a.cos(), a.sin());
                p.line_segment([c + dir * w * 0.32, c + dir * w * 0.5], Stroke::new(2.2_f32, color));
            }
            p.circle_stroke(c, w * 0.32, s);
            p.circle_filled(c, w * 0.11, color);
        },
        Icon::History => {
            p.circle_stroke(c, w * 0.5, s);
            p.line_segment([c, pos2(c.x, c.y - h * 0.3)], s);
            p.line_segment([c, pos2(c.x + w * 0.24, c.y + h * 0.1)], s);
        },
        Icon::Home => {
            let roof = [
                pos2(r.left(), c.y),
                pos2(c.x, r.top()),
                pos2(r.right(), c.y),
            ];
            p.add(Shape::line(roof.to_vec(), s));
            let body = [
                pos2(r.left() + w * 0.18, c.y - h * 0.12),
                pos2(r.left() + w * 0.18, r.bottom()),
                pos2(r.right() - w * 0.18, r.bottom()),
                pos2(r.right() - w * 0.18, c.y - h * 0.12),
            ];
            p.add(Shape::line(body.to_vec(), s));
        },
        Icon::Lock => {
            let body = Rect::from_min_max(pos2(r.left() + w * 0.1, c.y - h * 0.05), pos2(r.right() - w * 0.1, r.bottom()));
            p.rect_filled(body, 2.0, color);
            let arc: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let a = std::f32::consts::PI + i as f32 / 12.0 * std::f32::consts::PI;
                    pos2(c.x + a.cos() * w * 0.24, c.y - h * 0.05 + a.sin() * h * 0.42)
                })
                .collect();
            p.add(Shape::line(arc, s));
        },
        Icon::Info => {
            p.circle_stroke(c, w * 0.5, s);
            p.line_segment([pos2(c.x, c.y - h * 0.02), pos2(c.x, c.y + h * 0.28)], s);
            p.circle_filled(pos2(c.x, c.y - h * 0.24), 1.3, color);
        },
        Icon::Speaker | Icon::Muted => {
            let cone = vec![
                pos2(r.left(), c.y - h * 0.16),
                pos2(r.left() + w * 0.22, c.y - h * 0.16),
                pos2(r.left() + w * 0.5, r.top() + h * 0.05),
                pos2(r.left() + w * 0.5, r.bottom() - h * 0.05),
                pos2(r.left() + w * 0.22, c.y + h * 0.16),
                pos2(r.left(), c.y + h * 0.16),
            ];
            p.add(Shape::convex_polygon(cone, color, Stroke::NONE));
            if matches!(icon, Icon::Muted) {
                let x = r.left() + w * 0.78;
                p.line_segment([pos2(x - w * 0.18, c.y - h * 0.18), pos2(x + w * 0.18, c.y + h * 0.18)], s);
                p.line_segment([pos2(x - w * 0.18, c.y + h * 0.18), pos2(x + w * 0.18, c.y - h * 0.18)], s);
            } else {
                for k in [0.28_f32, 0.46] {
                    let arc: Vec<Pos2> = (0..=8)
                        .map(|i| {
                            let a = -0.8 + i as f32 / 8.0 * 1.6;
                            pos2(r.left() + w * 0.5 + a.cos() * w * k, c.y + a.sin() * h * k)
                        })
                        .collect();
                    p.add(Shape::line(arc, Stroke::new(1.4_f32, color)));
                }
            }
        },
        Icon::Pin => {
            let head = Rect::from_center_size(pos2(c.x, r.top() + h * 0.25), vec2(w * 0.5, h * 0.42));
            p.rect_filled(head, 1.5, color);
            p.line_segment([pos2(r.left() + w * 0.12, c.y + h * 0.05), pos2(r.right() - w * 0.12, c.y + h * 0.05)], s);
            p.line_segment([pos2(c.x, c.y + h * 0.05), pos2(c.x, r.bottom())], s);
        },
        Icon::Shield => {
            let pts = vec![
                pos2(c.x, r.top()),
                pos2(r.right(), r.top() + h * 0.18),
                pos2(r.right() - w * 0.08, c.y + h * 0.15),
                pos2(c.x, r.bottom()),
                pos2(r.left() + w * 0.08, c.y + h * 0.15),
                pos2(r.left(), r.top() + h * 0.18),
            ];
            p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        },
        Icon::Reader => {
            for (i, k) in [1.0_f32, 0.85, 1.0, 0.6].iter().enumerate() {
                let y = r.top() + h * (0.1 + i as f32 * 0.27);
                p.line_segment([pos2(r.left(), y), pos2(r.left() + w * k, y)], s);
            }
        },
        Icon::Minimize => {
            p.line_segment([pos2(r.left(), c.y), pos2(r.right(), c.y)], Stroke::new(1.0_f32, color));
        },
        Icon::Maximize => {
            p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
        },
        Icon::Restore => {
            let back = Rect::from_min_max(pos2(r.left() + w * 0.25, r.top()), pos2(r.right(), r.bottom() - h * 0.25));
            let front = Rect::from_min_max(pos2(r.left(), r.top() + h * 0.25), pos2(r.right() - w * 0.25, r.bottom()));
            let thin = Stroke::new(1.0_f32, color);
            p.line_segment([back.left_top(), back.right_top()], thin);
            p.line_segment([back.right_top(), back.right_bottom()], thin);
            p.line_segment([back.left_top(), pos2(back.left(), front.top())], thin);
            p.line_segment([pos2(front.right(), back.bottom()), back.right_bottom()], thin);
            p.rect_stroke(front, 0.0, thin, egui::StrokeKind::Inside);
        },
        Icon::Code => {
            p.line_segment([pos2(r.left() + w * 0.3, r.top() + h * 0.15), pos2(r.left(), c.y)], s);
            p.line_segment([pos2(r.left(), c.y), pos2(r.left() + w * 0.3, r.bottom() - h * 0.15)], s);
            p.line_segment([pos2(r.right() - w * 0.3, r.top() + h * 0.15), pos2(r.right(), c.y)], s);
            p.line_segment([pos2(r.right(), c.y), pos2(r.right() - w * 0.3, r.bottom() - h * 0.15)], s);
            p.line_segment([pos2(c.x + w * 0.1, r.top()), pos2(c.x - w * 0.1, r.bottom())], s);
        },
        Icon::Trash => {
            // Lid, handle, and a can with two ribs.
            p.line_segment([pos2(r.left(), r.top() + h * 0.2), pos2(r.right(), r.top() + h * 0.2)], s);
            p.line_segment([pos2(c.x - w * 0.15, r.top() + h * 0.2), pos2(c.x - w * 0.15, r.top())], s);
            p.line_segment([pos2(c.x - w * 0.15, r.top()), pos2(c.x + w * 0.15, r.top())], s);
            p.line_segment([pos2(c.x + w * 0.15, r.top()), pos2(c.x + w * 0.15, r.top() + h * 0.2)], s);
            let can = [
                pos2(r.left() + w * 0.12, r.top() + h * 0.2),
                pos2(r.left() + w * 0.2, r.bottom()),
                pos2(r.right() - w * 0.2, r.bottom()),
                pos2(r.right() - w * 0.12, r.top() + h * 0.2),
            ];
            p.line_segment([can[0], can[1]], s);
            p.line_segment([can[1], can[2]], s);
            p.line_segment([can[2], can[3]], s);
            p.line_segment([pos2(c.x - w * 0.12, r.top() + h * 0.38), pos2(c.x - w * 0.12, r.bottom() - h * 0.15)], s);
            p.line_segment([pos2(c.x + w * 0.12, r.top() + h * 0.38), pos2(c.x + w * 0.12, r.bottom() - h * 0.15)], s);
        },
        Icon::Edit => {
            // A pencil, tip at the bottom left.
            let tip = pos2(r.left(), r.bottom());
            let a = pos2(r.left() + w * 0.22, r.bottom());
            let b = pos2(r.left(), r.bottom() - h * 0.22);
            let a2 = pos2(r.right(), r.top() + h * 0.22);
            let b2 = pos2(r.right() - w * 0.22, r.top());
            p.line_segment([tip, a], s);
            p.line_segment([tip, b], s);
            p.line_segment([a, a2], s);
            p.line_segment([b, b2], s);
            p.line_segment([a2, b2], s);
        },
    }
}
