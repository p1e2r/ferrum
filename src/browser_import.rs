// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Importing bookmarks and history straight from other browsers installed on
//! this computer: Chrome, Edge, Brave, Vivaldi, Opera, Chromium, Arc, Firefox
//! and (on macOS) Safari.
//!
//! Chromium-based browsers keep bookmarks in a JSON file and history in
//! SQLite; Firefox keeps both in `places.sqlite`; Safari uses a property list
//! for bookmarks and SQLite for history. Databases are copied first, because
//! the browser may have them open and locked.

use std::path::{Path, PathBuf};

use crate::profile::{Bookmark, HistoryEntry};

/// Imported history is capped, newest first.
const HISTORY_LIMIT: usize = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum Kind {
    Chromium,
    Firefox,
    Safari,
}

/// One browser profile found on this computer.
#[derive(Clone, Debug)]
pub struct Source {
    /// "Chrome", "Chrome (Work)", "Firefox (default-release)"…
    pub name: String,
    pub kind: Kind,
    pub dir: PathBuf,
}

#[derive(Default)]
pub struct Imported {
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<HistoryEntry>,
    /// Problems worth telling the user about (e.g. macOS privacy settings).
    pub notes: Vec<String>,
}

/// Every browser profile we can import from.
pub fn find_sources() -> Vec<Source> {
    let mut out = Vec::new();
    for (name, dir) in chromium_roots() {
        chromium_profiles(&name, &dir, &mut out);
    }
    for dir in firefox_roots() {
        firefox_profiles(&dir, &mut out);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = dirs::home_dir() {
        let dir = home.join("Library/Safari");
        if dir.is_dir() {
            out.push(Source { name: "Safari".into(), kind: Kind::Safari, dir });
        }
    }
    out
}

fn chromium_roots() -> Vec<(String, PathBuf)> {
    let mut roots = Vec::new();
    let mut add = |name: &str, path: Option<PathBuf>| {
        if let Some(p) = path.filter(|p| p.is_dir()) {
            roots.push((name.to_string(), p));
        }
    };
    let local = dirs::data_local_dir();
    let config = dirs::config_dir();
    let j = |base: &Option<PathBuf>, rel: &str| base.as_ref().map(|b| b.join(rel));
    if cfg!(target_os = "macos") {
        // Both are ~/Library/Application Support on macOS.
        add("Chrome", j(&config, "Google/Chrome"));
        add("Edge", j(&config, "Microsoft Edge"));
        add("Brave", j(&config, "BraveSoftware/Brave-Browser"));
        add("Vivaldi", j(&config, "Vivaldi"));
        add("Opera", j(&config, "com.operasoftware.Opera"));
        add("Arc", j(&config, "Arc/User Data"));
        add("Chromium", j(&config, "Chromium"));
    } else if cfg!(target_os = "windows") {
        add("Chrome", j(&local, "Google/Chrome/User Data"));
        add("Edge", j(&local, "Microsoft/Edge/User Data"));
        add("Brave", j(&local, "BraveSoftware/Brave-Browser/User Data"));
        add("Vivaldi", j(&local, "Vivaldi/User Data"));
        add("Opera", j(&config, "Opera Software/Opera Stable"));
        add("Chromium", j(&local, "Chromium/User Data"));
    } else {
        add("Chrome", j(&config, "google-chrome"));
        add("Edge", j(&config, "microsoft-edge"));
        add("Brave", j(&config, "BraveSoftware/Brave-Browser"));
        add("Vivaldi", j(&config, "vivaldi"));
        add("Opera", j(&config, "opera"));
        add("Chromium", j(&config, "chromium"));
        add("Chromium", dirs::home_dir().map(|h| h.join("snap/chromium/common/chromium")));
    }
    roots
}

/// A Chromium "User Data" folder holds `Default` and `Profile N` folders,
/// named in `Local State`. Opera keeps a single profile in the root itself.
fn chromium_profiles(browser: &str, root: &Path, out: &mut Vec<Source>) {
    let names: serde_json::Value = std::fs::read_to_string(root.join("Local State"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let mut found = Vec::new();
    if root.join("Bookmarks").is_file() || root.join("History").is_file() {
        found.push((String::new(), root.to_path_buf()));
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for e in entries.flatten() {
            let dir = e.path();
            let folder = e.file_name().to_string_lossy().to_string();
            if dir.is_dir() && (dir.join("Bookmarks").is_file() || dir.join("History").is_file()) {
                let label = names["profile"]["info_cache"][&folder]["name"].as_str().unwrap_or(&folder).to_string();
                found.push((label, dir));
            }
        }
    }
    found.sort_by(|a, b| a.1.cmp(&b.1));
    let single = found.len() == 1;
    for (label, dir) in found {
        let name = if single || label.is_empty() { browser.to_string() } else { format!("{browser} ({label})") };
        out.push(Source { name, kind: Kind::Chromium, dir });
    }
}

fn firefox_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(target_os = "macos") {
        roots.extend(dirs::config_dir().map(|c| c.join("Firefox")));
    } else if cfg!(target_os = "windows") {
        roots.extend(dirs::config_dir().map(|c| c.join("Mozilla/Firefox")));
    } else if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".mozilla/firefox"));
        roots.push(home.join("snap/firefox/common/.mozilla/firefox"));
        roots.push(home.join(".var/app/org.mozilla.firefox/.mozilla/firefox"));
    }
    roots.retain(|r| r.is_dir());
    roots
}

fn firefox_profiles(root: &Path, out: &mut Vec<Source>) {
    let mut dirs = Vec::new();
    for base in [root.join("Profiles"), root.to_path_buf()] {
        if let Ok(entries) = std::fs::read_dir(&base) {
            for e in entries.flatten() {
                if e.path().join("places.sqlite").is_file() {
                    dirs.push(e.path());
                }
            }
        }
    }
    dirs.sort();
    dirs.dedup();
    let single = dirs.len() == 1;
    for dir in dirs {
        let folder = dir.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        // Folder names look like "a1b2c3d4.default-release".
        let label = folder.split_once('.').map(|(_, l)| l.to_string()).unwrap_or(folder);
        let name = if single { "Firefox".to_string() } else { format!("Firefox ({label})") };
        out.push(Source { name, kind: Kind::Firefox, dir });
    }
}

/// Read bookmarks and/or history from one profile.
pub fn import(source: &Source, bookmarks: bool, history: bool) -> Imported {
    let mut out = Imported::default();
    let result = match source.kind {
        Kind::Chromium => chromium(&source.dir, bookmarks, history, &mut out),
        Kind::Firefox => firefox(&source.dir, bookmarks, history, &mut out),
        Kind::Safari => safari(&source.dir, bookmarks, history, &mut out),
    };
    if let Err(e) = result {
        out.notes.push(e);
    }
    out.bookmarks.retain(|b| b.url.starts_with("http://") || b.url.starts_with("https://"));
    out.history.retain(|h| h.url.starts_with("http://") || h.url.starts_with("https://"));
    out
}

// ----- Chromium -----

fn chromium(dir: &Path, bookmarks: bool, history: bool, out: &mut Imported) -> Result<(), String> {
    if bookmarks
        && let Ok(text) = std::fs::read_to_string(dir.join("Bookmarks")) {
            out.bookmarks = chromium_bookmarks(&text);
        }
    if history && dir.join("History").is_file() {
        let db = open_copy(&dir.join("History"))?;
        let mut stmt = db
            .conn
            .prepare("SELECT url, title, visit_count, last_visit_time FROM urls WHERE hidden = 0 ORDER BY last_visit_time DESC LIMIT ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([HISTORY_LIMIT as i64], |r| {
                Ok(HistoryEntry {
                    url: r.get(0)?,
                    title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    visits: r.get::<_, i64>(2)?.max(1) as u32,
                    // Microseconds since 1601.
                    last_visit: (r.get::<_, i64>(3)? / 1_000_000 - 11_644_473_600).max(0) as u64,
                })
            })
            .map_err(|e| e.to_string())?;
        out.history = rows.flatten().collect();
    }
    Ok(())
}

/// The bookmarks bar first, then "Other bookmarks" and mobile ones; folders flattened.
pub fn chromium_bookmarks(json: &str) -> Vec<Bookmark> {
    fn walk(node: &serde_json::Value, out: &mut Vec<Bookmark>) {
        match node["type"].as_str() {
            Some("url") => {
                if let Some(url) = node["url"].as_str() {
                    let title = node["name"].as_str().unwrap_or(url);
                    out.push(Bookmark { title: if title.is_empty() { url } else { title }.to_string(), url: url.to_string() });
                }
            },
            _ => {
                if let Some(children) = node["children"].as_array() {
                    for c in children {
                        walk(c, out);
                    }
                }
            },
        }
    }
    let root: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let mut out = Vec::new();
    for key in ["bookmark_bar", "other", "synced"] {
        walk(&root["roots"][key], &mut out);
    }
    out
}

// ----- Firefox -----

fn firefox(dir: &Path, bookmarks: bool, history: bool, out: &mut Imported) -> Result<(), String> {
    let db = open_copy(&dir.join("places.sqlite"))?;
    if bookmarks {
        let mut stmt = db
            .conn
            .prepare(
                "SELECT COALESCE(b.title, p.title, p.url), p.url FROM moz_bookmarks b JOIN moz_places p ON b.fk = p.id \
                 WHERE b.type = 1 AND p.url NOT LIKE 'place:%' ORDER BY b.parent, b.position",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| Ok(Bookmark { title: r.get(0)?, url: r.get(1)? })).map_err(|e| e.to_string())?;
        out.bookmarks = rows.flatten().collect();
    }
    if history {
        let mut stmt = db
            .conn
            .prepare(
                "SELECT url, title, visit_count, last_visit_date FROM moz_places \
                 WHERE visit_count > 0 AND last_visit_date IS NOT NULL AND hidden = 0 ORDER BY last_visit_date DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([HISTORY_LIMIT as i64], |r| {
                Ok(HistoryEntry {
                    url: r.get(0)?,
                    title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    visits: r.get::<_, i64>(2)?.max(1) as u32,
                    // Microseconds since 1970.
                    last_visit: (r.get::<_, i64>(3)? / 1_000_000).max(0) as u64,
                })
            })
            .map_err(|e| e.to_string())?;
        out.history = rows.flatten().collect();
    }
    Ok(())
}

// ----- Safari -----

const SAFARI_ACCESS: &str = "macOS doesn't let apps read Safari's data. Allow it in System Settings → Privacy & Security → Full Disk Access → Ferrum, or export from Safari (File → Export → Bookmarks) and import the file.";

fn safari(dir: &Path, bookmarks: bool, history: bool, out: &mut Imported) -> Result<(), String> {
    if bookmarks {
        match plist::Value::from_file(dir.join("Bookmarks.plist")) {
            Ok(root) => out.bookmarks = safari_bookmarks(&root),
            Err(_) => return Err(SAFARI_ACCESS.into()),
        }
    }
    if history {
        let db = open_copy(&dir.join("History.db")).map_err(|_| SAFARI_ACCESS.to_string())?;
        let mut stmt = db
            .conn
            .prepare(
                "SELECT i.url, COALESCE(v.title, ''), i.visit_count, MAX(v.visit_time) FROM history_items i \
                 JOIN history_visits v ON v.history_item = i.id GROUP BY i.id ORDER BY MAX(v.visit_time) DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([HISTORY_LIMIT as i64], |r| {
                Ok(HistoryEntry {
                    url: r.get(0)?,
                    title: r.get(1)?,
                    visits: r.get::<_, i64>(2)?.max(1) as u32,
                    // Seconds since 2001.
                    last_visit: (r.get::<_, f64>(3)? as i64 + 978_307_200).max(0) as u64,
                })
            })
            .map_err(|e| e.to_string())?;
        out.history = rows.flatten().collect();
    }
    Ok(())
}

fn safari_bookmarks(root: &plist::Value) -> Vec<Bookmark> {
    fn walk(node: &plist::Value, out: &mut Vec<Bookmark>) {
        let Some(dict) = node.as_dictionary() else { return };
        // Skip the Reading List.
        if dict.get("Title").and_then(|t| t.as_string()) == Some("com.apple.ReadingList") {
            return;
        }
        if let Some(url) = dict.get("URLString").and_then(|u| u.as_string()) {
            let title = dict
                .get("URIDictionary")
                .and_then(|d| d.as_dictionary())
                .and_then(|d| d.get("title"))
                .and_then(|t| t.as_string())
                .unwrap_or(url);
            out.push(Bookmark { title: title.to_string(), url: url.to_string() });
        }
        if let Some(children) = dict.get("Children").and_then(|c| c.as_array()) {
            for c in children {
                walk(c, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

// ----- SQLite -----

/// A read-only copy of a database another browser may have open.
struct Copy {
    conn: rusqlite::Connection,
    dir: PathBuf,
}

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn open_copy(path: &Path) -> Result<Copy, String> {
    let dir = std::env::temp_dir().join(format!("ferrum-import-{}-{}", std::process::id(), crate::profile::now_secs()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join("db.sqlite");
    std::fs::copy(path, &target).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    // Recent changes may still be in the write-ahead log.
    for ext in ["-wal", "-shm"] {
        let side = PathBuf::from(format!("{}{ext}", path.display()));
        if side.is_file() {
            let _ = std::fs::copy(&side, dir.join(format!("db.sqlite{ext}")));
        }
    }
    let conn = rusqlite::Connection::open(&target).map_err(|e| e.to_string())?;
    Ok(Copy { conn, dir })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_chromium_bookmarks() {
        let json = r#"{"roots":{
            "bookmark_bar":{"type":"folder","children":[
                {"type":"url","name":"Rust","url":"https://www.rust-lang.org/"},
                {"type":"folder","name":"News","children":[{"type":"url","name":"","url":"https://news.ycombinator.com/"}]}]},
            "other":{"type":"folder","children":[{"type":"url","name":"Docs","url":"https://docs.rs/"}]},
            "synced":{"type":"folder","children":[]}}}"#;
        let list = chromium_bookmarks(json);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].title, "Rust");
        assert_eq!(list[1].title, "https://news.ycombinator.com/");
        assert_eq!(list[2].url, "https://docs.rs/");
    }

    #[test]
    fn reads_firefox_database() {
        let dir = std::env::temp_dir().join(format!("ferrum-ff-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = rusqlite::Connection::open(dir.join("places.sqlite")).unwrap();
        conn.execute_batch(
            "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, last_visit_date INTEGER, hidden INTEGER DEFAULT 0);
             CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER, parent INTEGER, position INTEGER, title TEXT);
             INSERT INTO moz_places VALUES (1, 'https://example.com/', 'Example', 3, 1700000000000000, 0);
             INSERT INTO moz_places VALUES (2, 'place:sort=8', NULL, 0, NULL, 0);
             INSERT INTO moz_bookmarks VALUES (1, 1, 1, 2, 0, 'My example');
             INSERT INTO moz_bookmarks VALUES (2, 1, 2, 2, 1, 'Smart folder');",
        )
        .unwrap();
        drop(conn);
        let got = import(&Source { name: "Firefox".into(), kind: Kind::Firefox, dir: dir.clone() }, true, true);
        assert_eq!(got.bookmarks.len(), 1);
        assert_eq!(got.bookmarks[0].title, "My example");
        assert_eq!(got.history.len(), 1);
        assert_eq!(got.history[0].last_visit, 1_700_000_000);
        let _ = std::fs::remove_dir_all(dir);
    }
}
