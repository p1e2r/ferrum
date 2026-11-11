// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Reader mode: the article's text, without ads, menus or sidebars, drawn
//! natively by Ferrum in a clean, readable column.
//!
//! A script finds the page's main content (an `<article>`, `<main>`, or the
//! element with the most paragraph text) and returns its headings,
//! paragraphs, quotes, list items and code blocks as JSON.

use egui::{Color32, FontId, Frame, Margin, RichText, Ui};
use serde::Deserialize;

use crate::profile::ReaderTheme;

pub const SCRIPT: &str = r#"(function () {
  const textLen = (el) => (el.innerText || "").trim().length;
  let root = document.querySelector("article") || document.querySelector("[role=main]") || document.querySelector("main");
  if (!root || textLen(root) < 400) {
    let best = null, bestScore = 0;
    for (const el of document.querySelectorAll("div, section, td")) {
      let score = 0;
      for (const p of el.children) {
        if (p.tagName === "P") score += textLen(p);
      }
      if (score > bestScore) { bestScore = score; best = el; }
    }
    if (best && bestScore > 200) root = best;
  }
  if (!root) root = document.body;
  const skip = "nav, aside, footer, header, form, script, style, noscript, button, figure figcaption, [aria-hidden=true], .ad, .ads, .advert, .share, .social, .related, .comments";
  const blocks = [];
  const seen = new Set();
  const walk = (el) => {
    for (const node of el.children) {
      if (node.matches(skip)) continue;
      const tag = node.tagName;
      let kind = null;
      if (/^H[1-6]$/.test(tag)) kind = tag === "H1" || tag === "H2" ? "h1" : "h2";
      else if (tag === "P") kind = "p";
      else if (tag === "BLOCKQUOTE") kind = "quote";
      else if (tag === "LI") kind = "li";
      else if (tag === "PRE") kind = "code";
      if (kind) {
        const text = kind === "code" ? node.innerText : (node.innerText || "").replace(/\s+/g, " ").trim();
        if (text && !seen.has(text) && (kind !== "p" || text.length > 1)) {
          seen.add(text);
          blocks.push([kind, text]);
        }
        if (kind !== "li") continue;
      }
      walk(node);
    }
  };
  walk(root);
  const meta = (n) => { const m = document.querySelector(`meta[name="${n}"], meta[property="${n}"]`); return m ? m.content : ""; };
  const h1 = document.querySelector("h1");
  const title = meta("og:title") || (h1 && h1.innerText.trim()) || document.title;
  if (blocks.length && blocks[0][1] === title) blocks.shift();
  return JSON.stringify({ title, byline: meta("author") || meta("article:author"), site: meta("og:site_name") || location.hostname, blocks });
})()"#;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ReaderDoc {
    pub title: String,
    pub byline: String,
    pub site: String,
    pub blocks: Vec<(String, String)>,
}

impl ReaderDoc {
    pub fn parse(json: &str) -> Option<Self> {
        serde_json::from_str::<ReaderDoc>(json).ok().filter(|d| !d.blocks.is_empty())
    }

    pub fn word_count(&self) -> usize {
        self.blocks.iter().map(|(_, t)| t.split_whitespace().count()).sum()
    }
}

/// (background, text, weak text) for a reader theme.
fn colors(theme: ReaderTheme, dark_ui: bool) -> (Color32, Color32, Color32) {
    match theme {
        ReaderTheme::Light => (Color32::from_rgb(252, 252, 250), Color32::from_rgb(28, 28, 30), Color32::from_rgb(110, 110, 115)),
        ReaderTheme::Sepia => (Color32::from_rgb(246, 238, 220), Color32::from_rgb(60, 45, 30), Color32::from_rgb(130, 110, 85)),
        ReaderTheme::Dark => (Color32::from_rgb(26, 27, 30), Color32::from_rgb(220, 220, 215), Color32::from_rgb(145, 145, 150)),
        ReaderTheme::Auto => colors(if dark_ui { ReaderTheme::Dark } else { ReaderTheme::Light }, dark_ui),
    }
}

pub struct ReaderControls {
    pub close: bool,
    pub changed: bool,
}

/// Draw the article. Returns what the user did with the reader toolbar.
pub fn draw(
    ui: &mut Ui,
    rect: egui::Rect,
    doc: &ReaderDoc,
    font_size: &mut f32,
    theme: &mut ReaderTheme,
    width: &mut f32,
) -> ReaderControls {
    let (bg, text, weak) = colors(*theme, ui.visuals().dark_mode);
    let mut out = ReaderControls { close: false, changed: false };
    ui.painter().rect_filled(rect, 0.0, bg);
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        // Toolbar
        Frame::new().inner_margin(Margin::symmetric(12, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                // Buttons in the reader's own colours, whatever the interface theme.
                let v = ui.visuals_mut();
                v.override_text_color = Some(text);
                v.widgets.inactive.weak_bg_fill = text.gamma_multiply(0.08);
                v.widgets.inactive.bg_fill = text.gamma_multiply(0.08);
                v.widgets.hovered.weak_bg_fill = text.gamma_multiply(0.15);
                v.widgets.hovered.bg_fill = text.gamma_multiply(0.15);
                v.widgets.active.weak_bg_fill = text.gamma_multiply(0.22);
                v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
                if ui.button("Exit reader").on_hover_text("Back to the page (Ctrl+Alt+R)").clicked() {
                    out.close = true;
                }
                ui.add_space(12.0);
                if ui.button("A−").on_hover_text("Smaller text").clicked() {
                    *font_size = (*font_size - 1.0).max(12.0);
                    out.changed = true;
                }
                if ui.button("A+").on_hover_text("Larger text").clicked() {
                    *font_size = (*font_size + 1.0).min(34.0);
                    out.changed = true;
                }
                ui.add_space(8.0);
                for (t, label) in [(ReaderTheme::Auto, "Auto"), (ReaderTheme::Light, "Light"), (ReaderTheme::Sepia, "Sepia"), (ReaderTheme::Dark, "Dark")] {
                    if ui.selectable_label(*theme == t, label).clicked() {
                        *theme = t;
                        out.changed = true;
                    }
                }
                ui.add_space(8.0);
                for (w, label) in [(600.0, "Narrow"), (720.0, "Medium"), (900.0, "Wide")] {
                    if ui.selectable_label((*width - w).abs() < 1.0, label).clicked() {
                        *width = w;
                        out.changed = true;
                    }
                }
                let minutes = (doc.word_count() as f32 / 230.0).ceil().max(1.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{minutes} min read")).color(weak));
                });
            });
        });

        let size = *font_size;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let column = width.min(ui.available_width() - 48.0).max(200.0);
            let side = ((ui.available_width() - column) / 2.0).max(24.0);
            ui.horizontal(|ui| {
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_width(column);
                    ui.spacing_mut().item_spacing.y = size * 0.8;
                    ui.add_space(24.0);
                    ui.label(RichText::new(&doc.site).size(size * 0.75).color(weak));
                    ui.label(RichText::new(&doc.title).size(size * 1.8).strong().color(text));
                    if !doc.byline.is_empty() {
                        ui.label(RichText::new(&doc.byline).size(size * 0.85).italics().color(weak));
                    }
                    ui.separator();
                    for (kind, body) in &doc.blocks {
                        match kind.as_str() {
                            "h1" => {
                                ui.add_space(size * 0.4);
                                ui.label(RichText::new(body).size(size * 1.35).strong().color(text));
                            },
                            "h2" => {
                                ui.label(RichText::new(body).size(size * 1.15).strong().color(text));
                            },
                            "quote" => {
                                Frame::new()
                                    .inner_margin(Margin { left: 16, right: 0, top: 2, bottom: 2 })
                                    .stroke(egui::Stroke::new(0.0_f32, weak))
                                    .show(ui, |ui| {
                                        let r = ui.label(RichText::new(body).size(size).italics().color(weak));
                                        let line = egui::Rect::from_min_max(r.rect.left_top() - egui::vec2(12.0, 0.0), r.rect.left_bottom() - egui::vec2(9.0, 0.0));
                                        ui.painter().rect_filled(line, 1.0, weak);
                                    });
                            },
                            "li" => {
                                ui.horizontal_top(|ui| {
                                    ui.label(RichText::new("•").size(size).color(text));
                                    ui.add(egui::Label::new(RichText::new(body).size(size).color(text)).wrap());
                                });
                            },
                            "code" => {
                                Frame::new().fill(text.gamma_multiply(0.07)).corner_radius(6.0).inner_margin(Margin::same(10)).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(RichText::new(body).font(FontId::monospace(size * 0.75)).color(text));
                                });
                            },
                            _ => {
                                ui.label(RichText::new(body).size(size).color(text).line_height(Some(size * 1.6)));
                            },
                        }
                    }
                    ui.add_space(60.0);
                });
            });
        });
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_article() {
        let doc = ReaderDoc::parse(r#"{"title":"T","byline":"","site":"s","blocks":[["p","one two three"],["h2","Head"]]}"#).unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.word_count(), 4);
        assert!(ReaderDoc::parse(r#"{"title":"T","blocks":[]}"#).is_none());
        assert!(ReaderDoc::parse("nonsense").is_none());
    }
}
