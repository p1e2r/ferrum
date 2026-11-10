// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Turns whatever the user typed in the address bar into a URL.

use url::Url;

use crate::profile::SearchEngine;

pub fn input_to_url(input: &str, engine: SearchEngine) -> Option<Url> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    // Already a full URL (https://…, file://…, about:blank, data:…).
    if input.contains("://")
        || input.starts_with("about:")
        || input.starts_with("data:")
        || input.starts_with("file:")
    {
        if let Ok(url) = Url::parse(input) {
            return Some(url);
        }
    }

    // A local file path.
    if input.starts_with('/') || (input.len() > 2 && input.as_bytes()[1] == b':' && input.as_bytes()[2] == b'\\') {
        if let Ok(url) = Url::from_file_path(input) {
            return Some(url);
        }
    }

    // Looks like a host name: "example.com", "localhost:8080", "10.0.0.1/path".
    let looks_like_host = !input.contains(char::is_whitespace)
        && (input.contains('.') || input.starts_with("localhost"))
        && !input.starts_with('.')
        && !input.ends_with('.');
    if looks_like_host {
        let local = input.starts_with("localhost") || input.starts_with("127.") || input.starts_with("192.168.") || input.starts_with("10.");
        let scheme = if local { "http" } else { "https" };
        if let Ok(url) = Url::parse(&format!("{scheme}://{input}")) {
            if url.host_str().is_some_and(|h| h.contains('.') || h == "localhost") {
                return Some(url);
            }
        }
    }

    // Otherwise treat it as a search.
    Some(engine.search_url(input))
}

#[cfg(test)]
mod tests {
    use super::input_to_url;
    use crate::profile::SearchEngine;

    #[test]
    fn parses_inputs() {
        let e = SearchEngine::DuckDuckGo;
        assert_eq!(input_to_url("example.com", e).unwrap().as_str(), "https://example.com/");
        assert_eq!(input_to_url("localhost:3000", e).unwrap().as_str(), "http://localhost:3000/");
        assert_eq!(input_to_url("about:blank", e).unwrap().as_str(), "about:blank");
        assert!(input_to_url("rust servo engine", e).unwrap().as_str().contains("q=rust+servo+engine"));
        assert!(input_to_url("what is 3.5", e).unwrap().as_str().contains("duckduckgo"));
        assert!(input_to_url("   ", e).is_none());
    }
}
