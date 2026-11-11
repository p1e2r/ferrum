// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Ad and tracker blocking.
//!
//! Servo asks the embedder about every resource a page loads
//! (`WebViewDelegate::load_web_resource`). Requests from one site to a known
//! ad or tracking server on *another* site are cancelled. Main documents are
//! never blocked, so typing a listed address still opens it.
//!
//! The list is a built-in set of well-known domains, extended with Peter
//! Lowe's ad and tracking server list, downloaded in the background at most
//! once a week and cached in the profile.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use servo::WebViewId;

const LIST_URL: &str = "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=nohtml&showintro=0&mimetype=plaintext";
const LIST_FILE: &str = "blocklist.txt";
const LIST_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

/// Domains that are blocked even before the downloaded list arrives.
const BUILT_IN: &[&str] = &[
    // Ad networks and exchanges
    "doubleclick.net", "googlesyndication.com", "googleadservices.com", "adservice.google.com",
    "pagead2.googlesyndication.com", "adnxs.com", "adsrvr.org", "advertising.com", "criteo.com",
    "criteo.net", "pubmatic.com", "rubiconproject.com", "openx.net", "casalemedia.com",
    "taboola.com", "outbrain.com", "amazon-adsystem.com", "media.net", "moatads.com",
    "smartadserver.com", "adform.net", "yieldmo.com", "sharethrough.com", "teads.tv",
    "bidswitch.net", "3lift.com", "triplelift.com", "contextweb.com", "indexww.com",
    "zemanta.com", "revcontent.com", "mgid.com", "adroll.com", "serving-sys.com",
    "adsafeprotected.com", "doubleverify.com", "innovid.com", "spotxchange.com", "springserve.com",
    "lijit.com", "sovrn.com", "33across.com", "gumgum.com", "undertone.com", "yieldlab.net",
    "adition.com", "adtech.de", "popads.net", "propellerads.com", "exoclick.com", "adsterra.com",
    "trafficjunky.net", "juicyads.com", "zedo.com", "infolinks.com", "buysellads.com",
    "carbonads.net", "admixer.net", "adkernel.com", "smaato.net", "inmobi.com", "applovin.com",
    "unityads.unity3d.com", "chartboost.com", "vungle.com", "ironsrc.com", "mopub.com",
    // Analytics and tracking
    "google-analytics.com", "googletagmanager.com", "googletagservices.com", "analytics.google.com",
    "scorecardresearch.com", "quantserve.com", "quantcount.com", "chartbeat.com", "chartbeat.net",
    "hotjar.com", "hotjar.io", "mouseflow.com", "fullstory.com", "crazyegg.com", "luckyorange.com",
    "clarity.ms", "mixpanel.com", "segment.io", "segment.com", "amplitude.com", "heap.io",
    "heapanalytics.com", "kissmetrics.com", "newrelic.com", "nr-data.net", "optimizely.com",
    "omtrdc.net", "demdex.net", "everesttech.net", "2o7.net", "bluekai.com", "krxd.net",
    "exelator.com", "eyeota.net", "rlcdn.com", "liadm.com", "tapad.com", "agkn.com",
    "addthis.com", "sharethis.com", "bounceexchange.com", "bouncex.net", "parsely.com",
    "parse.ly", "comscore.com", "nielsen.com", "imrworldwide.com", "statcounter.com",
    "yandex.ru/metrika", "mc.yandex.ru", "top-fwz1.mail.ru", "hm.baidu.com", "cnzz.com",
    "branch.io", "app-measurement.com", "appsflyer.com", "adjust.com", "kochava.com",
    "braze.com", "onesignal.com", "pushcrew.com", "pusher-ads.com", "trackcmp.net",
    "facebook.net", "connect.facebook.net", "pixel.facebook.com", "ads-twitter.com",
    "analytics.twitter.com", "static.ads-twitter.com", "ads.linkedin.com", "px.ads.linkedin.com",
    "snap.licdn.com", "analytics.tiktok.com", "ads.tiktok.com", "ct.pinterest.com",
    "bat.bing.com", "ads.yahoo.com", "analytics.yahoo.com", "sc-static.net", "tr.snapchat.com",
    "quora.com/_/ad", "redditstatic.com/ads", "alb.reddit.com", "pixel.reddit.com",
    "matomo.cloud", "piwik.pro", "kameleoon.com", "vwo.com", "visualwebsiteoptimizer.com",
    "zoominfo.com", "ws.zoominfo.com", "clearbit.com", "6sc.co", "demandbase.com", "bizible.com",
    "marketo.net", "mktoresp.com", "pardot.com", "hs-analytics.net", "hs-banner.com",
    "hsadspixel.net", "hubspot.net/analytics", "intercomcdn.com/analytics",
];

/// The registrable part of a host name, roughly: the last two labels, or three
/// for hosts like `bbc.co.uk`.
pub fn base_domain(host: &str) -> &str {
    let host = host.trim_end_matches('.');
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() <= 2 || host.parse::<std::net::IpAddr>().is_ok() {
        return host;
    }
    let n = labels.len();
    let second = labels[n - 2];
    let take = if labels[n - 1].len() == 2 && matches!(second, "co" | "com" | "org" | "net" | "gov" | "ac" | "edu" | "ne" | "or") {
        3
    } else {
        2
    };
    let skip: usize = labels[..n - take].iter().map(|l| l.len() + 1).sum();
    &host[skip..]
}

pub struct Blocker {
    domains: Arc<RwLock<HashSet<String>>>,
    pub enabled: Cell<bool>,
    /// Sites where blocking is switched off.
    pub exceptions: RefCell<HashSet<String>>,
    /// Blocked addresses per tab, for the page (URL) it is showing.
    blocked: RefCell<HashMap<WebViewId, (String, HashSet<String>)>>,
}

impl Blocker {
    pub fn new(enabled: bool, exceptions: &[String]) -> Self {
        let domains: HashSet<String> = BUILT_IN.iter().filter(|d| !d.contains('/')).map(|d| d.to_string()).collect();
        Self {
            domains: Arc::new(RwLock::new(domains)),
            enabled: Cell::new(enabled),
            exceptions: RefCell::new(exceptions.iter().cloned().collect()),
            blocked: RefCell::new(HashMap::new()),
        }
    }

    /// Load the cached full list on a background thread, and download a fresh
    /// one if it's missing or more than a week old.
    pub fn load_list_in_background(&self, profile_dir: Option<PathBuf>) {
        let domains = self.domains.clone();
        let spawn = std::thread::Builder::new().name("blocklist".into()).spawn(move || {
            let path = profile_dir.map(|d| d.join(LIST_FILE));
            let cached = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
            let fresh = path
                .as_ref()
                .and_then(|p| std::fs::metadata(p).ok())
                .and_then(|m| m.modified().ok())
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .is_some_and(|age| age < LIST_MAX_AGE);
            if let Some(text) = &cached {
                add_list(&domains, text);
            }
            if fresh {
                return;
            }
            match download_list() {
                Ok(text) => {
                    let count = add_list(&domains, &text);
                    log::info!("Blocklist updated: {count} domains");
                    if let Some(path) = &path {
                        let tmp = path.with_extension("tmp");
                        if std::fs::write(&tmp, &text).is_ok() {
                            let _ = std::fs::rename(&tmp, path);
                        }
                    }
                },
                Err(e) => log::info!("Could not download the blocklist: {e}"),
            }
        });
        if let Err(e) = spawn {
            log::warn!("Could not start the blocklist thread: {e}");
        }
    }

    pub fn domain_count(&self) -> usize {
        self.domains.read().map(|d| d.len()).unwrap_or(0)
    }

    /// Whether a request for `host`, made by a page on `page_host`, should be blocked.
    pub fn should_block(&self, host: &str, page_host: &str) -> bool {
        if !self.enabled.get() || host.is_empty() {
            return false;
        }
        // First-party requests are never blocked.
        if base_domain(host) == base_domain(page_host) {
            return false;
        }
        if self.exceptions.borrow().contains(page_host) {
            return false;
        }
        let Ok(domains) = self.domains.read() else { return false };
        // Check the host and each parent domain: ads.example.com, example.com.
        let mut rest = host;
        loop {
            if domains.contains(rest) {
                return true;
            }
            match rest.find('.') {
                Some(i) if rest[i + 1..].contains('.') => rest = &rest[i + 1..],
                _ => return false,
            }
        }
    }

    /// Count a blocked request made by the page at `page_url`. A different
    /// page in the same tab starts a new count.
    pub fn count_blocked(&self, webview: WebViewId, page_url: &str, request_url: &str) {
        let mut map = self.blocked.borrow_mut();
        let entry = map.entry(webview).or_insert_with(|| (page_url.to_string(), HashSet::new()));
        if entry.0 != page_url {
            *entry = (page_url.to_string(), HashSet::new());
        }
        entry.1.insert(request_url.to_string());
    }

    /// How many different addresses were blocked on the page at `page_url` in this tab.
    pub fn blocked_on(&self, webview: WebViewId, page_url: &str) -> u32 {
        self.blocked.borrow().get(&webview).filter(|(u, _)| u == page_url).map_or(0, |(_, set)| set.len() as u32)
    }

    /// A new page load in this tab starts the count again.
    pub fn reset_count(&self, webview: WebViewId) {
        self.blocked.borrow_mut().remove(&webview);
    }
}

fn add_list(domains: &RwLock<HashSet<String>>, text: &str) -> usize {
    let Ok(mut set) = domains.write() else { return 0 };
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        // Accept plain host lists and hosts-file lines ("0.0.0.0 example.com").
        let host = line.split_whitespace().last().unwrap_or("");
        if host.contains('.') && !host.contains('/') && host != "0.0.0.0" && host != "127.0.0.1" {
            set.insert(host.to_ascii_lowercase());
        }
    }
    set.len()
}

fn download_list() -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();
    let mut response = agent.get(LIST_URL).call().map_err(|e| e.to_string())?;
    let text = response.body_mut().with_config().limit(4 * 1024 * 1024).read_to_string().map_err(|e| e.to_string())?;
    if text.lines().filter(|l| l.contains('.')).count() < 100 {
        return Err("the downloaded list looks empty".into());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_domains() {
        assert_eq!(base_domain("www.example.com"), "example.com");
        assert_eq!(base_domain("a.b.example.com"), "example.com");
        assert_eq!(base_domain("news.bbc.co.uk"), "bbc.co.uk");
        assert_eq!(base_domain("example.com"), "example.com");
        assert_eq!(base_domain("localhost"), "localhost");
        assert_eq!(base_domain("192.168.1.10"), "192.168.1.10");
    }

    #[test]
    fn blocks_third_party_trackers_only() {
        let b = Blocker::new(true, &[]);
        assert!(b.should_block("www.google-analytics.com", "news.example.com"));
        assert!(b.should_block("stats.g.doubleclick.net", "example.org"));
        assert!(!b.should_block("cdn.example.com", "www.example.com"));
        assert!(!b.should_block("doubleclick.net", "www.doubleclick.net"));
        assert!(!b.should_block("example.com", "other.com"));
        b.exceptions.borrow_mut().insert("news.example.com".into());
        assert!(!b.should_block("www.google-analytics.com", "news.example.com"));
        b.enabled.set(false);
        assert!(!b.should_block("doubleclick.net", "example.org"));
    }

    #[test]
    fn parses_lists() {
        let set = RwLock::new(HashSet::new());
        add_list(&set, "# comment\nads.example.com\n0.0.0.0 tracker.example.net\n\nnot-a-host\n");
        let set = set.into_inner().unwrap();
        assert!(set.contains("ads.example.com"));
        assert!(set.contains("tracker.example.net"));
        assert_eq!(set.len(), 2);
    }
}
