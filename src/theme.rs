// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Colour themes for the interface.
//!
//! "Ferrum" is the built-in look and follows the light/dark mode setting. Every
//! other theme is a complete palette (backgrounds, surfaces, text) with its own
//! light or dark mode and a suggested accent colour.

use egui::{Color32, Stroke, Visuals};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThemePreset {
    #[default]
    Ferrum,
    Midnight,
    Graphite,
    Forest,
    Arctic,
    Plum,
    Paper,
    Sky,
    Rose,
    HighContrast,
}

/// The colours a theme defines.
#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Tab strip, side panels, menus and dialogs.
    pub panel: Color32,
    /// Toolbar, text fields and cards.
    pub surface: Color32,
    pub text: Color32,
    pub accent: [u8; 3],
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

impl ThemePreset {
    pub const ALL: [ThemePreset; 10] = [
        ThemePreset::Ferrum,
        ThemePreset::Midnight,
        ThemePreset::Graphite,
        ThemePreset::Forest,
        ThemePreset::Arctic,
        ThemePreset::Plum,
        ThemePreset::Paper,
        ThemePreset::Sky,
        ThemePreset::Rose,
        ThemePreset::HighContrast,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ThemePreset::Ferrum => "Ferrum",
            ThemePreset::Midnight => "Midnight",
            ThemePreset::Graphite => "Graphite",
            ThemePreset::Forest => "Forest",
            ThemePreset::Arctic => "Arctic",
            ThemePreset::Plum => "Plum",
            ThemePreset::Paper => "Paper",
            ThemePreset::Sky => "Sky",
            ThemePreset::Rose => "Rose",
            ThemePreset::HighContrast => "High contrast",
        }
    }

    /// The theme's own palette; `None` for Ferrum, which uses the standard look.
    pub fn palette(self) -> Option<Palette> {
        let p = |dark, panel, surface, text, accent| Some(Palette { dark, panel, surface, text, accent });
        match self {
            ThemePreset::Ferrum => None,
            ThemePreset::Midnight => p(true, rgb(22, 28, 43), rgb(14, 19, 31), rgb(222, 229, 242), [92, 140, 255]),
            ThemePreset::Graphite => p(true, rgb(36, 36, 39), rgb(26, 26, 28), rgb(236, 236, 236), [255, 159, 10]),
            ThemePreset::Forest => p(true, rgb(24, 38, 30), rgb(16, 27, 21), rgb(224, 238, 229), [94, 200, 130]),
            ThemePreset::Arctic => p(true, rgb(48, 54, 67), rgb(40, 45, 56), rgb(229, 233, 240), [136, 192, 208]),
            ThemePreset::Plum => p(true, rgb(36, 27, 46), rgb(27, 20, 35), rgb(239, 227, 247), [200, 120, 220]),
            ThemePreset::Paper => p(false, rgb(239, 233, 220), rgb(252, 250, 244), rgb(45, 42, 36), [196, 98, 45]),
            ThemePreset::Sky => p(false, rgb(227, 236, 247), rgb(255, 255, 255), rgb(29, 42, 58), [37, 110, 220]),
            ThemePreset::Rose => p(false, rgb(245, 227, 231), rgb(255, 250, 251), rgb(58, 34, 40), [214, 70, 110]),
            ThemePreset::HighContrast => p(true, rgb(0, 0, 0), rgb(0, 0, 0), rgb(255, 255, 255), [255, 214, 0]),
        }
    }

    /// Colours for the preview card in settings: (panel, surface, text, dark).
    pub fn preview(self, ferrum_dark: bool) -> (Color32, Color32, Color32) {
        match self.palette() {
            Some(p) => (p.panel, p.surface, p.text),
            None => {
                let v = if ferrum_dark { Visuals::dark() } else { Visuals::light() };
                (v.panel_fill, v.extreme_bg_color, v.text_color())
            },
        }
    }
}

/// Blend `a` towards `b` by `t` (0 = a, 1 = b).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Paint `visuals` with a palette.
pub fn apply(v: &mut Visuals, p: &Palette, high_contrast: bool) {
    *v = if p.dark { Visuals::dark() } else { Visuals::light() };
    let (panel, surface, text) = (p.panel, p.surface, p.text);
    v.panel_fill = panel;
    v.window_fill = panel;
    v.extreme_bg_color = surface;
    v.faint_bg_color = mix(panel, surface, 0.5);
    v.code_bg_color = surface;
    v.override_text_color = Some(text);
    v.weak_text_color = Some(mix(text, panel, 0.42));

    let line = if high_contrast { text } else { mix(panel, text, 0.16) };
    v.window_stroke = Stroke::new(1.0_f32, line);
    let w = &mut v.widgets;
    for (state, t) in [
        (&mut w.noninteractive, 0.0),
        (&mut w.inactive, 0.09),
        (&mut w.hovered, 0.16),
        (&mut w.active, 0.24),
        (&mut w.open, 0.12),
    ] {
        let fill = mix(panel, text, t);
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.fg_stroke.color = text;
        state.bg_stroke.color = if t == 0.0 { line } else { mix(panel, text, t + 0.1) };
        if high_contrast {
            state.bg_stroke = Stroke::new(1.0_f32, text);
        }
    }
}
