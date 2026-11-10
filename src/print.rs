// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Printing.
//!
//! Servo can't print yet, so Ferrum captures the page itself: it scrolls
//! through the page one screen at a time, takes a screenshot of each, stitches
//! them into one tall image, cuts that into A4 pages and writes a PDF (JPEG
//! pages, written by hand below — no PDF library needed). The PDF can then be
//! sent to a printer with the system's print command, or saved.

use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use image::{DynamicImage, GenericImageView, RgbaImage};
use servo::{JSValue, WebView};
use winit::window::Window;

/// Longest page Ferrum will capture, in screens.
const MAX_SCREENS: usize = 40;
/// A4 in PDF points, and the page margin.
const PAGE_W: f64 = 595.0;
const PAGE_H: f64 = 842.0;
const MARGIN: f64 = 28.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    Measuring,
    Capturing,
    Ready,
    Failed(String),
}

pub struct PrintJob {
    webview: WebView,
    pub title: String,
    pub stage: Stage,
    busy: bool,
    viewport_h: f64,
    original_scroll: f64,
    targets: Vec<f64>,
    /// (actual scroll position, screenshot) for each captured screen.
    shots: Vec<(f64, RgbaImage)>,
    pub pdf: Option<PathBuf>,
    pub pages: usize,
    pub printers: Vec<String>,
    pub printer: usize,
    pub copies: u32,
}

pub type Job = Rc<RefCell<PrintJob>>;

impl PrintJob {
    pub fn new(webview: WebView, title: String) -> Job {
        Rc::new(RefCell::new(Self {
            webview,
            title,
            stage: Stage::Measuring,
            busy: false,
            viewport_h: 0.0,
            original_scroll: 0.0,
            targets: Vec::new(),
            shots: Vec::new(),
            pdf: None,
            pages: 0,
            printers: Vec::new(),
            printer: 0,
            copies: 1,
        }))
    }

    pub fn progress(&self) -> (usize, usize) {
        (self.shots.len(), self.targets.len().max(1))
    }
}

fn numbers(value: Result<JSValue, servo::JavaScriptEvaluationError>) -> Option<Vec<f64>> {
    match value {
        Ok(JSValue::Array(items)) => items
            .into_iter()
            .map(|v| match v {
                JSValue::Number(n) => Some(n),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

/// Move the job one step forward. Called whenever the main thread wakes up;
/// each step starts one asynchronous request and returns.
pub fn advance(job: &Job, window: &Rc<Window>) {
    let (stage, busy) = {
        let j = job.borrow();
        (j.stage.clone(), j.busy)
    };
    if busy {
        return;
    }
    match stage {
        Stage::Measuring => {
            let webview = {
                let mut j = job.borrow_mut();
                j.busy = true;
                j.webview.clone()
            };
            let (job2, window2) = (job.clone(), window.clone());
            webview.evaluate_javascript(
                "[Math.max(document.documentElement.scrollHeight, document.body ? document.body.scrollHeight : 0), window.innerHeight, window.scrollY]",
                move |value| {
                    let mut j = job2.borrow_mut();
                    j.busy = false;
                    match numbers(value).filter(|v| v.len() == 3 && v[1] > 0.0) {
                        Some(v) => {
                            let (doc_h, view_h) = (v[0].max(v[1]), v[1]);
                            j.viewport_h = view_h;
                            j.original_scroll = v[2];
                            let mut y = 0.0;
                            while y < doc_h && j.targets.len() < MAX_SCREENS {
                                j.targets.push(y);
                                y += view_h;
                            }
                            j.stage = Stage::Capturing;
                        },
                        None => j.stage = Stage::Failed("Couldn't measure the page".into()),
                    }
                    window2.request_redraw();
                },
            );
        },
        Stage::Capturing => {
            let (webview, next) = {
                let mut j = job.borrow_mut();
                let i = j.shots.len();
                if i >= j.targets.len() {
                    drop(j);
                    finish(job);
                    window.request_redraw();
                    return;
                }
                j.busy = true;
                (j.webview.clone(), j.targets[i])
            };
            let (job2, window2, webview2) = (job.clone(), window.clone(), webview.clone());
            webview.evaluate_javascript(format!("window.scrollTo(0, {next}); [window.scrollY]"), move |value| {
                let actual = numbers(value).and_then(|v| v.first().copied()).unwrap_or(next);
                let (job3, window3) = (job2.clone(), window2.clone());
                // Servo waits until the scrolled page is fully rendered before capturing.
                webview2.take_screenshot(None, move |shot| {
                    let mut j = job3.borrow_mut();
                    j.busy = false;
                    match shot {
                        Ok(image) => j.shots.push((actual, image)),
                        Err(e) => j.stage = Stage::Failed(format!("Screenshot failed: {e:?}")),
                    }
                    window3.request_redraw();
                });
            });
        },
        Stage::Ready | Stage::Failed(_) => {},
    }
}

/// Put the scroll position back, build the PDF and look up printers.
fn finish(job: &Job) {
    let mut j = job.borrow_mut();
    let restore = j.original_scroll;
    j.webview.evaluate_javascript(format!("window.scrollTo(0, {restore})"), |_| {});

    let shots = std::mem::take(&mut j.shots);
    let tall = match stitch(&shots, j.viewport_h) {
        Some(t) => t,
        None => {
            j.stage = Stage::Failed("Nothing to print".into());
            return;
        },
    };
    let path = std::env::temp_dir().join(format!("ferrum-print-{}.pdf", std::process::id()));
    match write_pdf(&path, &tall) {
        Ok(pages) => {
            j.pages = pages;
            j.pdf = Some(path);
            j.printers = list_printers();
            j.stage = Stage::Ready;
        },
        Err(e) => j.stage = Stage::Failed(format!("Couldn't create the PDF: {e}")),
    }
}

/// Join screenshots into one tall image, dropping the overlap of the last
/// screen (which usually scrolls less than a full screen).
fn stitch(shots: &[(f64, RgbaImage)], viewport_h: f64) -> Option<RgbaImage> {
    let first = &shots.first()?.1;
    let width = first.width();
    let dpr = first.height() as f64 / viewport_h.max(1.0);
    let mut parts: Vec<(&RgbaImage, u32)> = Vec::new();
    let mut covered_until = f64::MIN;
    for (scroll, img) in shots {
        let skip_css = (covered_until - scroll).max(0.0);
        let skip = ((skip_css * dpr).round() as u32).min(img.height());
        if skip < img.height() {
            parts.push((img, skip));
        }
        covered_until = scroll + viewport_h;
    }
    let height: u32 = parts.iter().map(|(img, skip)| img.height() - skip).sum();
    let mut out = RgbaImage::new(width, height.max(1));
    let mut y = 0;
    for (img, skip) in parts {
        let view = img.view(0, skip, img.width().min(width), img.height() - skip);
        image::imageops::replace(&mut out, &*view, 0, y as i64);
        y += img.height() - skip;
    }
    Some(out)
}

/// Cut `image` into A4 pages and write them as a PDF. Returns the page count.
fn write_pdf(path: &Path, image: &RgbaImage) -> std::io::Result<usize> {
    let content_w = PAGE_W - 2.0 * MARGIN;
    let content_h = PAGE_H - 2.0 * MARGIN;
    let px_per_page = ((image.width() as f64) * content_h / content_w).floor().max(1.0) as u32;
    let rgb = DynamicImage::ImageRgba8(image.clone()).to_rgb8();

    let mut jpegs = Vec::new();
    let mut top = 0;
    while top < rgb.height() {
        let h = px_per_page.min(rgb.height() - top);
        let slice = image::imageops::crop_imm(&rgb, 0, top, rgb.width(), h).to_image();
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 88)
            .encode_image(&slice)
            .map_err(std::io::Error::other)?;
        jpegs.push((bytes, slice.width(), slice.height()));
        top += h;
    }

    // Objects: 1 catalog, 2 page tree, then 3 per page (page, contents, image).
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    let obj = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
        offsets.push(out.len());
        let n = offsets.len();
        out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    obj(&mut out, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = (0..jpegs.len()).map(|i| format!("{} 0 R", 3 + i * 3)).collect();
    obj(&mut out, &mut offsets, format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), jpegs.len()).as_bytes());

    for (i, (jpeg, w, h)) in jpegs.iter().enumerate() {
        let page = 3 + i * 3;
        let draw_w = content_w;
        let draw_h = *h as f64 * content_w / *w as f64;
        let y = PAGE_H - MARGIN - draw_h;
        obj(
            &mut out,
            &mut offsets,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] /Contents {} 0 R /Resources << /XObject << /Im0 {} 0 R >> >> >>",
                page + 1,
                page + 2
            )
            .as_bytes(),
        );
        let content = format!("q {draw_w:.2} 0 0 {draw_h:.2} {MARGIN:.2} {y:.2} cm /Im0 Do Q");
        obj(&mut out, &mut offsets, format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).as_bytes());
        let mut body = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            jpeg.len()
        )
        .into_bytes();
        body.extend_from_slice(jpeg);
        body.extend_from_slice(b"\nendstream");
        obj(&mut out, &mut offsets, &body);
    }

    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for off in &offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1).as_bytes());

    std::fs::File::create(path)?.write_all(&out)?;
    Ok(jpegs.len())
}

/// Printer names, default printer first.
pub fn list_printers() -> Vec<String> {
    let run = |cmd: &str, args: &[&str]| -> String {
        Command::new(cmd)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    let mut printers: Vec<String>;
    if cfg!(windows) {
        printers = run("powershell", &["-NoProfile", "-Command", "Get-Printer | Sort-Object -Property Default -Descending | ForEach-Object Name"])
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
    } else {
        // CUPS, used by both macOS and Linux.
        printers = run("lpstat", &["-e"]).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        let default = run("lpstat", &["-d"]);
        if let Some(name) = default.rsplit(':').next().map(str::trim).filter(|n| !n.is_empty()) {
            if let Some(i) = printers.iter().position(|p| p == name) {
                let p = printers.remove(i);
                printers.insert(0, p);
            }
        }
    }
    printers
}

/// Send the PDF to `printer`.
pub fn send_to_printer(pdf: &Path, printer: &str, copies: u32) -> Result<(), String> {
    let copies = copies.clamp(1, 99);
    let status = if cfg!(windows) {
        let script = format!(
            "1..{copies} | ForEach-Object {{ Start-Process -FilePath '{}' -Verb PrintTo -ArgumentList '\"{}\"' -WindowStyle Hidden }}",
            pdf.display().to_string().replace('\'', "''"),
            printer.replace('\'', "''")
        );
        Command::new("powershell").args(["-NoProfile", "-Command", &script]).status()
    } else {
        Command::new("lp").arg("-d").arg(printer).arg("-n").arg(copies.to_string()).arg(pdf).status()
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("the print command failed ({s})")),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_is_well_formed() {
        let img = RgbaImage::from_pixel(400, 1500, image::Rgba([200, 100, 50, 255]));
        let path = std::env::temp_dir().join("ferrum-test.pdf");
        let pages = write_pdf(&path, &img).unwrap();
        // 400 px wide → ~617 px per A4 page → 3 pages.
        assert_eq!(pages, 3);
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn stitching_drops_overlap() {
        let a = RgbaImage::new(10, 100);
        let b = RgbaImage::new(10, 100);
        // Second screen only scrolled 40px: 60px of it overlap the first.
        let tall = stitch(&[(0.0, a), (40.0, b)], 100.0).unwrap();
        assert_eq!(tall.height(), 140);
    }
}
