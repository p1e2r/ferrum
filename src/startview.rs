// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! The start page with a video background.
//!
//! The start page is normally drawn by Ferrum (egui). Video is better left to
//! the system's web engine, which decodes it with the graphics hardware, so
//! with a video background the start page is a small page of Ferrum's own,
//! shown in one shared web view: the clock, title, search box and tiles over
//! the video.
//!
//! The engines don't reliably play video from a custom URL scheme (WebKitGTK
//! and WKWebView fetch media outside the scheme handler), so the page and
//! the video come from a tiny HTTP server inside Ferrum, listening only on
//! 127.0.0.1 and answering only under a random secret path:
//! `http://127.0.0.1:<port>/<secret>/start` and `…/video` (with byte ranges,
//! which video players ask for).
//!
//! The page talks to Ferrum like any page does, with `window.ipc`:
//! `start-go` (search or address), `start-open`, `start-remove`.

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// The id the start page uses for its messages (no tab has it).
pub const START_ID: u64 = u64::MAX;

/// Largest piece of video sent per request.
const CHUNK: u64 = 4 * 1024 * 1024;

/// The local server for the start page: its address, with the secret.
pub struct Server {
    base: String,
}

impl Server {
    /// Start serving `shared` on a free port on 127.0.0.1.
    pub fn start(shared: Shared) -> std::io::Result<Server> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let secret = secret();
        let base = format!("http://127.0.0.1:{port}/{secret}/");
        let prefix = format!("/{secret}/");
        std::thread::Builder::new().name("start-page".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = shared.clone();
                let prefix = prefix.clone();
                let _ = std::thread::Builder::new().name("start-page-request".into()).spawn(move || handle(stream, &prefix, &shared));
            }
        })?;
        Ok(Server { base })
    }

    pub fn url(&self) -> String {
        format!("{}start", self.base)
    }

    /// The server's address, with the secret, ending in `/`.
    pub fn base(&self) -> &str {
        &self.base
    }

}

/// The built-in PDF viewer (Linux) showing `src`, on the server at `base`.
pub fn pdf_viewer(base: &str, src: &url::Url) -> String {
    let q: String = url::form_urlencoded::Serializer::new(String::new()).append_pair("src", src.as_str()).finish();
    format!("{base}pdf?{q}")
}

/// The address to show for a page: a PDF in the built-in viewer shows the
/// PDF's own address.
pub fn shown_url(url: &url::Url) -> Option<url::Url> {
    if url.host_str() != Some("127.0.0.1") || !url.path().ends_with("/pdf") {
        return None;
    }
    url.query_pairs().find(|(k, _)| k == "src").and_then(|(_, v)| url::Url::parse(&v).ok())
}

/// Whether a URL is a PDF by its name.
pub fn is_pdf(url: &url::Url) -> bool {
    url.path().to_ascii_lowercase().ends_with(".pdf")
}

/// A random path part nobody else can guess (other programs and pages on
/// this computer can reach 127.0.0.1, but not without it).
fn secret() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for i in 0..2 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(i);
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        h.write_u32(std::process::id());
        out += &format!("{:016x}", h.finish());
    }
    out
}

/// One HTTP request (keep-alive, so a video's many range requests share a connection).
fn handle(stream: TcpStream, prefix: &str, shared: &Shared) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(30)));
    let Ok(mut write) = stream.try_clone() else { return };
    let mut read = BufReader::new(stream);
    loop {
        let mut line = String::new();
        if read.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let mut parts = line.split_whitespace();
        let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("").to_string());
        let mut range = None;
        loop {
            let mut h = String::new();
            if read.read_line(&mut h).unwrap_or(0) == 0 {
                return;
            }
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                if k.eq_ignore_ascii_case("range") {
                    range = Some(v.trim().to_string());
                }
            }
        }
        let head = method == "HEAD";
        let reply = match target.strip_prefix(prefix) {
            Some(path) if method == "GET" || head => serve(path, range.as_deref(), shared),
            _ => Reply::status(404),
        };
        if reply.write(&mut write, head).is_err() {
            return;
        }
    }
}

struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn status(status: u16) -> Reply {
        Reply { status, headers: Vec::new(), body: Vec::new() }
    }

    fn write(self, out: &mut TcpStream, head: bool) -> std::io::Result<()> {
        let reason = match self.status {
            200 => "OK",
            206 => "Partial Content",
            404 => "Not Found",
            416 => "Range Not Satisfiable",
            _ => "Error",
        };
        let mut text = format!("HTTP/1.1 {} {reason}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n", self.status, self.body.len());
        for (k, v) in &self.headers {
            text += &format!("{k}: {v}\r\n");
        }
        text += "\r\n";
        out.write_all(text.as_bytes())?;
        if !head {
            out.write_all(&self.body)?;
        }
        out.flush()
    }
}

/// What the page and the video handler serve; updated when settings change.
#[derive(Default)]
pub struct Served {
    pub html: String,
    pub video: Option<PathBuf>,
}

pub type Shared = Arc<Mutex<Served>>;

/// Everything shown on the page.
#[derive(Hash, PartialEq, Clone)]
pub struct StartData {
    pub clock: bool,
    pub clock_24h: bool,
    pub title: String,
    pub private: bool,
    pub search: bool,
    pub engine: String,
    pub bookmarks: Vec<(String, String)>,
    pub frequent: Vec<(String, String)>,
    /// How much the video is darkened, in percent.
    pub dim: u8,
    pub accent: [u8; 3],
    pub video: Option<PathBuf>,
}

/// Answer a request for `path` (after the secret, with any query).
fn serve(target: &str, range: Option<&str>, shared: &Shared) -> Reply {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let (html, video) = match shared.lock() {
        Ok(s) => (s.html.clone(), s.video.clone()),
        Err(_) => (String::new(), None),
    };
    match path {
        "pdf" => Reply { status: 200, headers: vec![("Content-Type", "text/html; charset=utf-8".into())], body: PDF_VIEWER.as_bytes().to_vec() },
        "pdf-data" => pdf_data(query),
        #[cfg(target_os = "linux")]
        "pdfjs/pdf.min.mjs" => script(include_bytes!("../assets/pdfjs/pdf.min.mjs")),
        #[cfg(target_os = "linux")]
        "pdfjs/pdf.worker.min.mjs" => script(include_bytes!("../assets/pdfjs/pdf.worker.min.mjs")),
        "start" => Reply { status: 200, headers: vec![("Content-Type", "text/html; charset=utf-8".into())], body: html.into_bytes() },
        "video" => match video {
            Some(file) => serve_video(range, &file),
            None => Reply::status(404),
        },
        _ => Reply::status(404),
    }
}

#[cfg(target_os = "linux")]
fn script(bytes: &'static [u8]) -> Reply {
    Reply { status: 200, headers: vec![("Content-Type", "text/javascript".into())], body: bytes.to_vec() }
}

/// The PDF itself for the viewer: a local file, or fetched from the web.
fn pdf_data(query: &str) -> Reply {
    let src = url::form_urlencoded::parse(query.as_bytes()).find(|(k, _)| k == "src").map(|(_, v)| v.into_owned());
    let Some(src) = src.and_then(|s| url::Url::parse(&s).ok()) else { return Reply::status(404) };
    let body = match src.scheme() {
        // Only PDFs, never other files.
        "file" if is_pdf(&src) => src.to_file_path().ok().and_then(|p| std::fs::read(p).ok()),
        "http" | "https" => {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .tls_config(crate::tls_config())
                .user_agent(format!("Ferrum/{}", crate::VERSION))
                .timeout_global(Some(std::time::Duration::from_secs(120)))
                .build()
                .into();
            agent.get(src.as_str()).call().ok().and_then(|mut r| r.body_mut().with_config().limit(300 * 1024 * 1024).read_to_vec().ok())
        },
        _ => None,
    };
    match body {
        Some(body) => Reply { status: 200, headers: vec![("Content-Type", "application/pdf".into())], body },
        None => Reply::status(404),
    }
}

fn video_type(file: &std::path::Path) -> &'static str {
    match file.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("m4v") => "video/x-m4v",
        Some("ogv") => "video/ogg",
        _ => "video/mp4",
    }
}

/// The video file, a byte range at a time.
fn serve_video(range: Option<&str>, file: &std::path::Path) -> Reply {
    let Ok(mut f) = std::fs::File::open(file) else { return Reply::status(404) };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if len == 0 {
        return Reply::status(404);
    }
    let asked = range.and_then(|r| parse_range(r, len));
    if range.is_some() && asked.is_none() {
        return Reply { status: 416, headers: vec![("Content-Range", format!("bytes */{len}"))], body: Vec::new() };
    }
    let (start, end) = asked.unwrap_or((0, len - 1));
    let end = end.min(start + CHUNK - 1).min(len - 1);
    if start > end {
        return Reply { status: 416, headers: vec![("Content-Range", format!("bytes */{len}"))], body: Vec::new() };
    }
    let mut body = vec![0u8; (end - start + 1) as usize];
    if f.seek(SeekFrom::Start(start)).is_err() || f.read_exact(&mut body).is_err() {
        return Reply::status(500);
    }
    // Always answer with a range, so players know they can ask for more.
    Reply {
        status: 206,
        headers: vec![
            ("Content-Type", video_type(file).into()),
            ("Accept-Ranges", "bytes".into()),
            ("Content-Range", format!("bytes {start}-{end}/{len}")),
        ],
        body,
    }
}

/// `bytes=a-b`, `bytes=a-` or `bytes=-n`, as an inclusive (start, end).
fn parse_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        return Some((len.saturating_sub(n), len - 1));
    }
    let start: u64 = a.parse().ok()?;
    let end = if b.is_empty() { len - 1 } else { b.parse::<u64>().ok()?.min(len - 1) };
    Some((start, end))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// The same colours as Ferrum's letter icons.
fn icon_color(seed: &str) -> String {
    let hash = seed.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    format!("hsl({}deg 45% 55%)", hash % 360)
}

fn letter(title: &str) -> String {
    let t = title.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.");
    t.chars().find(|c| c.is_alphanumeric()).unwrap_or('•').to_uppercase().to_string()
}

fn tiles(heading: &str, items: &[(String, String)], bookmark: bool) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut out = format!("<h2>{}</h2><div class=tiles>", esc(heading));
    for (title, url) in items {
        let shown = if title.trim().is_empty() { url } else { title };
        let short: String = if shown.chars().count() > 14 { format!("{}…", shown.chars().take(13).collect::<String>()) } else { shown.clone() };
        out += &format!(
            "<a class=tile href=\"{u}\" data-url=\"{u}\" data-bookmark=\"{b}\" title=\"{u}\"><span class=icon style=\"background:{c}\">{l}</span><span class=name>{t}</span><button class=x title=\"{x}\">×</button></a>",
            u = esc(url),
            b = bookmark,
            c = icon_color(url),
            l = esc(&letter(shown)),
            t = esc(&short),
            x = if bookmark { "Remove bookmark" } else { "Remove from this page" },
        );
    }
    out + "</div>"
}

pub fn html(d: &StartData) -> String {
    let [r, g, b] = d.accent;
    let search = if d.search {
        format!("<form id=search><input id=q autocomplete=off spellcheck=false placeholder=\"Search with {} or type an address\"></form>", esc(&d.engine))
    } else {
        String::new()
    };
    let private = if d.private { "<p class=note>Private window — history, cookies and site data are forgotten when it closes</p>" } else { "" };
    let clock = if d.clock { "<div id=time></div><div id=date></div>" } else { "" };
    let video = if d.video.is_some() { "<video id=bg autoplay muted loop playsinline src=\"video\"></video>" } else { "" };
    format!(
        r##"<!doctype html><html><head><meta charset=utf-8><title>New Tab</title><meta name=color-scheme content="dark">
<style>
html,body{{margin:0;height:100%;overflow:hidden;background:#111;color:#fff;font:15px -apple-system,BlinkMacSystemFont,"Segoe UI",Ubuntu,Cantarell,sans-serif}}
#bg{{position:fixed;inset:0;width:100%;height:100%;object-fit:cover;z-index:0}}
#dim{{position:fixed;inset:0;background:rgba(0,0,0,{dim});z-index:1}}
main{{position:relative;z-index:2;height:100%;overflow:auto;display:flex;flex-direction:column;align-items:center;padding:{top}vh 20px 40px;box-sizing:border-box}}
#time{{font-size:64px;font-weight:300;text-shadow:0 2px 16px rgba(0,0,0,.4)}}
#date{{font-size:15px;opacity:.85;margin-bottom:14px}}
h1{{font-size:{h1}px;margin:4px 0 22px;font-weight:700;text-shadow:0 2px 18px rgba(0,0,0,.45)}}
.note{{opacity:.8;margin:-12px 0 20px}}
#search{{width:min(760px,90vw);margin-bottom:28px}}
#q{{width:100%;box-sizing:border-box;padding:13px 18px;font:inherit;font-size:17px;color:#fff;border-radius:999px;border:1px solid rgba(255,255,255,.25);background:rgba(20,20,24,.45);-webkit-backdrop-filter:blur(14px);backdrop-filter:blur(14px);outline:none}}
#q:focus{{border-color:rgb({r},{g},{b});box-shadow:0 0 0 3px rgba({r},{g},{b},.35)}}
#q::placeholder{{color:rgba(255,255,255,.65)}}
h2{{font-size:13px;font-weight:600;opacity:.8;margin:8px 0 6px;text-shadow:0 1px 8px rgba(0,0,0,.5)}}
.tiles{{display:flex;flex-wrap:wrap;justify-content:center;max-width:840px;margin-bottom:14px}}
.tile{{position:relative;width:100px;height:88px;margin:2px;border-radius:14px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:7px;color:#fff;text-decoration:none}}
.tile:hover{{background:rgba(255,255,255,.16);-webkit-backdrop-filter:blur(10px);backdrop-filter:blur(10px)}}
.icon{{width:40px;height:40px;border-radius:10px;display:flex;align-items:center;justify-content:center;font-size:23px;box-shadow:0 2px 10px rgba(0,0,0,.25)}}
.name{{font-size:12px;max-width:92px;overflow:hidden;white-space:nowrap;text-shadow:0 1px 6px rgba(0,0,0,.6)}}
.x{{display:none;position:absolute;top:4px;right:4px;width:20px;height:20px;border:0;border-radius:50%;background:rgba(0,0,0,.45);color:#fff;font-size:14px;line-height:20px;padding:0;cursor:pointer}}
.tile:hover .x{{display:block}}
.x:hover{{background:rgba(0,0,0,.75)}}
</style></head><body>
{video}<div id=dim></div>
<main>{clock}<h1>{title}</h1>{private}{search}{bookmarks}{frequent}</main>
<script>
(function(){{
  var post=function(m){{try{{window.ipc.postMessage(JSON.stringify(m))}}catch(e){{}}}};
  var h24={h24};
  function tick(){{
    var t=document.getElementById("time"); if(!t) return;
    var d=new Date();
    var hh=d.getHours(), mm=("0"+d.getMinutes()).slice(-2);
    t.textContent=h24?(("0"+hh).slice(-2)+":"+mm):(((hh%12)||12)+":"+mm+" "+(hh<12?"AM":"PM"));
    document.getElementById("date").textContent=d.toLocaleDateString(undefined,{{weekday:"long",day:"numeric",month:"long"}});
  }}
  tick(); setInterval(tick,1000);
  var f=document.getElementById("search");
  if(f) f.addEventListener("submit",function(e){{e.preventDefault();var q=document.getElementById("q").value.trim();if(q) post({{t:"start-go",q:q}});}});
  document.querySelectorAll(".tile").forEach(function(a){{
    a.addEventListener("click",function(e){{
      e.preventDefault();
      if(e.target.classList.contains("x")){{post({{t:"start-remove",url:a.dataset.url,bookmark:a.dataset.bookmark==="true"}});a.remove();return;}}
      post({{t:"start-open",url:a.dataset.url,newTab:e.metaKey||e.ctrlKey}});
    }});
    a.addEventListener("auxclick",function(e){{if(e.button===1){{e.preventDefault();post({{t:"start-open",url:a.dataset.url,newTab:true}});}}}});
  }});
  // Don't spend anything on the video while the page isn't shown.
  var v=document.getElementById("bg");
  if(v) document.addEventListener("visibilitychange",function(){{document.hidden?v.pause():v.play().catch(function(){{}})}});
}})();
</script></body></html>"##,
        dim = d.dim.min(90) as f32 / 100.0,
        top = if d.clock { 10 } else { 16 },
        h1 = if d.clock { 34 } else { 52 },
        title = esc(&d.title),
        bookmarks = tiles("Bookmarks", &d.bookmarks, true),
        frequent = tiles("Frequently visited", &d.frequent, false),
        h24 = d.clock_24h,
    )
}

/// The PDF viewer page (Linux, where WebKitGTK has no PDF viewer), using pdf.js.
const PDF_VIEWER: &str = r#"<!doctype html><html><head><meta charset=utf-8><title>PDF</title><meta name=color-scheme content="dark light">
<style>
html,body{margin:0;background:#525659;font:14px -apple-system,"Segoe UI",Ubuntu,Cantarell,sans-serif;color:#eee}
#bar{position:fixed;top:0;left:0;right:0;height:42px;display:flex;align-items:center;gap:10px;padding:0 14px;background:#323639;box-shadow:0 1px 4px rgba(0,0,0,.4);z-index:2}
#name{flex:1;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
button{background:#4a4e51;color:#eee;border:0;border-radius:8px;padding:5px 11px;font:inherit;cursor:pointer}
button:hover{background:#5d6164}
#pages{padding:56px 0 20px;display:flex;flex-direction:column;align-items:center;gap:12px}
.page{background:#fff;box-shadow:0 2px 10px rgba(0,0,0,.4);position:relative}
.page canvas{display:block;width:100%;height:100%}
#msg{margin-top:120px;text-align:center;color:#ddd}
</style></head><body>
<div id=bar><span id=name></span><span id=count></span><button id=out title="Zoom out">−</button><span id=zoom>100%</span><button id=in title="Zoom in">+</button><button id=fit>Fit width</button><button id=dl>Download</button></div>
<div id=pages><div id=msg>Loading PDF…</div></div>
<script type=module>
import * as pdfjs from "./pdfjs/pdf.min.mjs";
pdfjs.GlobalWorkerOptions.workerSrc = "./pdfjs/pdf.worker.min.mjs";
const src = new URLSearchParams(location.search).get("src") || "";
const name = decodeURIComponent(src.split("/").pop().split("?")[0] || "PDF");
document.title = name; document.getElementById("name").textContent = name;
document.getElementById("dl").onclick = () => { try { window.ipc.postMessage(JSON.stringify({t:"download", url:src})) } catch (e) {} };
let doc, scale = 0, pages = [];
const box = document.getElementById("pages");
const observer = new IntersectionObserver(es => es.forEach(e => { if (e.isIntersecting) draw(e.target) }), {rootMargin: "800px"});
async function draw(div) {
  if (div.dataset.drawn == scale) return;
  div.dataset.drawn = scale;
  const page = await doc.getPage(+div.dataset.n);
  const vp = page.getViewport({scale: scale * devicePixelRatio});
  const c = document.createElement("canvas"); c.width = vp.width; c.height = vp.height;
  await page.render({canvasContext: c.getContext("2d"), viewport: vp}).promise;
  div.replaceChildren(c);
}
async function layout() {
  document.getElementById("zoom").textContent = Math.round(scale * 100) + "%";
  for (const div of pages) {
    const page = await doc.getPage(+div.dataset.n);
    const vp = page.getViewport({scale});
    div.style.width = vp.width + "px"; div.style.height = vp.height + "px";
    div.dataset.drawn = "";
  }
  pages.forEach(d => { observer.unobserve(d); observer.observe(d) });
}
async function fitWidth() {
  const page = await doc.getPage(1);
  scale = Math.min(2.5, (innerWidth - 40) / page.getViewport({scale: 1}).width);
  scale = Math.max(0.25, Math.min(scale, 1.6));
  layout();
}
document.getElementById("in").onclick = () => { scale = Math.min(5, scale * 1.2); layout() };
document.getElementById("out").onclick = () => { scale = Math.max(0.25, scale / 1.2); layout() };
document.getElementById("fit").onclick = fitWidth;
try {
  doc = await pdfjs.getDocument({url: "./pdf-data?src=" + encodeURIComponent(src)}).promise;
  document.getElementById("count").textContent = doc.numPages + (doc.numPages == 1 ? " page" : " pages");
  box.replaceChildren();
  for (let n = 1; n <= doc.numPages; n++) {
    const div = document.createElement("div"); div.className = "page"; div.dataset.n = n;
    box.appendChild(div); pages.push(div);
  }
  await fitWidth();
} catch (e) {
  document.getElementById("msg").textContent = "Couldn't open this PDF (" + e.message + ").";
}
</script></body></html>"#;

/// The file types the video picker offers.
pub const VIDEO_TYPES: &[&str] = &["mp4", "m4v", "mov", "webm", "ogv"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_page_and_video_ranges_only_with_the_secret() {
        let dir = std::env::temp_dir().join(format!("ferrum-start-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("v.mp4");
        std::fs::write(&file, (0..=255u8).cycle().take(10_000).collect::<Vec<u8>>()).unwrap();
        let shared: Shared = Arc::new(Mutex::new(Served { html: "<p>hi</p>".into(), video: Some(file) }));
        let server = Server::start(shared).unwrap();
        let get = |url: &str, range: Option<&str>| {
            let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).proxy(None).build().into();
            let mut req = agent.get(url);
            if let Some(r) = range {
                req = req.header("Range", r);
            }
            let mut resp = req.call().unwrap();
            let status = resp.status().as_u16();
            let cr = resp.headers().get("content-range").map(|v| v.to_str().unwrap().to_string());
            (status, cr, resp.body_mut().read_to_vec().unwrap())
        };
        let (s, _, body) = get(&server.url(), None);
        assert_eq!((s, body.as_slice()), (200, &b"<p>hi</p>"[..]));
        let video = server.url().replace("/start", "/video");
        let (s, cr, body) = get(&video, Some("bytes=100-199"));
        assert_eq!((s, cr.as_deref(), body.len(), body[0]), (206, Some("bytes 100-199/10000"), 100, 100));
        let (s, cr, _) = get(&video, None);
        assert_eq!((s, cr.as_deref()), (206, Some("bytes 0-9999/10000")));
        // Without the secret: nothing.
        let wrong = format!("http://127.0.0.1:{}/start", server.base.split(':').nth(2).unwrap().split('/').next().unwrap());
        assert_eq!(get(&wrong, None).0, 404);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-", 1000), Some((0, 999)));
        assert_eq!(parse_range("bytes=100-199", 1000), Some((100, 199)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=900-5000", 1000), Some((900, 999)));
        assert_eq!(parse_range("items=1-2", 1000), None);
    }

    #[test]
    fn page_escapes_titles() {
        let d = StartData {
            clock: true,
            clock_24h: true,
            title: "<b>x</b>".into(),
            private: false,
            search: true,
            engine: "DuckDuckGo".into(),
            bookmarks: vec![("A \"quote\"".into(), "https://a.example/".into())],
            frequent: vec![],
            dim: 30,
            accent: [1, 2, 3],
            video: None,
        };
        let h = html(&d);
        assert!(h.contains("&lt;b&gt;x&lt;/b&gt;"));
        assert!(h.contains("A &quot;quote&quot;"));
        assert!(!h.contains("<video"));
    }
}
