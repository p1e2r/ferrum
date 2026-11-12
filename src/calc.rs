// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! The address-bar calculator: type `12*(3+4)`, `2^10`, `sqrt(2)` or
//! `15% of 80` and the answer appears at the top of the suggestions.
//!
//! A small recursive-descent parser over `+ - * / ^ %`, brackets, `×` `÷`
//! `·`, implicit multiplication (`2pi`, `3(4+1)`), constants `pi` `π` `e`
//! and functions (`sqrt` `√` `cbrt` `abs` `sin` `cos` `tan` `asin` `acos`
//! `atan` `ln` `log` `log2` `exp` `round` `floor` `ceil`). Trigonometry is in
//! radians. Text is only treated as a sum when it has an operator or a
//! function, so plain numbers and words are still searches.

/// The answer, formatted for display, if `input` is a sum.
pub fn answer(input: &str) -> Option<String> {
    let s = input.trim().trim_end_matches('=').trim();
    if s.is_empty() || s.len() > 200 || !looks_like_sum(s) {
        return None;
    }
    let value = evaluate(s)?;
    value.is_finite().then(|| format_number(value))
}

/// Something worth calculating: digits plus an operator or a function.
fn looks_like_sum(s: &str) -> bool {
    let lower = s.to_lowercase();
    // Addresses and searches that happen to contain symbols.
    if lower.contains("://") || lower.starts_with("www.") || lower.contains(' ') && lower.split_whitespace().any(|w| w.len() > 5 && w.chars().all(char::is_alphabetic) && !FUNCTIONS.contains(&w) && w != "of") {
        return false;
    }
    let has_digit = s.chars().any(|c| c.is_ascii_digit()) || lower.contains("pi") || s.contains('π');
    // Implicit multiplication counts too: 2pi, 2π, 3(4+1).
    let implicit = ["pi", "π", "("].iter().any(|t| lower.match_indices(t).any(|(i, _)| lower[..i].ends_with(|c: char| c.is_ascii_digit())));
    let chars: Vec<char> = s.chars().collect();
    let times_x = (0..chars.len()).any(|i| chars[i] == 'x' && is_times_x(&chars, i));
    let has_op = s.chars().skip(1).any(|c| "+-*/^×÷·%".contains(c)) || s.contains('√') || implicit || times_x || FUNCTIONS.iter().any(|f| lower.contains(&format!("{f}(")));
    // A date like 2026-11-12 or a phone number isn't a sum.
    let date_like = s.chars().all(|c| c.is_ascii_digit() || c == '-') && s.matches('-').count() >= 2;
    has_digit && has_op && !date_like
}

const FUNCTIONS: &[&str] = &["sqrt", "cbrt", "abs", "sin", "cos", "tan", "asin", "acos", "atan", "ln", "log", "log2", "exp", "round", "floor", "ceil"];

pub fn evaluate(s: &str) -> Option<f64> {
    let tokens = tokenize(s)?;
    let mut p = Parser { tokens, pos: 0, last_was_percent: false };
    let v = p.expr()?;
    (p.pos == p.tokens.len()).then_some(v)
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Op(char),
    Open,
    Close,
    Name(String),
    Of,
}

fn tokenize(s: &str) -> Option<Vec<Tok>> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => i += 1,
            '0'..='9' | '.' | ',' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == ',' || chars[i] == '_') {
                    i += 1;
                }
                // Scientific notation: 1e3, 2.5E-4.
                if i + 1 < chars.len() && (chars[i] == 'e' || chars[i] == 'E') && (chars[i + 1].is_ascii_digit() || ((chars[i + 1] == '-' || chars[i + 1] == '+') && i + 2 < chars.len() && chars[i + 2].is_ascii_digit())) {
                    i += 2;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                // Commas group thousands (1,000,000).
                let text: String = chars[start..i].iter().filter(|c| **c != ',' && **c != '_').collect();
                out.push(Tok::Num(text.parse().ok()?));
            },
            '+' | '-' | '*' | '/' | '^' | '%' => {
                // ** is power too.
                if c == '*' && chars.get(i + 1) == Some(&'*') {
                    out.push(Tok::Op('^'));
                    i += 2;
                    continue;
                }
                out.push(Tok::Op(c));
                i += 1;
            },
            '×' | '·' | 'x' if c != 'x' || is_times_x(&chars, i) => {
                out.push(Tok::Op('*'));
                i += 1;
            },
            '÷' => {
                out.push(Tok::Op('/'));
                i += 1;
            },
            '(' | '[' => {
                out.push(Tok::Open);
                i += 1;
            },
            ')' | ']' => {
                out.push(Tok::Close);
                i += 1;
            },
            '√' => {
                out.push(Tok::Name("sqrt".into()));
                i += 1;
            },
            'π' => {
                out.push(Tok::Name("pi".into()));
                i += 1;
            },
            c if c.is_alphabetic() => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric()) {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect::<String>().to_lowercase();
                if word == "of" {
                    out.push(Tok::Of);
                } else {
                    out.push(Tok::Name(word));
                }
            },
            _ => return None,
        }
    }
    Some(out)
}

/// `x` between two numbers is "times" (3x4), not a name.
fn is_times_x(chars: &[char], i: usize) -> bool {
    let before = chars[..i].iter().rev().find(|c| **c != ' ');
    let after = chars[i + 1..].iter().find(|c| **c != ' ');
    matches!(before, Some(c) if c.is_ascii_digit() || *c == ')') && matches!(after, Some(c) if c.is_ascii_digit() || *c == '(')
}

struct Parser {
    tokens: Vec<Tok>,
    pos: usize,
    /// The last operand was a percentage (so `200 + 10%` adds 10% of 200).
    last_was_percent: bool,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    // expr := term (('+'|'-') term)*
    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        while let Some(Tok::Op(op @ ('+' | '-'))) = self.peek().cloned() {
            self.pos += 1;
            let rhs = self.term()?;
            // 200 + 10% means 200 + 10% of 200.
            let rhs = if self.last_was_percent { v * rhs } else { rhs };
            self.last_was_percent = false;
            v = if op == '+' { v + rhs } else { v - rhs };
        }
        Some(v)
    }

    // term := factor (('*'|'/'|implicit) factor)*
    fn term(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        loop {
            match self.peek().cloned() {
                Some(Tok::Op('*')) => {
                    self.pos += 1;
                    v *= self.unary()?;
                },
                Some(Tok::Op('/')) => {
                    self.pos += 1;
                    let d = self.unary()?;
                    if d == 0.0 {
                        return None;
                    }
                    v /= d;
                },
                Some(Tok::Of) => {
                    // 15% of 80
                    self.pos += 1;
                    v *= self.unary()?;
                    self.last_was_percent = false;
                },
                // Implicit multiplication: 2pi, 3(4+1), (1+2)(3+4).
                Some(Tok::Open) | Some(Tok::Name(_)) | Some(Tok::Num(_)) => {
                    v *= self.unary()?;
                },
                _ => break,
            }
        }
        Some(v)
    }

    // unary := ('-'|'+') unary | power
    fn unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.pos += 1;
                Some(-self.unary()?)
            },
            Some(Tok::Op('+')) => {
                self.pos += 1;
                self.unary()
            },
            _ => self.power(),
        }
    }

    // power := postfix ('^' unary)?   (right-associative)
    fn power(&mut self) -> Option<f64> {
        let base = self.postfix()?;
        if let Some(Tok::Op('^')) = self.peek() {
            self.pos += 1;
            let exp = self.unary()?;
            return Some(base.powf(exp));
        }
        Some(base)
    }

    // postfix := atom '%'?
    fn postfix(&mut self) -> Option<f64> {
        let v = self.atom()?;
        if let Some(Tok::Op('%')) = self.peek() {
            self.pos += 1;
            self.last_was_percent = true;
            return Some(v / 100.0);
        }
        self.last_was_percent = false;
        Some(v)
    }

    fn atom(&mut self) -> Option<f64> {
        match self.next()? {
            Tok::Num(n) => Some(n),
            Tok::Open => {
                let v = self.expr()?;
                match self.peek() {
                    Some(Tok::Close) => {
                        self.pos += 1;
                        Some(v)
                    },
                    // A missing closing bracket at the end is fine: (1+2
                    None => Some(v),
                    _ => None,
                }
            },
            Tok::Name(name) => match name.as_str() {
                "pi" => Some(std::f64::consts::PI),
                "e" => Some(std::f64::consts::E),
                "tau" => Some(std::f64::consts::TAU),
                f => {
                    let arg = self.power()?;
                    let v = match f {
                        "sqrt" => arg.sqrt(),
                        "cbrt" => arg.cbrt(),
                        "abs" => arg.abs(),
                        "sin" => arg.sin(),
                        "cos" => arg.cos(),
                        "tan" => arg.tan(),
                        "asin" => arg.asin(),
                        "acos" => arg.acos(),
                        "atan" => arg.atan(),
                        "ln" => arg.ln(),
                        "log" => arg.log10(),
                        "log2" => arg.log2(),
                        "exp" => arg.exp(),
                        "round" => arg.round(),
                        "floor" => arg.floor(),
                        "ceil" => arg.ceil(),
                        _ => return None,
                    };
                    Some(v)
                },
            },
            _ => None,
        }
    }
}

/// Up to 10 significant digits, thousands grouped, no trailing zeros.
pub fn format_number(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let abs = v.abs();
    if !(1e-6..1e15).contains(&abs) {
        let s = format!("{v:.6e}");
        // 1.500000e3 → 1.5e3
        if let Some((m, e)) = s.split_once('e') {
            let m = m.trim_end_matches('0').trim_end_matches('.');
            return format!("{m}e{e}");
        }
        return s;
    }
    let digits = (10 - abs.log10().floor() as i32 - 1).clamp(0, 10) as usize;
    let s = format!("{v:.digits$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    let (int, frac) = s.split_once('.').map(|(a, b)| (a.to_string(), Some(b.to_string()))).unwrap_or((s.clone(), None));
    let (sign, int) = int.strip_prefix('-').map(|i| ("-", i.to_string())).unwrap_or(("", int));
    let mut grouped = String::new();
    for (n, c) in int.chars().enumerate() {
        if n > 0 && (int.len() - n) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    match frac {
        Some(f) => format!("{sign}{grouped}.{f}"),
        None => format!("{sign}{grouped}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums() {
        assert_eq!(answer("1+1").as_deref(), Some("2"));
        assert_eq!(answer("12*(3+4)").as_deref(), Some("84"));
        assert_eq!(answer("2^10").as_deref(), Some("1,024"));
        assert_eq!(answer("2**3").as_deref(), Some("8"));
        assert_eq!(answer("10/4").as_deref(), Some("2.5"));
        assert_eq!(answer("-3 + 5").as_deref(), Some("2"));
        assert_eq!(answer("2^3^2").as_deref(), Some("512"));
        assert_eq!(answer("-2^2").as_deref(), Some("-4"));
        assert_eq!(answer("sqrt(16)").as_deref(), Some("4"));
        assert_eq!(answer("√2").as_deref(), Some("1.414213562"));
        assert_eq!(answer("2pi").as_deref(), Some("6.283185307"));
        assert_eq!(answer("3(4+1)").as_deref(), Some("15"));
        assert_eq!(answer("15% of 80").as_deref(), Some("12"));
        assert_eq!(answer("200 + 10%").as_deref(), Some("220"));
        assert_eq!(answer("50%").as_deref(), Some("0.5"));
        assert_eq!(answer("3 × 4 ÷ 2").as_deref(), Some("6"));
        assert_eq!(answer("3x4").as_deref(), Some("12"));
        assert_eq!(answer("1,000 * 3").as_deref(), Some("3,000"));
        assert_eq!(answer("1e3 + 1").as_deref(), Some("1,001"));
        assert_eq!(answer("0.1+0.2").as_deref(), Some("0.3"));
        assert_eq!(answer("1/3").as_deref(), Some("0.3333333333"));
        assert_eq!(answer("(1+2").as_deref(), Some("3"));
        assert_eq!(answer("5*5=").as_deref(), Some("25"));
        assert_eq!(answer("log(1000)").as_deref(), Some("3"));
        assert_eq!(answer("2^100").as_deref(), Some("1.267651e30"));
    }

    #[test]
    fn not_sums() {
        for s in ["42", "rust", "github.com", "https://a.com/x-y", "c++ tutorial", "2026-11-12", "1/0", "best laptops 2026", "covid-19 news", "x", "e", "pi", ""] {
            assert_eq!(answer(s), None, "{s}");
        }
    }
}
