// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Checks GitHub for a newer Ferrum release, on a background thread.

use std::sync::{Arc, Mutex};
use std::time::Duration;

const RELEASES_API: &str = "https://api.github.com/repos/p1e2r/ferrum/releases?per_page=30";
pub const RELEASES_PAGE: &str = "https://github.com/p1e2r/ferrum/releases";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStatus {
    Checking,
    UpToDate,
    Available { version: String, url: String },
    Failed(String),
}

/// `"v0.6.2"` → `(0, 6, 2)`.
pub fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let v = v.trim().trim_start_matches('v');
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u32>().ok());
    Some((parts.next()??, parts.next().flatten().unwrap_or(0), parts.next().flatten().unwrap_or(0)))
}

/// The newest (tag, page) in a GitHub releases list, by version number.
fn newest(json: &serde_json::Value) -> Option<(String, String)> {
    json.as_array()?
        .iter()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false))
        .filter_map(|r| {
            let tag = r["tag_name"].as_str()?;
            let url = r["html_url"].as_str().unwrap_or(RELEASES_PAGE);
            Some((parse_version(tag)?, tag.trim_start_matches('v').to_string(), url.to_string()))
        })
        .max_by_key(|(v, _, _)| *v)
        .map(|(_, tag, url)| (tag, url))
}

/// Start a check; the result appears in the returned slot. `notify` wakes the UI.
pub fn check(current: &str, notify: Arc<dyn Fn() + Send + Sync>) -> Arc<Mutex<UpdateStatus>> {
    let slot = Arc::new(Mutex::new(UpdateStatus::Checking));
    let out = slot.clone();
    let current = current.to_string();
    let _ = std::thread::Builder::new().name("update-check".into()).spawn(move || {
        let status = match fetch() {
            Ok(json) => match newest(&json) {
                Some((version, url)) if parse_version(&version) > parse_version(&current) => UpdateStatus::Available { version, url },
                _ => UpdateStatus::UpToDate,
            },
            Err(e) => UpdateStatus::Failed(e),
        };
        if let Ok(mut s) = out.lock() {
            *s = status;
        }
        notify();
    });
    slot
}

fn fetch() -> Result<serde_json::Value, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let mut response = agent
        .get(RELEASES_API)
        .header("User-Agent", "Ferrum-update-check")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    let text = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.6.2"), Some((0, 6, 2)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("0.5.0-nightly"), Some((0, 5, 0)));
        assert_eq!(parse_version("nightly"), None);
        assert!(parse_version("0.10.0") > parse_version("0.9.9"));
    }

    #[test]
    fn picks_highest_version() {
        let json: serde_json::Value = serde_json::from_str(
            r#"[{"tag_name":"v0.4.0","html_url":"a"},{"tag_name":"v0.10.1","html_url":"b"},{"tag_name":"nightly","html_url":"c"},{"tag_name":"v0.9.0","html_url":"d","draft":true}]"#,
        )
        .unwrap();
        assert_eq!(newest(&json), Some(("0.10.1".into(), "b".into())));
    }
}
