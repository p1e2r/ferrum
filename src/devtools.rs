// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! The page-source viewer (Ctrl+U). Developer tools themselves are the web
//! engine's own (F12).

use egui::text::LayoutJob;
use egui::{Color32, FontId, RichText, TextFormat, Ui};

/// A page's HTML source, split into lines for fast drawing.
pub struct Source {
    pub url: String,
    pub lines: Vec<String>,
    pub wrap: bool,
}

impl Source {
    pub fn new(url: String, html: &str) -> Self {
        Self { url, lines: html.lines().map(|l| l.replace('\t', "    ")).collect(), wrap: false }
    }
}

/// Colour one line of HTML: tags, attribute names, attribute values, comments.
#[cfg(test)]
fn highlight(line: &str, dark: bool, base: Color32) -> LayoutJob {
    highlight_numbered(line, None, dark, base)
}

/// `highlight`, with a grey line number in front.
fn highlight_numbered(line: &str, number: Option<(usize, usize, Color32)>, dark: bool, base: Color32) -> LayoutJob {
    let (tag, attr, value, comment) = if dark {
        (Color32::from_rgb(120, 170, 255), Color32::from_rgb(170, 210, 120), Color32::from_rgb(240, 170, 110), Color32::from_rgb(120, 125, 130))
    } else {
        (Color32::from_rgb(20, 80, 190), Color32::from_rgb(90, 120, 20), Color32::from_rgb(170, 70, 0), Color32::from_rgb(120, 125, 130))
    };
    let font = FontId::monospace(12.5);
    let mut job = LayoutJob::default();
    let fmt = |c: Color32| TextFormat { font_id: font.clone(), color: c, ..Default::default() };
    if let Some((n, digits, color)) = number {
        job.append(&format!("{n:>digits$}  "), 0.0, fmt(color));
    }
    let mut in_tag = false;
    let mut in_comment = false;
    let mut quote: Option<char> = None;
    let mut buf = String::new();
    let mut buf_color = base;
    let push = |job: &mut LayoutJob, buf: &mut String, color: Color32| {
        if !buf.is_empty() {
            job.append(buf, 0.0, fmt(color));
            buf.clear();
        }
    };
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let color = if in_comment {
            comment
        } else if quote.is_some() {
            value
        } else if in_tag {
            if c == '"' || c == '\'' { value } else if c == '<' || c == '>' || c == '/' { tag } else { buf_color }
        } else if c == '<' {
            if chars[i..].starts_with(&['<', '!', '-', '-']) { comment } else { tag }
        } else {
            base
        };
        if color != buf_color {
            push(&mut job, &mut buf, buf_color);
            buf_color = color;
        }
        buf.push(c);
        if in_comment {
            if c == '>' && i >= 2 && chars[i - 1] == '-' && chars[i - 2] == '-' {
                in_comment = false;
            }
        } else if let Some(q) = quote {
            if c == q {
                quote = None;
                push(&mut job, &mut buf, buf_color);
                buf_color = attr;
            }
        } else if in_tag {
            match c {
                '"' | '\'' => quote = Some(c),
                '>' => {
                    in_tag = false;
                    push(&mut job, &mut buf, buf_color);
                    buf_color = base;
                },
                ' ' => {
                    push(&mut job, &mut buf, buf_color);
                    buf_color = attr;
                },
                _ => {},
            }
        } else if c == '<' {
            if chars[i..].starts_with(&['<', '!', '-', '-']) {
                in_comment = true;
            } else {
                in_tag = true;
                // The tag name itself.
                buf_color = tag;
            }
        }
        i += 1;
    }
    push(&mut job, &mut buf, buf_color);
    job
}

/// Draw a page-source tab.
pub fn draw_source(ui: &mut Ui, rect: egui::Rect, source: &mut Source) {
    ui.painter().rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(10.0, 6.0))), |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Source of {}", source.url)).strong());
            ui.label(RichText::new(format!("{} lines", source.lines.len())).weak());
            ui.checkbox(&mut source.wrap, "Wrap long lines");
            if ui.small_button("Copy all").clicked() {
                ui.ctx().copy_text(source.lines.join("\n"));
            }
        });
        ui.separator();
        let dark = ui.visuals().dark_mode;
        let base = ui.visuals().text_color();
        let weak = ui.visuals().weak_text_color();
        let digits = source.lines.len().max(1).to_string().len();
        ui.spacing_mut().item_spacing.y = 0.0;
        let row_height = ui.fonts_mut(|f| f.row_height(&FontId::monospace(12.5)));
        let scroll = if source.wrap { egui::ScrollArea::vertical() } else { egui::ScrollArea::both() };
        if source.wrap {
            // Wrapped lines have different heights, so draw them all (fine for normal pages).
            scroll.auto_shrink(false).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (n, line) in source.lines.iter().enumerate() {
                    let mut job = highlight_numbered(line, Some((n + 1, digits, weak)), dark, base);
                    job.wrap.max_width = ui.available_width();
                    ui.label(job);
                }
            });
        } else {
            // Only the lines on screen are laid out, so even huge pages scroll smoothly.
            scroll.auto_shrink(false).show_rows(ui, row_height, source.lines.len(), |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for n in range {
                    ui.add(egui::Label::new(highlight_numbered(&source.lines[n], Some((n + 1, digits, weak)), dark, base)).extend());
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_keeps_text() {
        let line = r#"<a href="x">link</a> <!-- c -->"#;
        let job = highlight(line, false, Color32::BLACK);
        assert_eq!(job.text, line);
    }
}
