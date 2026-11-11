// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! The user's profile: settings, bookmarks, history and the last session.
//! Stored as small JSON files; writes are batched so typing/browsing never waits on disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::theme::ThemePreset;
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
    /// A URL template from settings, with `%s` where the query goes.
    Custom,
}

impl SearchEngine {
    pub const ALL: [SearchEngine; 7] = [
        SearchEngine::DuckDuckGo,
        SearchEngine::Google,
        SearchEngine::Bing,
        SearchEngine::Brave,
        SearchEngine::Startpage,
        SearchEngine::Ecosia,
        SearchEngine::Custom,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SearchEngine::DuckDuckGo => "DuckDuckGo",
            SearchEngine::Google => "Google",
            SearchEngine::Bing => "Bing",
            SearchEngine::Brave => "Brave Search",
            SearchEngine::Startpage => "Startpage",
            SearchEngine::Ecosia => "Ecosia",
            SearchEngine::Custom => "Custom",
        }
    }

    /// Search URL for `query`. `custom` is the user's template (used for `Custom`).
    pub fn search_url(self, query: &str, custom: &str) -> Url {
        if self == SearchEngine::Custom && custom.contains("%s") {
            let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
            if let Ok(url) = Url::parse(&custom.replace("%s", &encoded)) {
                return url;
            }
        }
        let base = match self {
            SearchEngine::DuckDuckGo => "https://duckduckgo.com/html/",
            SearchEngine::Google => "https://www.google.com/search",
            SearchEngine::Bing => "https://www.bing.com/search",
            SearchEngine::Brave => "https://search.brave.com/search",
            SearchEngine::Startpage => "https://www.startpage.com/do/search",
            SearchEngine::Ecosia => "https://www.ecosia.org/search",
            SearchEngine::Custom => "https://duckduckgo.com/html/",
        };
        let mut url = Url::parse(base).expect("valid search URL");
        url.query_pairs_mut().append_pair("q", query);
        url
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabLayout {
    #[default]
    Top,
    /// Tabs in a list down the left side.
    Vertical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartBackground {
    /// The theme's normal background.
    #[default]
    Plain,
    /// A gradient made from the accent colour.
    Gradient,
    /// A solid colour of your choice.
    Color,
    /// An image file from disk.
    Image,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewTabPage {
    #[default]
    StartPage,
    HomePage,
    Blank,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // General
    pub search_engine: SearchEngine,
    /// Template for `SearchEngine::Custom`, e.g. `https://example.com/?q=%s`.
    pub custom_search_url: String,
    /// Empty = the built-in start page.
    pub home_page: String,
    pub new_tab_page: NewTabPage,
    pub restore_session: bool,
    pub default_zoom: f32,
    pub ask_where_to_save: bool,

    /// Show bookmark and history matches under the address bar while typing.
    pub show_suggestions: bool,
    /// Where downloads are saved; empty = the system Downloads folder.
    pub download_dir: String,

    // Privacy & web content (passed to the engine)
    /// Only load pages over HTTPS.
    pub https_only: bool,
    /// Let sites ask to show notifications.
    pub allow_notifications: bool,
    /// Let sites ask for your location.
    pub allow_location: bool,
    /// Default page font size in CSS pixels.
    pub font_size: u8,
    /// Custom User-Agent string; empty = the engine's default.
    pub user_agent: String,

    // Look
    pub theme_preset: ThemePreset,
    /// Light/dark mode for the Ferrum theme (other themes have their own).
    pub theme: ThemeChoice,
    /// Tell websites to use their dark style when the interface is dark.
    /// Darken light-only sites in dark mode. (Saved under a new name: an
    /// older setting called `dark_websites` did something else.)
    pub force_dark_websites: bool,
    pub accent: [u8; 3],
    /// Size of the whole interface (not the page), 0.8–1.5.
    pub ui_scale: f32,
    pub compact: bool,
    /// Corner rounding of buttons, tabs and fields, in points.
    pub roundness: u8,
    /// Bumped when a default look changes, to move people off the old default.
    #[serde(default)] // missing in older files: 0
    pub look_version: u32,
    pub tab_layout: TabLayout,
    pub show_bookmarks_bar: bool,
    pub show_home_button: bool,
    pub show_bookmark_button: bool,
    pub show_downloads_button: bool,

    /// How wide tabs can get.
    pub tab_width: TabWidth,
    /// Which tabs show a close button.
    pub tab_close_buttons: CloseButtons,
    /// New tabs open next to the current one instead of at the end.
    pub new_tab_next_to_current: bool,
    /// Closing the last tab closes the window (otherwise a new tab opens).
    pub close_window_with_last_tab: bool,
    /// Hide the forward button when there's nothing to go forward to.
    pub forward_when_available: bool,
    pub show_reload_button: bool,
    pub show_reader_button: bool,
    pub show_loading_bar: bool,
    /// Show just the site name in the address bar until it's clicked.
    pub domain_only_address: bool,
    /// Animate panels, switches and fades.
    pub animations: bool,

    // Start page
    pub start_show_search: bool,
    pub clock_24h: bool,
    /// How many tiles the start page shows in each group at most.
    pub start_tiles: u8,
    pub start_title: String,
    pub start_background: StartBackground,
    pub start_color: [u8; 3],
    pub start_image: String,
    pub start_show_clock: bool,
    pub start_show_bookmarks: bool,
    pub start_show_frequent: bool,

    // Modern features
    /// Block requests to known ad and tracking servers from other sites.
    pub block_trackers: bool,
    /// Sites (host names) where the blocker is switched off.
    pub blocker_exceptions: Vec<String>,
    /// Remembered permission answers: host → feature → allowed.
    pub site_permissions: BTreeMap<String, BTreeMap<String, bool>>,
    /// Check GitHub for a newer Ferrum at start-up (at most once a day).
    pub check_updates: bool,
    /// Last update check, seconds since the Unix epoch.
    pub last_update_check: u64,
    /// Reopen the tabs at the next start even if restoring is off (after an update).
    pub resume_once: bool,
    /// Unload tabs that haven't been used for a while (they reload when selected).
    pub sleep_tabs: bool,
    pub sleep_minutes: u32,
    pub reader_font_size: f32,
    pub reader_theme: ReaderTheme,
    pub reader_width: f32,
    /// Draw Ferrum's own title bar (tabs in the title bar, Chrome-style window
    /// buttons) instead of the system one.
    pub custom_title_bar: bool,
    /// The user picked the search engine themselves (so Ferrum never changes it).
    pub search_engine_chosen: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TabWidth {
    Narrow,
    #[default]
    Normal,
    Wide,
}

impl TabWidth {
    pub fn max(self) -> f32 {
        match self {
            TabWidth::Narrow => 160.0,
            TabWidth::Normal => 220.0,
            TabWidth::Wide => 300.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloseButtons {
    /// Every tab that's wide enough.
    #[default]
    All,
    /// The current tab, and others while the pointer is over them.
    Hover,
    /// Only the current tab.
    Active,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReaderTheme {
    /// Follows the interface's light/dark mode.
    #[default]
    Auto,
    Light,
    Sepia,
    Dark,
}

pub const DEFAULT_ACCENT: [u8; 3] = [222, 98, 52];

impl Default for Settings {
    fn default() -> Self {
        Self {
            search_engine: SearchEngine::default(),
            custom_search_url: String::new(),
            home_page: String::new(),
            new_tab_page: NewTabPage::default(),
            restore_session: true,
            default_zoom: 1.0,
            ask_where_to_save: false,
            show_suggestions: true,
            download_dir: String::new(),
            https_only: false,
            allow_notifications: false,
            allow_location: false,
            font_size: 16,
            user_agent: String::new(),
            theme_preset: ThemePreset::default(),
            theme: ThemeChoice::default(),
            force_dark_websites: false,
            accent: DEFAULT_ACCENT,
            ui_scale: 1.0,
            compact: false,
            roundness: 12,
            look_version: 2,
            tab_layout: TabLayout::default(),
            show_bookmarks_bar: true,
            show_home_button: true,
            show_bookmark_button: true,
            show_downloads_button: true,
            tab_width: TabWidth::Normal,
            tab_close_buttons: CloseButtons::All,
            new_tab_next_to_current: false,
            close_window_with_last_tab: false,
            forward_when_available: false,
            show_reload_button: true,
            show_reader_button: true,
            show_loading_bar: true,
            domain_only_address: false,
            animations: true,
            start_show_search: true,
            clock_24h: true,
            start_tiles: 16,
            start_title: "Ferrum".into(),
            start_background: StartBackground::default(),
            start_color: [40, 44, 58],
            start_image: String::new(),
            start_show_clock: false,
            start_show_bookmarks: true,
            start_show_frequent: true,
            block_trackers: true,
            blocker_exceptions: Vec::new(),
            site_permissions: BTreeMap::new(),
            check_updates: true,
            last_update_check: 0,
            resume_once: false,
            sleep_tabs: true,
            sleep_minutes: 30,
            reader_font_size: 19.0,
            reader_theme: ReaderTheme::Auto,
            reader_width: 720.0,
            custom_title_bar: true,
            search_engine_chosen: false,
        }
    }
}

/// Countries whose networks block DuckDuckGo (Indonesia has since 2024), by time zone.
pub fn duckduckgo_blocked_here() -> bool {
    let tz = iana_time_zone::get_timezone().unwrap_or_default();
    matches!(tz.as_str(), "Asia/Jakarta" | "Asia/Pontianak" | "Asia/Makassar" | "Asia/Jayapura")
}

impl Settings {
    /// Pick a search engine that works on this network, unless the user chose one.
    pub fn fit_search_engine_to_region(&mut self) {
        if !self.search_engine_chosen && self.search_engine == SearchEngine::DuckDuckGo && duckduckgo_blocked_here() {
            self.search_engine = SearchEngine::Google;
        }
    }

    pub fn search_url(&self, query: &str) -> Url {
        self.search_engine.search_url(query, &self.custom_search_url)
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
    /// Page titles, parallel to `tabs`, so restored tabs show their names
    /// before they've loaded. (Missing in sessions saved by older versions.)
    titles: Vec<String>,
    /// Which tabs are pinned, parallel to `tabs`.
    pinned: Vec<bool>,
    active: usize,
}

/// A tab as saved in the session.
pub struct SavedTab {
    pub url: String,
    pub title: String,
    pub pinned: bool,
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
                // Rounder corners are the new default; keep a value someone chose.
                if profile.settings.look_version < 1 {
                    if profile.settings.roundness == 6 {
                        profile.settings.roundness = 12;
                    }
                    profile.settings.look_version = 1;
                }
                // Themes used to set light/dark themselves: keep the look people had.
                if profile.settings.look_version < 2 {
                    let preset = profile.settings.theme_preset;
                    if preset != crate::theme::ThemePreset::Ferrum {
                        profile.settings.theme = if preset.native_dark() { ThemeChoice::Dark } else { ThemeChoice::Light };
                    }
                    profile.settings.look_version = 2;
                }
                profile.bookmarks = read_json(&dir.join("bookmarks.json"));
                profile.history = read_json(&dir.join("history.json"));
                profile.dir = Some(dir);
            }
        }
        if profile.bookmarks.is_empty() && profile.dir.is_some() && !profile.dir.as_ref().unwrap().join("bookmarks.json").exists() {
            profile.bookmarks = vec![
                Bookmark { title: "Wikipedia".into(), url: "https://en.wikipedia.org/".into() },
                Bookmark { title: "Hacker News".into(), url: "https://news.ycombinator.com/".into() },
                Bookmark { title: "GitHub".into(), url: "https://github.com/".into() },
                Bookmark { title: "crates.io".into(), url: "https://crates.io/".into() },
            ];
        }
        // Ferrum used to ship with a Servo bookmark; swap the untouched default for GitHub.
        for b in &mut profile.bookmarks {
            if b.title == "Servo" && b.url == "https://servo.org/" {
                b.title = "GitHub".into();
                b.url = "https://github.com/".into();
            }
        }
        profile
    }

    /// The profile folder (`None` in a private window).
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    pub fn is_private(&self) -> bool {
        self.dir.is_none()
    }

    /// Where the web engine keeps cookies and site storage for this profile (Windows, Linux).
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

    /// When pending changes should be written, if there are any.
    pub fn flush_at(&self) -> Option<Instant> {
        self.dirty_since.map(|t| t + SAVE_DELAY)
    }

    /// True when changes have waited long enough to be written.
    pub fn flush_due(&self) -> bool {
        self.dirty_since.is_some_and(|t| t.elapsed() >= SAVE_DELAY)
    }

    // ----- session -----

    /// The tabs open last time as (url, title) pairs, and which one was active.
    pub fn load_session(&self) -> (Vec<SavedTab>, usize) {
        let Some(dir) = &self.dir else { return (Vec::new(), 0) };
        let mut session: Session = read_json(&dir.join("session.json"));
        session.titles.resize(session.tabs.len(), String::new());
        session.pinned.resize(session.tabs.len(), false);
        let tabs = session
            .tabs
            .into_iter()
            .zip(session.titles)
            .zip(session.pinned)
            .map(|((url, title), pinned)| SavedTab { url, title, pinned })
            .collect();
        (tabs, session.active)
    }

    pub fn save_session(&self, tabs: Vec<SavedTab>, active: usize) {
        if let Some(dir) = &self.dir {
            let mut session = Session { active, ..Default::default() };
            for t in tabs {
                session.tabs.push(t.url);
                session.titles.push(t.title);
                session.pinned.push(t.pinned);
            }
            write_json(&dir.join("session.json"), &session);
        }
    }

    /// Replace the bookmarks with an imported list, skipping ones already present.
    pub fn import_bookmarks(&mut self, items: Vec<Bookmark>) -> usize {
        let mut added = 0;
        for b in items {
            if !self.is_bookmarked(&b.url) {
                self.bookmarks.push(b);
                added += 1;
            }
        }
        self.mark_dirty();
        added
    }

    /// Merge history from another browser: known pages keep the higher visit
    /// count and the later visit; the newest pages are kept.
    pub fn import_history(&mut self, items: Vec<HistoryEntry>) -> usize {
        if self.is_private() {
            return 0;
        }
        let mut index: std::collections::HashMap<String, usize> =
            self.history.iter().enumerate().map(|(i, h)| (h.url.clone(), i)).collect();
        let mut added = 0;
        for h in items {
            match index.get(&h.url) {
                Some(&i) => {
                    let e = &mut self.history[i];
                    e.visits = e.visits.max(h.visits);
                    e.last_visit = e.last_visit.max(h.last_visit);
                    if e.title.is_empty() {
                        e.title = h.title;
                    }
                },
                None => {
                    index.insert(h.url.clone(), self.history.len());
                    self.history.push(h);
                    added += 1;
                },
            }
        }
        // History is kept oldest first.
        self.history.sort_by_key(|h| h.last_visit);
        if self.history.len() > MAX_HISTORY {
            let excess = self.history.len() - MAX_HISTORY;
            self.history.drain(..excess);
        }
        self.mark_dirty();
        added
    }

    // ----- bookmarks -----

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.bookmarks.iter().any(|b| b.url == url)
    }

    /// Remove a bookmark. Returns where it was and what it was, for undo.
    pub fn remove_bookmark(&mut self, url: &str) -> Option<(usize, Bookmark)> {
        let i = self.bookmarks.iter().position(|b| b.url == url)?;
        let b = self.bookmarks.remove(i);
        self.mark_dirty();
        Some((i, b))
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
