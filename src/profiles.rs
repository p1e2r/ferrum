// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Profiles: separate sets of settings, bookmarks, history, cookies and
//! sign-ins, each opened in its own window (`ferrum --profile <id>`).
//!
//! The first profile ("default") lives in Ferrum's data folder as before;
//! the others in `Profiles/<id>/` inside it. `profiles.json` lists them with
//! their name, colour and linked Google account. On macOS each profile also
//! gets its own WebKit data store (`store_identifier`); elsewhere the engine
//! keeps its data in the profile's folder.
//!
//! Linking a Google account needs no OAuth: you sign in to Google in the
//! profile as usual, and the page script reads the account chip on Google's
//! pages (name, email, picture). Nothing is sent anywhere.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const DEFAULT_ID: &str = "default";

/// Colours offered for new profiles.
pub const COLORS: &[[u8; 3]] = &[
    [26, 115, 232],
    [217, 48, 37],
    [30, 142, 62],
    [249, 171, 0],
    [161, 66, 244],
    [0, 137, 123],
    [232, 113, 10],
    [95, 99, 104],
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GoogleAccount {
    pub name: String,
    pub email: String,
    /// The account picture's address (googleusercontent.com).
    pub picture: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    pub color: [u8; 3],
    #[serde(default)]
    pub google: Option<GoogleAccount>,
}

impl ProfileInfo {
    /// The name to show: the profile's, or the Google account's if it was never renamed.
    pub fn display_name(&self) -> String {
        match &self.google {
            Some(g) if self.name.starts_with("Person ") && !g.name.is_empty() => g.name.clone(),
            _ => self.name.clone(),
        }
    }

    pub fn initial(&self) -> String {
        self.display_name().chars().find(|c| c.is_alphanumeric()).unwrap_or('?').to_uppercase().to_string()
    }
}

fn root() -> Option<PathBuf> {
    crate::profile::Profile::default_dir()
}

fn list_file() -> Option<PathBuf> {
    root().map(|r| r.join("profiles.json"))
}

/// Every profile; the default one is always first.
pub fn list() -> Vec<ProfileInfo> {
    let mut list: Vec<ProfileInfo> = list_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if !list.iter().any(|p| p.id == DEFAULT_ID) {
        list.insert(0, ProfileInfo { id: DEFAULT_ID.into(), name: "Person 1".into(), color: COLORS[0], google: None });
    }
    list
}

pub fn save(list: &[ProfileInfo]) {
    let Some(file) = list_file() else { return };
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(list) {
        let tmp = file.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(tmp, file);
        }
    }
}

pub fn get(id: &str) -> ProfileInfo {
    list().into_iter().find(|p| p.id == id).unwrap_or_else(|| ProfileInfo { id: id.into(), name: "Person 1".into(), color: COLORS[0], google: None })
}

/// Change one profile and save the list.
pub fn update(id: &str, change: impl FnOnce(&mut ProfileInfo)) {
    let mut all = list();
    if let Some(p) = all.iter_mut().find(|p| p.id == id) {
        change(p);
        save(&all);
    }
}

/// Where a profile keeps its data.
pub fn dir(id: &str) -> Option<PathBuf> {
    let root = root()?;
    Some(if id == DEFAULT_ID { root } else { root.join("Profiles").join(id) })
}

/// A new profile, saved to the list.
pub fn create() -> ProfileInfo {
    let mut all = list();
    let n = (1..).find(|n| !all.iter().any(|p| p.name == format!("Person {n}"))).unwrap_or(all.len() + 1);
    let id = format!("p{:x}", crate::profile::now_secs() ^ ((std::process::id() as u64) << 32));
    let info = ProfileInfo { id, name: format!("Person {n}"), color: COLORS[all.len() % COLORS.len()], google: None };
    all.push(info.clone());
    save(&all);
    info
}

/// Remove a profile and its data (never the default one).
pub fn delete(id: &str) {
    if id == DEFAULT_ID {
        return;
    }
    let mut all = list();
    all.retain(|p| p.id != id);
    save(&all);
    if let Some(d) = dir(id) {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// Open a profile in a new window.
pub fn launch(id: &str) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).args(["--profile", id]).spawn();
    }
}

/// macOS: a stable WebKit data-store identifier for a profile (None for the
/// default profile, which keeps WebKit's default store).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn store_identifier(id: &str) -> Option<[u8; 16]> {
    if id == DEFAULT_ID {
        return None;
    }
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(format!("ferrum-profile:{id}").as_bytes());
    let mut out = [0u8; 16];
    out.copy_from_slice(&hash[..16]);
    // Mark it as a version-4 style UUID.
    out[6] = (out[6] & 0x0f) | 0x40;
    out[8] = (out[8] & 0x3f) | 0x80;
    Some(out)
}

/// Read the account from the label of Google's account button, e.g.
/// "Google Account: Andrew Example\n(andrew@example.com)" (any language).
pub fn parse_account_label(label: &str, picture: Option<String>) -> Option<GoogleAccount> {
    let open = label.rfind('(')?;
    let close = label[open..].find(')')? + open;
    let email = label[open + 1..close].trim().to_string();
    if !email.contains('@') {
        return None;
    }
    let before = &label[..open];
    let name = before.split_once(':').map(|(_, n)| n).unwrap_or(before).trim().to_string();
    Some(GoogleAccount { name, email, picture: picture.filter(|p| p.starts_with("https://")) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_google_account_labels() {
        let a = parse_account_label("Google Account: Andrew Example  \n(andrew@example.com)", Some("https://lh3.googleusercontent.com/a/x".into())).unwrap();
        assert_eq!((a.name.as_str(), a.email.as_str()), ("Andrew Example", "andrew@example.com"));
        assert!(a.picture.is_some());
        let b = parse_account_label("Akun Google: Budi (budi@gmail.com)", Some("javascript:x".into())).unwrap();
        assert_eq!((b.name.as_str(), b.email.as_str(), b.picture), ("Budi", "budi@gmail.com", None));
        assert!(parse_account_label("Sign in", None).is_none());
    }

    #[test]
    fn store_ids_are_stable_and_distinct() {
        assert_eq!(store_identifier(DEFAULT_ID), None);
        assert_eq!(store_identifier("p1"), store_identifier("p1"));
        assert_ne!(store_identifier("p1"), store_identifier("p2"));
    }
}
