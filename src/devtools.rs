// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Developer tools: a JavaScript console for the active tab (messages the page
//! logged, plus a prompt that runs code in the page) and a page-source viewer.

use std::collections::VecDeque;

use egui::text::LayoutJob;
use egui::{Color32, FontId, RichText, TextFormat, Ui};
use servo::{ConsoleLogLevel, JSValue};

const MAX_MESSAGES: usize = 1000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Log,
    Info,
    Warn,
    Error,
    Debug,
    /// Code typed at the prompt.
    Input,
    /// The result of code typed at the prompt.
    Result,
}

impl From<ConsoleLogLevel> for Level {
    fn from(level: ConsoleLogLevel) -> Self {
        match level {
            ConsoleLogLevel::Warn => Level::Warn,
            ConsoleLogLevel::Error => Level::Error,
            ConsoleLogLevel::Info => Level::Info,
            ConsoleLogLevel::Debug | ConsoleLogLevel::Trace => Level::Debug,
            _ => Level::Log,
        }
    }
}

/// A tab's console messages.
#[derive(Default)]
pub struct Console {
    pub messages: VecDeque<(Level, String)>,
    pub errors: usize,
}

impl Console {
    pub fn push(&mut self, level: Level, text: String) {
        if level == Level::Error {
            self.errors += 1;
        }
        self.messages.push_back((level, text));
        while self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.errors = 0;
    }
}

/// The console panel's own state (shared by all tabs).
#[derive(Default)]
pub struct DevTools {
    pub open: bool,
    pub input: String,
    pub filter: String,
    pub history: Vec<String>,
    pub history_pos: Option<usize>,
    pub focus: bool,
}

/// Render a JavaScript value the way a console would.
pub fn format_value(value: &JSValue) -> String {
    match value {
        JSValue::Undefined => "undefined".into(),
        JSValue::Null => "null".into(),
        JSValue::Boolean(b) => b.to_string(),
        JSValue::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", *n as i64) } else { n.to_string() }
        },
        JSValue::String(s) => format!("{s:?}"),
        JSValue::Element(_) => "<element>".into(),
        JSValue::ShadowRoot(_) => "#shadow-root".into(),
        JSValue::Frame(_) => "[frame]".into(),
        JSValue::Window(_) => "[window]".into(),
        JSValue::Array(items) => {
            let shown: Vec<String> = items.iter().take(50).map(format_value).collect();
            let more = if items.len() > 50 { format!(", … {} more", items.len() - 50) } else { String::new() };
            format!("[{}{more}]", shown.join(", "))
        },
        JSValue::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let shown: Vec<String> = keys.iter().take(30).map(|k| format!("{k}: {}", format_value(&map[*k]))).collect();
            let more = if map.len() > 30 { ", …" } else { "" };
            format!("{{{}{more}}}", shown.join(", "))
        },
    }
}

fn level_style(level: Level, dark: bool) -> (Color32, Color32, &'static str) {
    let (fg, bg) = match (level, dark) {
        (Level::Error, true) => (Color32::from_rgb(255, 128, 128), Color32::from_rgb(70, 30, 32)),
        (Level::Error, false) => (Color32::from_rgb(180, 20, 20), Color32::from_rgb(255, 235, 235)),
        (Level::Warn, true) => (Color32::from_rgb(240, 200, 110), Color32::from_rgb(60, 50, 20)),
        (Level::Warn, false) => (Color32::from_rgb(130, 90, 0), Color32::from_rgb(255, 248, 220)),
        (Level::Input, _) => (Color32::from_rgb(90, 140, 230), Color32::TRANSPARENT),
        (Level::Debug, _) => (Color32::GRAY, Color32::TRANSPARENT),
        _ => (Color32::PLACEHOLDER, Color32::TRANSPARENT),
    };
    let prefix = match level {
        Level::Input => "› ",
        Level::Result => "‹ ",
        _ => "",
    };
    (fg, bg, prefix)
}

pub enum ConsoleAction {
    None,
    Run(String),
    Clear,
    Close,
}

/// Draw the console panel's contents. Returns what to do.
pub fn draw_console(ui: &mut Ui, tools: &mut DevTools, console: Option<&Console>) -> ConsoleAction {
    let mut action = ConsoleAction::None;
    let dark = ui.visuals().dark_mode;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Console").strong());
        if let Some(c) = console {
            if c.errors > 0 {
                let errors = if c.errors == 1 { "1 error".to_string() } else { format!("{} errors", c.errors) };
                ui.label(RichText::new(errors).color(Color32::from_rgb(220, 70, 70)).size(12.0));
            }
        }
        ui.add(egui::TextEdit::singleline(&mut tools.filter).hint_text("Filter").desired_width(160.0));
        if ui.small_button("Clear").clicked() {
            action = ConsoleAction::Clear;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Close").on_hover_text("F12").clicked() {
                action = ConsoleAction::Close;
            }
        });
    });
    ui.separator();

    let input_height = 30.0;
    let filter = tools.filter.to_lowercase();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .max_height((ui.available_height() - input_height).max(20.0))
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            match console {
                Some(c) => {
                    for (level, text) in c.messages.iter().filter(|(_, t)| filter.is_empty() || t.to_lowercase().contains(&filter)) {
                        let (fg, bg, prefix) = level_style(*level, dark);
                        egui::Frame::new().fill(bg).inner_margin(egui::Margin::symmetric(4, 1)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let color = if fg == Color32::PLACEHOLDER { ui.visuals().text_color() } else { fg };
                            ui.add(egui::Label::new(RichText::new(format!("{prefix}{text}")).font(FontId::monospace(12.0)).color(color)).wrap());
                        });
                    }
                },
                None => {
                    ui.label(RichText::new("Open a web page to use the console.").weak());
                },
            }
        });

    ui.horizontal(|ui| {
        ui.label(RichText::new("›").monospace().strong());
        let id = egui::Id::new("console_input");
        let resp = ui.add(
            egui::TextEdit::singleline(&mut tools.input)
                .id(id)
                .font(FontId::monospace(13.0))
                .hint_text("Run JavaScript in this page (Enter to run, Up/Down for history)")
                .desired_width(f32::INFINITY),
        );
        if tools.focus {
            resp.request_focus();
            tools.focus = false;
        }
        if resp.has_focus() {
            let (up, down) = ui.input(|i| (i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown)));
            if up && !tools.history.is_empty() {
                let pos = tools.history_pos.map_or(tools.history.len() - 1, |p| p.saturating_sub(1));
                tools.history_pos = Some(pos);
                tools.input = tools.history[pos].clone();
            }
            if down {
                if let Some(p) = tools.history_pos {
                    if p + 1 < tools.history.len() {
                        tools.history_pos = Some(p + 1);
                        tools.input = tools.history[p + 1].clone();
                    } else {
                        tools.history_pos = None;
                        tools.input.clear();
                    }
                }
            }
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let code = std::mem::take(&mut tools.input);
            if !code.trim().is_empty() {
                tools.history.push(code.clone());
                tools.history_pos = None;
                action = ConsoleAction::Run(code);
            }
            tools.focus = true;
        }
    });
    action
}

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
    use std::collections::HashMap;

    #[test]
    fn formats_values() {
        assert_eq!(format_value(&JSValue::Number(3.0)), "3");
        assert_eq!(format_value(&JSValue::Number(0.5)), "0.5");
        assert_eq!(format_value(&JSValue::String("a".into())), "\"a\"");
        assert_eq!(format_value(&JSValue::Array(vec![JSValue::Null, JSValue::Boolean(true)])), "[null, true]");
        let mut m = HashMap::new();
        m.insert("b".to_string(), JSValue::Number(2.0));
        m.insert("a".to_string(), JSValue::Undefined);
        assert_eq!(format_value(&JSValue::Object(m)), "{a: undefined, b: 2}");
    }

    #[test]
    fn highlight_keeps_text() {
        let line = r#"<a href="x">link</a> <!-- c -->"#;
        let job = highlight(line, false, Color32::BLACK);
        assert_eq!(job.text, line);
    }

    #[test]
    fn console_is_capped() {
        let mut c = Console::default();
        for i in 0..(MAX_MESSAGES + 10) {
            c.push(Level::Log, i.to_string());
        }
        assert_eq!(c.messages.len(), MAX_MESSAGES);
        c.push(Level::Error, "x".into());
        assert_eq!(c.errors, 1);
    }
}
