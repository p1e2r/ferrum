// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Small fixes for sites written for Chrome/Firefox, injected into every page
//! as a user script.
//!
//! **Range sliders.** Sites style `<input type=range>` with Chrome's
//! `::-webkit-slider-thumb` / `::-webkit-slider-runnable-track` or Firefox's
//! `::-moz-range-*` pseudo-elements. Servo calls these parts `::slider-thumb`,
//! `::slider-track` and `::slider-fill`, drops the vendor rules, and draws its
//! own default slider on top of the site's design. The script finds those
//! rules in the page's stylesheets, renames the pseudo-elements and adds them
//! back, and, when the site turned the native look off (`appearance: none`),
//! hides Servo's default track and fill as Chrome does.

pub const SCRIPT: &str = r#"(function () {
  if (window.__ferrumCompat) return;
  window.__ferrumCompat = true;
  const VENDOR = /::?-(webkit-slider-thumb|moz-range-thumb|webkit-slider-runnable-track|moz-range-track|moz-range-progress)/;
  const RENAME = [
    [/::?-webkit-slider-thumb/g, "::slider-thumb"],
    [/::?-moz-range-thumb/g, "::slider-thumb"],
    [/::?-webkit-slider-runnable-track/g, "::slider-track"],
    [/::?-moz-range-track/g, "::slider-track"],
    [/::?-moz-range-progress/g, "::slider-fill"],
  ];
  const seen = new Set();
  let out = "";
  let nativeOff = false;
  function convert(css) {
    if (!VENDOR.test(css) || seen.has(css)) return;
    seen.add(css);
    if (/appearance\s*:\s*none/.test(css)) nativeOff = true;
    const rule = /([^{}]+)\{([^{}]*)\}/g;
    let m;
    while ((m = rule.exec(css))) {
      const parts = m[1].split(",").map((s) => s.trim()).filter((s) => VENDOR.test(s));
      if (!parts.length) continue;
      let selector = parts.join(", ");
      for (const [from, to] of RENAME) selector = selector.replace(from, to);
      // Chrome needs a negative margin-top to centre the thumb; Servo centres it itself.
      const body = m[2].replace(/margin-top\s*:[^;]*;?/g, "").replace(/-webkit-appearance\s*:[^;]*;?/g, "");
      out += selector + "{" + body + "}\n";
    }
  }
  function apply() {
    if (!out) return;
    let style = document.getElementById("__ferrum-compat");
    if (!style) {
      style = document.createElement("style");
      style.id = "__ferrum-compat";
      (document.head || document.documentElement).appendChild(style);
    }
    const base = nativeOff
      ? "input[type=range]::slider-track, input[type=range]::slider-fill { background: transparent; border-color: transparent; }\n"
      : "";
    style.textContent = base + out;
  }
  function scan() {
    if (!document.querySelector("input[type=range]")) return;
    for (const el of document.querySelectorAll("style")) convert(el.textContent || "");
    const links = [...document.querySelectorAll("link[rel~=stylesheet][href]")];
    apply();
    Promise.all(
      links.map((l) => fetch(l.href).then((r) => (r.ok ? r.text() : "")).then(convert).catch(() => {}))
    ).then(apply);
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", scan);
  else scan();
  window.addEventListener("load", () => { scan(); setTimeout(scan, 3000); });
})();"#;
