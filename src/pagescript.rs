// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! The script Ferrum adds to every page before the page's own scripts run.
//!
//! The web engine draws the page in its own native view, which takes the
//! keyboard and mouse while the page is focused. The script passes Ferrum's
//! shortcuts (Ctrl+T, Ctrl+L, F12…) and the mouse back/forward buttons to the
//! browser, reports the page's icon and whether audio or video is playing,
//! and blocks requests to known ad and tracking servers on other sites.
//! Messages go to Ferrum with `window.ipc.postMessage` (see browser.rs).

/// Keys Ferrum handles even while a page has focus, with Ctrl (⌘ on macOS).
const MOD_KEYS: &str = "tTwWnNlLrRfFgGdDbBhHjJoOpPuU,=+-0123456789/[]mMiIsS";

pub fn script(block_trackers: bool, blocklist: &[String], exceptions: &[String], notifications: &[(String, bool)], allow_notifications: bool, force_dark: bool, fps30: bool) -> String {
    let notifications = serde_json::to_string(notifications).unwrap_or_else(|_| "[]".into());
    let list = serde_json::to_string(blocklist).unwrap_or_else(|_| "[]".into());
    let exceptions = serde_json::to_string(exceptions).unwrap_or_else(|_| "[]".into());
    let mac = cfg!(target_os = "macos");
    format!(
        r#"(function () {{
  if (window.__ferrum) return;
  window.__ferrum = true;
  const post = (m) => {{ try {{ window.ipc.postMessage(JSON.stringify(m)); }} catch (e) {{}} }};
  const top = window === window.top;

  // ----- energy saver: animations at 30 frames a second -----
  // requestAnimationFrame callbacks run at most every other frame; callbacks
  // for the same frame still all run together.
  {{
    let cap = {fps30};
    const raf = window.requestAnimationFrame.bind(window);
    const caf = window.cancelAnimationFrame.bind(window);
    const ids = new Map();
    let next = 1, last = -1e9;
    window.requestAnimationFrame = function (cb) {{
      if (!cap) return raf(cb);
      const id = -(next++);
      const tick = (t) => {{
        if (t - last >= 32 || t === last) {{ last = t; ids.delete(id); cb(t); }}
        else ids.set(id, raf(tick));
      }};
      ids.set(id, raf(tick));
      return id;
    }};
    window.cancelAnimationFrame = function (id) {{
      if (ids.has(id)) {{ caf(ids.get(id)); ids.delete(id); }} else caf(id);
    }};
    window.__ferrumFps30 = (on) => {{ cap = on; }};
  }}

  // ----- dark mode for sites without one -----
  // When it's on and the page is still light, invert it (and invert images
  // and video back), like Chrome's "auto dark mode".
  let forceDark = {force_dark};
  const lum = (c) => {{
    const m = (c || "").match(/[\d.]+/g);
    if (!m || (m.length > 3 && +m[3] < 0.5)) return null;
    return (0.299 * m[0] + 0.587 * m[1] + 0.114 * m[2]) / 255;
  }};
  const bgAt = (el) => {{
    for (; el && el.nodeType === 1; el = el.parentElement) {{
      const l = lum(getComputedStyle(el).backgroundColor);
      if (l !== null) return l;
    }}
    return null;
  }};
  // Only darken pages that are clearly light: light background at almost
  // every sampled point and dark text. Pages with their own dark look are left alone.
  const pageIsLight = () => {{
    const root = document.documentElement, body = document.body;
    if (!root || !body) return false;
    const text = lum(getComputedStyle(body).color);
    if (text !== null && text > 0.55) return false;
    const scheme = getComputedStyle(root).colorScheme || "";
    let light = 0;
    const points = [[0.5, 0.5], [0.25, 0.3], [0.75, 0.3], [0.25, 0.75], [0.75, 0.75], [0.5, 0.12]];
    for (const [x, y] of points) {{
      let el = document.elementFromPoint(innerWidth * x, innerHeight * y);
      if (el && /^(img|video|canvas|picture|svg|iframe)$/i.test(el.tagName)) el = el.parentElement;
      let l = bgAt(el);
      if (l === null) l = scheme.includes("dark") ? 0 : 1;
      if (l > 0.7) light++;
    }}
    return light >= points.length - 1;
  }};
  const darkStyle = () => {{
    if (!top || !document.documentElement) return;
    let style = document.getElementById("__ferrum_dark");
    if (!forceDark) {{
      if (style) style.remove();
      return;
    }}
    // Measure the page as it is, without our filter.
    if (style) style.disabled = true;
    const light = pageIsLight();
    if (style) style.disabled = false;
    if (light && !style) {{
      style = document.createElement("style");
      style.id = "__ferrum_dark";
      style.textContent = "html{{filter:invert(1) hue-rotate(180deg)!important;background:#fff!important}}" +
        "img,video,picture,canvas,iframe,embed,object,[style*='background-image']{{filter:invert(1) hue-rotate(180deg)!important}}";
      (document.head || document.documentElement).appendChild(style);
    }} else if (!light && style) {{
      style.remove();
    }}
  }};
  window.__ferrumDark = (on) => {{ forceDark = on; darkStyle(); }};
  document.addEventListener("DOMContentLoaded", darkStyle);
  addEventListener("load", () => {{
    darkStyle();
    setTimeout(darkStyle, 1500);
    setTimeout(darkStyle, 4000);
  }});

  // ----- browser shortcuts -----
  const MAC = {mac};
  const MODS = new Set({mod_keys:?}.split(""));
  window.addEventListener("keydown", (e) => {{
    const cmd = MAC ? e.metaKey : e.ctrlKey;
    const fkey = /^F([1-9]|1[0-2])$/.test(e.key) && !["F1", "F2", "F4", "F7", "F8", "F10"].includes(e.key);
    const nav = e.altKey && !cmd && (e.key === "ArrowLeft" || e.key === "ArrowRight");
    const tab = e.ctrlKey && e.key === "Tab";
    if ((cmd && MODS.has(e.key)) || fkey || nav || tab) {{
      e.preventDefault();
      e.stopImmediatePropagation();
      post({{ t: "key", key: e.key, ctrl: e.ctrlKey, meta: e.metaKey, shift: e.shiftKey, alt: e.altKey }});
    }} else if (e.key === "Escape") {{
      // Closes Ferrum's menus; the page still gets it too.
      post({{ t: "escape" }});
    }}
  }}, true);
  // Mouse back / forward buttons.
  window.addEventListener("mouseup", (e) => {{
    if (e.button === 3) {{ e.preventDefault(); history.back(); }}
    if (e.button === 4) {{ e.preventDefault(); history.forward(); }}
  }}, true);

  if (!top) return;

  // ----- linking a Google account (profiles.rs) -----
  // On Google's pages, report which account is signed in (from the account
  // button), so a profile can show it. Nothing leaves this computer.
  if (/(^|\.)google\.[a-z.]+$/.test(location.hostname)) {{
    const report = () => {{
      const a = document.querySelector('a[href*="accounts.google.com/SignOutOptions"]') ||
        Array.from(document.querySelectorAll("[aria-label]")).find((e) => /\(.+@.+\)/.test(e.getAttribute("aria-label")));
      const label = a && a.getAttribute("aria-label");
      if (!label) return;
      const img = a.querySelector("img");
      post({{ t: "google-account", label: label, picture: img ? img.src : null }});
    }};
    addEventListener("load", () => setTimeout(report, 1000));
  }}

  // A click on the page closes Ferrum's menus and popups.
  window.addEventListener("mousedown", () => post({{ t: "click" }}), true);

  // ----- web notifications -----
  // The system engines don't all support the Notification API, so Ferrum
  // provides it: Ferrum asks the user, then shows the notification itself.
  {{
    const remembered = new Map({notifications});
    let state = remembered.has(location.hostname) ? (remembered.get(location.hostname) ? "granted" : "denied") : ({allow_notifications} ? "default" : "denied");
    const waiting = [];
    window.__ferrumPermission = (answer) => {{ state = answer; while (waiting.length) waiting.shift()(answer); }};
    class FerrumNotification extends EventTarget {{
      constructor(title, options = {{}}) {{
        super();
        this.title = String(title);
        this.body = options.body ? String(options.body) : "";
        this.tag = options.tag || "";
        this.icon = options.icon || "";
        this.onclick = this.onshow = this.onclose = this.onerror = null;
        setTimeout(() => {{
          if (state !== "granted") {{ this.dispatchEvent(new Event("error")); if (this.onerror) this.onerror(new Event("error")); return; }}
          post({{ t: "notify", title: this.title, body: this.body }});
          this.dispatchEvent(new Event("show"));
          if (this.onshow) this.onshow(new Event("show"));
        }}, 0);
      }}
      close() {{}}
      static get permission() {{ return state; }}
      static requestPermission(callback) {{
        return new Promise((resolve) => {{
          const done = (answer) => {{ if (callback) callback(answer); resolve(answer); }};
          if (state !== "default") return done(state);
          waiting.push(done);
          post({{ t: "permission" }});
        }});
      }}
    }}
    FerrumNotification.maxActions = 0;
    try {{ Object.defineProperty(window, "Notification", {{ value: FerrumNotification, configurable: true, writable: true }}); }} catch (e) {{}}
  }}

  // ----- page icon -----
  const icon = () => {{
    const link = document.querySelector("link[rel~='icon'][href], link[rel='shortcut icon'][href], link[rel='apple-touch-icon'][href]");
    const href = link ? link.href : location.origin + "/favicon.ico";
    if (href.startsWith("http")) post({{ t: "icon", href }});
  }};
  window.addEventListener("load", icon);

  // ----- playing audio / video -----
  let playing = false;
  const media = () => {{
    const now = [...document.querySelectorAll("video, audio")].some((m) => !m.paused && !m.ended && !m.muted && m.volume > 0);
    if (now !== playing) {{ playing = now; post({{ t: "media", playing }}); }}
  }};
  for (const ev of ["play", "playing", "pause", "ended", "volumechange", "emptied"]) document.addEventListener(ev, media, true);

  // ----- ad and tracker blocking -----
  if (!{block_trackers}) return;
  const base = (h) => {{
    const p = h.replace(/\.$/, "").split(".");
    if (p.length <= 2 || /^\d+$/.test(p[p.length - 1])) return h;
    const two = p[p.length - 1].length === 2 && ["co", "com", "org", "net", "gov", "ac", "edu", "ne", "or"].includes(p[p.length - 2]);
    return p.slice(two ? -3 : -2).join(".");
  }};
  const page = location.hostname;
  if ({exceptions}.includes(page)) return;
  const LIST = new Set({list});
  const blocked = new Set();
  const listed = (url) => {{
    let u;
    try {{ u = new URL(url, location.href); }} catch (e) {{ return false; }}
    if (!u.protocol.startsWith("http")) return false;
    const host = u.hostname.toLowerCase();
    if (base(host) === base(page)) return false;
    let h = host;
    for (;;) {{
      if (LIST.has(h)) {{
        if (!blocked.has(u.href)) {{ blocked.add(u.href); post({{ t: "blocked", count: blocked.size }}); }}
        return true;
      }}
      const i = h.indexOf(".");
      if (i < 0 || h.indexOf(".", i + 1) < 0) return false;
      h = h.slice(i + 1);
    }}
  }};
  // Scripts, frames and images added to the page.
  const guard = (el) => {{
    if (!el || !el.tagName) return;
    const src = el.getAttribute && (el.getAttribute("src") || (el.tagName === "LINK" && el.getAttribute("href")));
    if (src && listed(src)) {{
      if (el.tagName === "SCRIPT") el.type = "javascript/blocked";
      el.removeAttribute("src");
      if (el.tagName === "IFRAME" || el.tagName === "IMG") el.style.display = "none";
      if (el.tagName === "LINK") el.removeAttribute("href");
    }}
  }};
  new MutationObserver((records) => {{
    for (const r of records) for (const n of r.addedNodes) {{
      if (n.nodeType !== 1) continue;
      guard(n);
      if (n.querySelectorAll) n.querySelectorAll("script[src], iframe[src], img[src]").forEach(guard);
    }}
  }}).observe(document.documentElement, {{ childList: true, subtree: true }});
  const wrapSrc = (proto, attr) => {{
    const d = Object.getOwnPropertyDescriptor(proto, attr);
    if (!d || !d.set) return;
    Object.defineProperty(proto, attr, {{ ...d, set(v) {{ if (listed(String(v))) return; d.set.call(this, v); }} }});
  }};
  wrapSrc(HTMLScriptElement.prototype, "src");
  wrapSrc(HTMLImageElement.prototype, "src");
  wrapSrc(HTMLIFrameElement.prototype, "src");
  const setAttr = Element.prototype.setAttribute;
  Element.prototype.setAttribute = function (name, value) {{
    if ((name === "src" || name === "href") && /^(SCRIPT|IMG|IFRAME)$/.test(this.tagName) && listed(String(value))) return;
    return setAttr.call(this, name, value);
  }};
  // Requests made from scripts.
  const realFetch = window.fetch;
  window.fetch = function (input, init) {{
    const url = typeof input === "string" ? input : input && input.url;
    if (url && listed(url)) return Promise.reject(new TypeError("Blocked by Ferrum"));
    return realFetch.call(this, input, init);
  }};
  const open = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (method, url) {{
    if (listed(String(url))) {{ this.__ferrumBlocked = true; }}
    return open.apply(this, arguments);
  }};
  const send = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.send = function () {{
    if (this.__ferrumBlocked) {{ try {{ this.abort(); }} catch (e) {{}} return; }}
    return send.apply(this, arguments);
  }};
  if (navigator.sendBeacon) {{
    const beacon = navigator.sendBeacon.bind(navigator);
    navigator.sendBeacon = (url, data) => (listed(url) ? true : beacon(url, data));
  }}
}})();"#,
        mod_keys = MOD_KEYS,
    )
}

/// Mutes or unmutes every `<audio>`/`<video>`, including ones that start later.
pub fn mute(muted: bool) -> String {
    format!(
        "(function(m){{window.__ferrumMuted=m;document.querySelectorAll('video,audio').forEach(function(e){{e.muted=m}});\
         if(!window.__ferrumMuteHook){{window.__ferrumMuteHook=true;document.addEventListener('play',function(e){{\
         if(window.__ferrumMuted&&'muted' in e.target)e.target.muted=true}},true)}}}})({muted})"
    )
}
