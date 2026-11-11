// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Bookmark import and export in the Netscape bookmark file format, which
//! Firefox, Chrome, Edge and Safari all read and write.

use crate::profile::Bookmark;

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

pub fn export(bookmarks: &[Bookmark]) -> String {
    let mut out = String::from(
        "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<!-- This is an automatically generated file. -->\n\
         <META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n\
         <TITLE>Bookmarks</TITLE>\n<H1>Bookmarks</H1>\n<DL><p>\n",
    );
    for b in bookmarks {
        out.push_str(&format!("    <DT><A HREF=\"{}\">{}</A>\n", escape(&b.url), escape(&b.title)));
    }
    out.push_str("</DL><p>\n");
    out
}

/// Every link in a bookmarks file (folders are flattened).
pub fn import(html: &str) -> Vec<Bookmark> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(start) = lower[pos..].find("<a ") {
        let tag_start = pos + start;
        let Some(tag_end) = lower[tag_start..].find('>').map(|e| tag_start + e) else { break };
        let tag = &html[tag_start..tag_end];
        let tag_lower = &lower[tag_start..tag_end];
        let close = lower[tag_end..].find("</a>").map(|e| tag_end + e).unwrap_or(tag_end + 1);
        pos = close.max(tag_end + 1);
        let Some(href_at) = tag_lower.find("href=") else { continue };
        let rest = &tag[href_at + 5..];
        let url = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
            _ => rest.split_whitespace().next().unwrap_or(""),
        };
        let url = unescape(url);
        if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file:")) {
            continue;
        }
        let title = unescape(html.get(tag_end + 1..close).unwrap_or("").trim());
        let title = if title.is_empty() { url.clone() } else { title };
        out.push(Bookmark { title, url });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let list = vec![
            Bookmark { title: "A & B".into(), url: "https://a.example/?x=1&y=2".into() },
            Bookmark { title: "<Two>".into(), url: "https://b.example/".into() },
        ];
        let back = import(&export(&list));
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].title, "A & B");
        assert_eq!(back[0].url, "https://a.example/?x=1&y=2");
        assert_eq!(back[1].title, "<Two>");
    }

    #[test]
    fn reads_browser_exports() {
        let html = r#"<DL><p><DT><H3>Folder</H3><DL><p>
            <DT><A HREF="https://rust-lang.org/" ADD_DATE="1" ICON="data:x">Rust</A>
            <DT><a href='https://servo.org'>Servo</a>
            <DT><A HREF="javascript:alert(1)">Bad</A>
            <DT><A HREF="place:sort=8">Smart</A></DL></DL>"#;
        let list = import(html);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].url, "https://rust-lang.org/");
        assert_eq!(list[1].title, "Servo");
    }
}
