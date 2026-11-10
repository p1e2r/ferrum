// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Find in page.
//!
//! Servo has no find API yet, so Ferrum runs a small script in the page that
//! wraps matches in `<mark>` elements, scrolls to the current one and reports
//! "n of m" back. The bar itself is drawn natively in ui.rs.

use std::cell::RefCell;
use std::rc::Rc;

use servo::{JSValue, WebView};
use winit::window::Window;

#[derive(Default)]
pub struct FindState {
    pub open: bool,
    pub query: String,
    pub match_case: bool,
    /// (current, total) as reported by the page; current is 1-based, 0 = none.
    pub result: Rc<RefCell<(u32, u32)>>,
    pub focus: bool,
    last_searched: Option<(String, bool)>,
}

const SCRIPT: &str = r#"
(function (query, matchCase, step) {
  const W = window;
  const clear = () => {
    for (const m of W.__ferrumMarks || []) {
      const p = m.parentNode;
      if (!p) continue;
      p.replaceChild(document.createTextNode(m.textContent), m);
      p.normalize();
    }
    W.__ferrumMarks = [];
    W.__ferrumIndex = -1;
  };
  const show = () => {
    const marks = W.__ferrumMarks || [];
    marks.forEach((m, i) => {
      m.style.background = i === W.__ferrumIndex ? '#ff9632' : '#ffe14d';
    });
    const cur = marks[W.__ferrumIndex];
    if (cur && cur.scrollIntoView) cur.scrollIntoView({ block: 'center', inline: 'nearest' });
    return [marks.length ? W.__ferrumIndex + 1 : 0, marks.length];
  };
  if (step !== 0 && query === null) {
    const n = (W.__ferrumMarks || []).length;
    if (n) W.__ferrumIndex = (W.__ferrumIndex + step + n) % n;
    return show();
  }
  clear();
  if (!query) return [0, 0];
  const needle = matchCase ? query : query.toLowerCase();
  const skip = new Set(['SCRIPT', 'STYLE', 'NOSCRIPT', 'TEXTAREA', 'INPUT', 'SELECT', 'OPTION', 'TEMPLATE']);
  const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_TEXT, {
    acceptNode(n) {
      const p = n.parentElement;
      if (!p || skip.has(p.tagName)) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  const nodes = [];
  while (walker.nextNode()) nodes.push(walker.currentNode);
  const marks = [];
  for (let node of nodes) {
    let hay = matchCase ? node.data : node.data.toLowerCase();
    let at = hay.indexOf(needle);
    while (at !== -1 && marks.length < 5000) {
      const match = node.splitText(at);
      const rest = match.splitText(needle.length);
      const mark = document.createElement('mark');
      mark.setAttribute('data-ferrum-find', '');
      mark.style.color = '#000';
      mark.style.borderRadius = '2px';
      match.parentNode.replaceChild(mark, match);
      mark.appendChild(match);
      marks.push(mark);
      node = rest;
      hay = matchCase ? node.data : node.data.toLowerCase();
      at = hay.indexOf(needle);
    }
  }
  W.__ferrumMarks = marks;
  W.__ferrumIndex = marks.length ? 0 : -1;
  return show();
})
"#;

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

impl FindState {
    fn run(&self, webview: &WebView, window: &Rc<Window>, args: String) {
        let result = self.result.clone();
        let window = window.clone();
        webview.evaluate_javascript(format!("{SCRIPT}({args})"), move |value| {
            if let Ok(JSValue::Array(v)) = value {
                let num = |i: usize| match v.get(i) {
                    Some(JSValue::Number(n)) => *n as u32,
                    _ => 0,
                };
                *result.borrow_mut() = (num(0), num(1));
            }
            window.request_redraw();
        });
    }

    /// Search again if the query or options changed since the last search.
    pub fn search_if_changed(&mut self, webview: &WebView, window: &Rc<Window>) {
        let key = (self.query.clone(), self.match_case);
        if self.last_searched.as_ref() == Some(&key) {
            return;
        }
        self.last_searched = Some(key);
        self.run(webview, window, format!("{}, {}, 0", js_string(&self.query), self.match_case));
    }

    /// Move to the next (`1`) or previous (`-1`) match.
    pub fn step(&mut self, webview: &WebView, window: &Rc<Window>, delta: i32) {
        if self.last_searched.is_none() {
            self.search_if_changed(webview, window);
            return;
        }
        self.run(webview, window, format!("null, false, {delta}"));
    }

    /// Remove highlights from the page and hide the bar.
    pub fn close(&mut self, webview: Option<&WebView>, window: &Rc<Window>) {
        self.open = false;
        self.last_searched = None;
        *self.result.borrow_mut() = (0, 0);
        if let Some(webview) = webview {
            self.run(webview, window, "\"\", false, 0".into());
        }
    }

    /// The page changed (navigation or tab switch): highlights are gone or belong elsewhere.
    pub fn reset(&mut self) {
        self.last_searched = None;
        *self.result.borrow_mut() = (0, 0);
    }
}
