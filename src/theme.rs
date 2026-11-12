// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Colour themes for the interface.
//!
//! "Ferrum" is the built-in look. Every other theme is a complete palette
//! (backgrounds, surfaces, text) with a suggested accent colour, in a light
//! and a dark version, so the light/dark setting works with every theme.
//! Each theme also has a "native" mode: the version it was designed as.

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
    /// The look of Google's account pages: white and light grey, Google blue, pills.
    Google,
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
    pub const ALL: [ThemePreset; 11] = [
        ThemePreset::Ferrum,
        ThemePreset::Midnight,
        ThemePreset::Graphite,
        ThemePreset::Forest,
        ThemePreset::Arctic,
        ThemePreset::Plum,
        ThemePreset::Paper,
        ThemePreset::Sky,
        ThemePreset::Rose,
        ThemePreset::Google,
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
            ThemePreset::Google => "Google",
            ThemePreset::HighContrast => "High contrast",
        }
    }

    /// Whether the theme was designed as a dark one.
    pub fn native_dark(self) -> bool {
        self.native().is_some_and(|p| p.dark)
    }

    /// The theme's palette in light or dark mode; `None` for Ferrum.
    pub fn palette_for(self, dark: bool) -> Option<Palette> {
        let native = self.native()?;
        if native.dark == dark {
            return Some(native);
        }
        let p = |panel, surface, text, accent| Some(Palette { dark, panel, surface, text, accent });
        match self {
            ThemePreset::Ferrum => None,
            // Light versions of the dark themes…
            ThemePreset::Midnight => p(rgb(226, 232, 245), rgb(247, 249, 253), rgb(22, 30, 48), [52, 100, 230]),
            ThemePreset::Graphite => p(rgb(232, 232, 234), rgb(250, 250, 250), rgb(30, 30, 32), [224, 128, 0]),
            ThemePreset::Forest => p(rgb(222, 236, 226), rgb(246, 251, 247), rgb(22, 44, 30), [36, 145, 82]),
            ThemePreset::Arctic => p(rgb(229, 233, 240), rgb(243, 245, 249), rgb(46, 52, 64), [94, 129, 172]),
            ThemePreset::Plum => p(rgb(238, 228, 244), rgb(250, 246, 252), rgb(48, 30, 60), [150, 70, 180]),
            ThemePreset::HighContrast => p(rgb(255, 255, 255), rgb(255, 255, 255), rgb(0, 0, 0), [0, 70, 200]),
            // …and dark versions of the light ones.
            ThemePreset::Paper => p(rgb(44, 40, 34), rgb(33, 30, 25), rgb(238, 230, 214), [222, 130, 70]),
            ThemePreset::Sky => p(rgb(24, 34, 48), rgb(16, 24, 36), rgb(222, 234, 248), [90, 160, 255]),
            ThemePreset::Rose => p(rgb(46, 28, 34), rgb(34, 20, 25), rgb(248, 228, 233), [240, 110, 150]),
            // Google's dark surfaces and light blue.
            ThemePreset::Google => p(rgb(32, 33, 36), rgb(48, 49, 52), rgb(232, 234, 237), [138, 180, 248]),
        }
    }

    fn native(self) -> Option<Palette> {
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
            // Google's account pages: light grey around white, Google blue.
            ThemePreset::Google => p(false, rgb(241, 243, 244), rgb(255, 255, 255), rgb(32, 33, 36), [26, 115, 232]),
            ThemePreset::HighContrast => p(true, rgb(0, 0, 0), rgb(0, 0, 0), rgb(255, 255, 255), [255, 214, 0]),
        }
    }

    /// Colours for the preview card in settings: (panel, surface, text), in light or dark mode.
    pub fn preview(self, dark: bool) -> (Color32, Color32, Color32) {
        match self.palette_for(dark) {
            Some(p) => (p.panel, p.surface, p.text),
            None => {
                let v = if dark { Visuals::dark() } else { Visuals::light() };
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
