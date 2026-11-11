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

/// A non-interactive icon of `size`, for use inside rows and labels.
pub fn icon(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_icon(ui, icon, rect.shrink(size * 0.15), color);
    response
}

fn paint_icon(ui: &Ui, icon: Icon, r: Rect, color: Color32) {
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
    }
}
