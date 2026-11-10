// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! The user's profile: settings, bookmarks, history and the last session.
//! Stored as small JSON files; writes are batched so typing/browsing never waits on disk.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use url::Url;

const MAX_HISTORY: usize = 5000;
const SAVE_DELAY: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchEngine {
    #[default]
    DuckDuckGo,
    Google,
    Bing,
    Brave,
    Startpage,
    Ecosia,
}

impl SearchEngine {
    pub const ALL: [SearchEngine; 6] = [
        SearchEngine::DuckDuckGo,
        SearchEngine::Google,
        SearchEngine::Bing,
        SearchEngine::Brave,
        SearchEngine::Startpage,
        SearchEngine::Ecosia,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SearchEngine::DuckDuckGo => "DuckDuckGo",
            SearchEngine::Google => "Google",
            SearchEngine::Bing => "Bing",
            SearchEngine::Brave => "Brave Search",
            SearchEngine::Startpage => "Startpage",
            SearchEngine::Ecosia => "Ecosia",
        }
    }

    pub fn search_url(self, query: &str) -> Url {
        let base = match self {
            SearchEngine::DuckDuckGo => "https://duckduckgo.com/html/",
            SearchEngine::Google => "https://www.google.com/search",
            SearchEngine::Bing => "https://www.bing.com/search",
            SearchEngine::Brave => "https://search.brave.com/search",
            SearchEngine::Startpage => "https://www.startpage.com/do/search",
            SearchEngine::Ecosia => "https://www.ecosia.org/search",
        };
        let mut url = Url::parse(base).expect("valid search URL");
        url.query_pairs_mut().append_pair("q", query);
        url
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub search_engine: SearchEngine,
    pub theme: ThemeChoice,
    /// Empty = the built-in start page.
    pub home_page: String,
    pub restore_session: bool,
    pub show_bookmarks_bar: bool,
    pub default_zoom: f32,
    pub ask_where_to_save: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            search_engine: SearchEngine::default(),
            theme: ThemeChoice::default(),
            home_page: String::new(),
            restore_session: true,
            show_bookmarks_bar: true,
            default_zoom: 1.0,
            ask_where_to_save: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visits: u32,
    /// Seconds since the Unix epoch.
    pub last_visit: u64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Session {
    tabs: Vec<String>,
    active: usize,
}

pub struct Profile {
    /// `None` in a private window: nothing is written to disk.
    dir: Option<PathBuf>,
    pub settings: Settings,
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<HistoryEntry>,
    dirty_since: Option<Instant>,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_json<T: for<'a> Deserialize<'a> + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_json<T: Serialize>(path: &Path, value: &T) {
    let Ok(bytes) = serde_json::to_vec_pretty(value) else { return };
    // Write then rename, so a crash never leaves a half-written file.
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

impl Profile {
    pub fn default_dir() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("Ferrum"))
    }

    pub fn load(dir: Option<PathBuf>) -> Self {
        let mut profile = Self {
            dir: None,
            settings: Settings::default(),
            bookmarks: Vec::new(),
            history: Vec::new(),
            dirty_since: None,
        };
        if let Some(dir) = dir {
            if std::fs::create_dir_all(&dir).is_ok() {
                profile.settings = read_json(&dir.join("settings.json"));
                profile.bookmarks = read_json(&dir.join("bookmarks.json"));
                profile.history = read_json(&dir.join("history.json"));
                profile.dir = Some(dir);
            }
        }
        if profile.bookmarks.is_empty() && profile.dir.is_some() && !profile.dir.as_ref().unwrap().join("bookmarks.json").exists() {
            profile.bookmarks = vec![
                Bookmark { title: "Wikipedia".into(), url: "https://en.wikipedia.org/".into() },
                Bookmark { title: "Hacker News".into(), url: "https://news.ycombinator.com/".into() },
                Bookmark { title: "Servo".into(), url: "https://servo.org/".into() },
                Bookmark { title: "crates.io".into(), url: "https://crates.io/".into() },
            ];
        }
        profile
    }

    pub fn is_private(&self) -> bool {
        self.dir.is_none()
    }

    /// Where Servo keeps cookies and site storage for this profile.
    pub fn engine_dir(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join("engine"))
    }

    pub fn mark_dirty(&mut self) {
        if self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now());
        }
    }

    /// Writes pending changes if they've waited long enough (or `force`).
    pub fn flush(&mut self, force: bool) {
        let Some(since) = self.dirty_since else { return };
        if !force && since.elapsed() < SAVE_DELAY {
            return;
        }
        self.dirty_since = None;
        let Some(dir) = &self.dir else { return };
        write_json(&dir.join("settings.json"), &self.settings);
        write_json(&dir.join("bookmarks.json"), &self.bookmarks);
        write_json(&dir.join("history.json"), &self.history);
    }

    pub fn needs_flush(&self) -> bool {
        self.dirty_since.is_some()
    }

    // ----- session -----

    pub fn load_session(&self) -> (Vec<String>, usize) {
        let Some(dir) = &self.dir else { return (Vec::new(), 0) };
        let session: Session = read_json(&dir.join("session.json"));
        (session.tabs, session.active)
    }

    pub fn save_session(&self, tabs: Vec<String>, active: usize) {
        if let Some(dir) = &self.dir {
            write_json(&dir.join("session.json"), &Session { tabs, active });
        }
    }

    // ----- bookmarks -----

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.bookmarks.iter().any(|b| b.url == url)
    }

    pub fn toggle_bookmark(&mut self, url: &str, title: &str) {
        if let Some(i) = self.bookmarks.iter().position(|b| b.url == url) {
            self.bookmarks.remove(i);
        } else {
            let title = if title.trim().is_empty() { url } else { title };
            self.bookmarks.push(Bookmark { title: title.to_string(), url: url.to_string() });
        }
        self.mark_dirty();
    }

    // ----- history -----

    pub fn record_visit(&mut self, url: &Url, title: Option<&str>) {
        if self.is_private() || !matches!(url.scheme(), "http" | "https") {
            return;
        }
        let url_str = url.as_str();
        let now = now_secs();
        if let Some(i) = self.history.iter().position(|h| h.url == url_str) {
            let mut entry = self.history.remove(i);
            entry.visits += 1;
            entry.last_visit = now;
            if let Some(t) = title.filter(|t| !t.is_empty()) {
                entry.title = t.to_string();
            }
            self.history.push(entry);
        } else {
            self.history.push(HistoryEntry {
                url: url_str.to_string(),
                title: title.unwrap_or_default().to_string(),
                visits: 1,
                last_visit: now,
            });
        }
        if self.history.len() > MAX_HISTORY {
            let excess = self.history.len() - MAX_HISTORY;
            self.history.drain(..excess);
        }
        self.mark_dirty();
    }

    pub fn update_title(&mut self, url: &Url, title: &str) {
        if title.is_empty() {
            return;
        }
        if let Some(entry) = self.history.iter_mut().rev().find(|h| h.url == url.as_str()) {
            if entry.title != title {
                entry.title = title.to_string();
                self.mark_dirty();
            }
        }
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.mark_dirty();
    }

    /// Address-bar suggestions: bookmarks first, then history ranked by visits and recency.
    pub fn suggestions(&self, input: &str, limit: usize) -> Vec<(String, String, bool)> {
        let needle = input.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let matches = |url: &str, title: &str| {
            let u = url.to_lowercase();
            let bare = u.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.");
            bare.starts_with(&needle) || u.contains(&needle) || title.to_lowercase().contains(&needle)
        };

        let mut out: Vec<(String, String, bool)> = self
            .bookmarks
            .iter()
            .filter(|b| matches(&b.url, &b.title))
            .map(|b| (b.title.clone(), b.url.clone(), true))
            .take(limit)
            .collect();

        let now = now_secs();
        let mut ranked: Vec<(f64, &HistoryEntry)> = self
            .history
            .iter()
            .filter(|h| matches(&h.url, &h.title) && !out.iter().any(|(_, u, _)| u == &h.url))
            .map(|h| {
                let age_days = (now.saturating_sub(h.last_visit)) as f64 / 86400.0;
                (h.visits as f64 / (1.0 + age_days / 7.0), h)
            })
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        out.extend(
            ranked
                .into_iter()
                .take(limit.saturating_sub(out.len()))
                .map(|(_, h)| (h.title.clone(), h.url.clone(), false)),
        );
        out
    }
}
