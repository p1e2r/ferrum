// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! A small download manager. Each download runs on its own thread and streams
//! straight to disk, so large files never sit in memory.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Running,
    Done,
    Failed(String),
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct Progress {
    pub file_name: String,
    pub path: Option<PathBuf>,
    pub received: u64,
    pub total: Option<u64>,
    pub status: Status,
}

pub struct Download {
    pub url: String,
    pub progress: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
}

impl Download {
    pub fn snapshot(&self) -> Progress {
        self.progress.lock().map(|p| p.clone()).unwrap_or_else(|e| e.into_inner().clone())
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub type Notify = Arc<dyn Fn() + Send + Sync>;

pub struct Downloads {
    pub items: Vec<Download>,
    /// Where files are saved; `None` = the system Downloads folder.
    pub dir: Option<PathBuf>,
    notify: Notify,
}

/// Paths that are usually downloads rather than pages.
pub fn looks_like_download(url: &url::Url) -> bool {
    // Videos, audio, images and PDFs aren't here: they open in the tab (the
    // engines play and show them; PDFs on Linux use the built-in viewer).
    // Formats the engines can't show (mkv, avi…) are still downloaded.
    const EXTS: &[&str] = &[
        "zip", "7z", "rar", "gz", "tgz", "xz", "bz2", "zst", "tar", "exe", "msi", "dmg", "pkg",
        "deb", "rpm", "appimage", "apk", "iso", "img", "bin", "jar", "doc", "docx", "xls",
        "xlsx", "ppt", "pptx", "odt", "ods", "csv", "mkv", "avi", "wmv", "torrent", "epub", "crate", "whl",
    ];
    let path = url.path().to_ascii_lowercase();
    path.rsplit_once('.')
        .is_some_and(|(_, ext)| EXTS.contains(&ext) && !ext.contains('/'))
}

fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() { "download".to_string() } else { cleaned }
}

fn name_from_disposition(value: &str) -> Option<String> {
    // filename*=UTF-8''name.ext  or  filename="name.ext"
    for part in value.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("filename*=") {
            let v = v.rsplit("''").next().unwrap_or(v);
            let decoded = percent_decode(v);
            return Some(decoded.trim_matches('"').to_string());
        }
    }
    for part in value.split(';') {
        if let Some(v) = part.trim().strip_prefix("filename=") {
            return Some(v.trim_matches('"').to_string());
        }
    }
    None
}

pub fn percent_decode(s: &str) -> String {
    url::form_urlencoded::parse(format!("x={}", s.replace('+', "%2B")).as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| s.to_string())
}

fn name_from_url(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.path_segments().and_then(|mut s| s.next_back().map(percent_decode)))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "download".to_string())
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    (1..10_000)
        .map(|i| dir.join(format!("{stem} ({i}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(candidate)
}

impl Downloads {
    pub fn new(notify: Notify) -> Self {
        Self { items: Vec::new(), dir: None, notify }
    }

    pub fn active_count(&self) -> usize {
        self.items.iter().filter(|d| d.snapshot().status == Status::Running).count()
    }

    pub fn clear_finished(&mut self) {
        self.items.retain(|d| d.snapshot().status == Status::Running);
    }

    /// Start downloading `url`. `target` overrides the destination file.
    pub fn start(&mut self, url: String, cookies: Option<String>, referer: Option<String>, target: Option<PathBuf>) {
        let progress = Arc::new(Mutex::new(Progress {
            file_name: name_from_url(&url),
            path: None,
            received: 0,
            total: None,
            status: Status::Running,
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        self.items.push(Download { url: url.clone(), progress: progress.clone(), cancel: cancel.clone() });

        let notify = self.notify.clone();
        let dir = self.dir.clone();
        std::thread::Builder::new()
            .name("ferrum-download".into())
            .spawn(move || {
                let result = run(&url, cookies, referer, target, dir, &progress, &cancel, &notify);
                if let Ok(mut p) = progress.lock() {
                    p.status = match result {
                        Ok(()) => Status::Done,
                        Err(_) if cancel.load(Ordering::Relaxed) => Status::Cancelled,
                        Err(e) => Status::Failed(e),
                    };
                }
                notify();
            })
            .ok();
        (self.notify)();
    }
}

fn run(
    url: &str,
    cookies: Option<String>,
    referer: Option<String>,
    target: Option<PathBuf>,
    dir: Option<PathBuf>,
    progress: &Mutex<Progress>,
    cancel: &AtomicBool,
    notify: &Notify,
) -> Result<(), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().tls_config(crate::tls_config())
        .timeout_connect(Some(Duration::from_secs(20)))
        .user_agent(format!("Ferrum/{}", crate::VERSION))
        .build()
        .into();
    let mut request = agent.get(url);
    if let Some(c) = cookies.filter(|c| !c.is_empty()) {
        request = request.header("Cookie", c);
    }
    if let Some(r) = referer {
        request = request.header("Referer", r);
    }
    let mut response = request.call().map_err(|e| e.to_string())?;

    let name = response
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(name_from_disposition)
        .unwrap_or_else(|| name_from_url(url));
    let path = match target {
        Some(p) => p,
        None => {
            let dir = dir
                .or_else(dirs::download_dir)
                .or_else(dirs::home_dir)
                .ok_or("No downloads folder found")?;
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            unique_path(&dir, &sanitize(&name))
        },
    };
    let total = response.body().content_length();
    if let Ok(mut p) = progress.lock() {
        p.file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(name);
        p.path = Some(path.clone());
        p.total = total;
    }

    let partial = path.with_extension(format!(
        "{}part",
        path.extension().map(|e| format!("{}.", e.to_string_lossy())).unwrap_or_default()
    ));
    let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
    let mut reader = response.body_mut().as_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut received = 0u64;
    let mut last_notify = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(&partial);
            return Err("cancelled".into());
        }
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        received += n as u64;
        if last_notify.elapsed() > Duration::from_millis(250) {
            if let Ok(mut p) = progress.lock() {
                p.received = received;
            }
            notify();
            last_notify = Instant::now();
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(&partial, &path).map_err(|e| e.to_string())?;
    if let Ok(mut p) = progress.lock() {
        p.received = received;
        p.total = Some(received);
    }
    Ok(())
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{bytes} B") } else { format!("{v:.1} {}", UNITS[i]) }
}

/// Open a file or folder with the system's default app.
pub fn open_path(path: &Path) {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();
    if let Err(e) = result {
        log::warn!("Could not open {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(name_from_disposition("attachment; filename=\"a b.zip\""), Some("a b.zip".into()));
        assert_eq!(name_from_disposition("attachment; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf"), Some("résumé.pdf".into()));
        assert_eq!(name_from_url("https://x.org/dl/file%20one.tar.gz?x=1"), "file one.tar.gz");
        assert_eq!(sanitize("../evil:name?.exe"), "_evil_name_.exe");
        assert!(looks_like_download(&url::Url::parse("https://x.org/a/b.ZIP").unwrap()));
        assert!(!looks_like_download(&url::Url::parse("https://x.org/a/b.html").unwrap()));
        for media in ["a.pdf", "a.mp3", "a.mp4", "a.webm", "a.png", "a.mov", "a.ogg", "a.flac"] {
            assert!(!looks_like_download(&url::Url::parse(&format!("https://x.org/{media}")).unwrap()), "{media}");
        }
    }
}
