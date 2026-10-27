//! The export-name detection for CommonJS modules imported from ES modules
//! (issue #1241): what Node does with cjs-module-lexer. It finds, without
//! running the source, the names assigned to `exports` and the specifiers
//! whose exports are re-exported (`module.exports = require(..)`, spreads,
//! `__exportStar`, Babel's `Object.keys(..).forEach`). The caller follows the
//! re-exports. Forms outside the documented grammar are not detected, as in
//! Node. See `docs/adr/2026-10-26-2300-node-esm-interop.md`.

use super::super::*;
use std::collections::HashMap;

pub(crate) struct CjsNames {
    pub exports: Vec<String>,
    pub reexports: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Prev {
    None,
    Word(usize, usize),
    Punct(u8),
    Value,
    CloseParen(bool),
    CloseBrace(bool),
}

#[derive(Clone, Copy, PartialEq)]
enum Brace {
    Block,
    Object,
    Template,
}

pub(super) struct Lexer<'a> {
    pub(super) s: &'a [u8],
    pub(super) pos: usize,
    pub(super) exports: Vec<String>,
    pub(super) reexports: Vec<String>,
    pub(super) star_map: HashMap<String, String>,
}

pub(super) fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c >= 0x80
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

const EXPRESSION_WORDS: &[&str] = &[
    "return",
    "typeof",
    "instanceof",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "case",
    "do",
    "else",
    "yield",
    "await",
];

impl<'a> Lexer<'a> {
    pub(super) fn at(&self, i: usize) -> u8 {
        self.s.get(i).copied().unwrap_or(0)
    }

    pub(super) fn peek(&self) -> u8 {
        self.at(self.pos)
    }

    pub(super) fn starts(&self, text: &str, at: usize) -> bool {
        self.s
            .get(at..)
            .is_some_and(|r| r.starts_with(text.as_bytes()))
    }

    fn skip_line(&mut self) {
        while self.pos < self.s.len() && self.peek() != b'\n' && self.peek() != b'\r' {
            self.pos += 1;
        }
    }

    fn skip_block(&mut self) {
        self.pos += 2;
        while self.pos < self.s.len() && !self.starts("*/", self.pos) {
            self.pos += 1;
        }
        self.pos = (self.pos + 2).min(self.s.len());
    }

    /// Skip whitespace and comments; the next byte (0 at the end).
    pub(super) fn ws(&mut self) -> u8 {
        loop {
            let c = self.peek();
            if is_ws(c) {
                self.pos += 1;
            } else if c == b'/' && self.at(self.pos + 1) == b'/' {
                self.skip_line();
            } else if c == b'/' && self.at(self.pos + 1) == b'*' {
                self.skip_block();
            } else {
                return c;
            }
        }
    }

    pub(super) fn ident(&mut self) -> Option<(usize, usize)> {
        let start = self.pos;
        let c = self.peek();
        if !(c.is_ascii_alphabetic() || c == b'_' || c == b'$' || c >= 0x80) {
            return None;
        }
        while is_ident(self.peek()) {
            self.pos += 1;
        }
        Some((start, self.pos))
    }

    pub(super) fn text(&self, a: usize, b: usize) -> String {
        String::from_utf8_lossy(&self.s[a..b]).into_owned()
    }

    /// A string literal starting at the quote at `pos`; leaves `pos` after it.
    pub(super) fn string(&mut self) -> Option<String> {
        let quote = self.peek();
        self.pos += 1;
        let mut out = Vec::new();
        while self.pos < self.s.len() {
            let c = self.peek();
            self.pos += 1;
            match c {
                _ if c == quote => return Some(String::from_utf8_lossy(&out).into_owned()),
                b'\n' | b'\r' => return None,
                b'\\' => {
                    let e = self.peek();
                    self.pos += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'v' => out.push(11),
                        b'0' => out.push(0),
                        b'x' | b'u' => {
                            let braced = e == b'u' && self.peek() == b'{';
                            let (digits, from) = match (e, braced) {
                                (b'x', _) => (2, self.pos),
                                (_, true) => (usize::MAX, self.pos + 1),
                                _ => (4, self.pos),
                            };
                            let mut end = from;
                            while end - from < digits && self.at(end).is_ascii_hexdigit() {
                                end += 1;
                            }
                            let hex = String::from_utf8_lossy(&self.s[from..end]).into_owned();
                            let ch = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32);
                            self.pos = if braced { end + 1 } else { end };
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(
                                ch.unwrap_or('\u{fffd}').encode_utf8(&mut buf).as_bytes(),
                            );
                        }
                        b'\n' => {}
                        b'\r' => {
                            if self.peek() == b'\n' {
                                self.pos += 1;
                            }
                        }
                        _ => out.push(e),
                    }
                }
                _ => out.push(c),
            }
        }
        None
    }

    fn regex_allowed(&self, prev: Prev) -> bool {
        match prev {
            Prev::None => true,
            Prev::Punct(c) => !matches!(c, b']'),
            Prev::Value => false,
            Prev::CloseParen(keyword) => keyword,
            Prev::CloseBrace(block) => block,
            Prev::Word(a, b) => {
                let w = &self.s[a..b];
                EXPRESSION_WORDS.iter().any(|k| k.as_bytes() == w)
            }
        }
    }

    fn skip_regex(&mut self) {
        self.pos += 1;
        let mut class = false;
        while self.pos < self.s.len() {
            let c = self.peek();
            self.pos += 1;
            match c {
                b'\\' => self.pos += 1,
                b'[' => class = true,
                b']' => class = false,
                b'/' if !class => return,
                b'\n' | b'\r' => return,
                _ => {}
            }
        }
    }

    /// From after a backtick or a `}` closing a `${`: to the next `${` (true)
    /// or the closing backtick (false).
    fn skip_template(&mut self) -> bool {
        while self.pos < self.s.len() {
            let c = self.peek();
            self.pos += 1;
            match c {
                b'\\' => self.pos += 1,
                b'`' => return false,
                b'$' if self.peek() == b'{' => {
                    self.pos += 1;
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    fn run(&mut self) {
        let mut prev = Prev::None;
        let mut parens: Vec<bool> = Vec::new();
        let mut braces: Vec<(Brace, bool)> = Vec::new();
        while self.pos < self.s.len() {
            let c = self.peek();
            if is_ws(c) {
                self.pos += 1;
                continue;
            }
            match c {
                b'\'' | b'"' => {
                    if self.string().is_none() {
                        return;
                    }
                    prev = Prev::Value;
                }
                b'`' => {
                    self.pos += 1;
                    if self.skip_template() {
                        braces.push((Brace::Template, false));
                        prev = Prev::Punct(b'{');
                    } else {
                        prev = Prev::Value;
                    }
                }
                b'/' => {
                    let n = self.at(self.pos + 1);
                    if n == b'/' {
                        self.skip_line();
                    } else if n == b'*' {
                        self.skip_block();
                    } else if self.regex_allowed(prev) {
                        self.skip_regex();
                        prev = Prev::Value;
                    } else {
                        self.pos += 1;
                        prev = Prev::Punct(b'/');
                    }
                }
                b'(' => {
                    let kw = matches!(prev, Prev::Word(a, b)
                        if matches!(&self.s[a..b], b"if" | b"while" | b"for" | b"with"));
                    parens.push(kw);
                    self.pos += 1;
                    prev = Prev::Punct(b'(');
                }
                b')' => {
                    let kw = parens.pop().unwrap_or(false);
                    self.pos += 1;
                    prev = Prev::CloseParen(kw);
                }
                b'{' => {
                    let block = match prev {
                        Prev::None | Prev::CloseParen(_) | Prev::CloseBrace(_) => true,
                        Prev::Punct(p) => matches!(p, b';' | b'{' | b'}' | b'>'),
                        Prev::Word(a, b) => {
                            let w = &self.s[a..b];
                            matches!(w, b"else" | b"do" | b"try" | b"finally")
                                || !EXPRESSION_WORDS.iter().any(|k| k.as_bytes() == w)
                        }
                        Prev::Value => false,
                    };
                    braces.push((if block { Brace::Block } else { Brace::Object }, block));
                    self.pos += 1;
                    prev = Prev::Punct(b'{');
                }
                b'}' => {
                    self.pos += 1;
                    match braces.pop() {
                        Some((Brace::Template, _)) => {
                            if self.skip_template() {
                                braces.push((Brace::Template, false));
                            } else {
                                prev = Prev::Value;
                                continue;
                            }
                        }
                        Some((_, block)) => prev = Prev::CloseBrace(block),
                        None => prev = Prev::CloseBrace(true),
                    }
                    if !matches!(prev, Prev::CloseBrace(_)) {
                        prev = Prev::Punct(b'{');
                    }
                }
                _ if c.is_ascii_digit() => {
                    while is_ident(self.peek()) || self.peek() == b'.' {
                        self.pos += 1;
                    }
                    prev = Prev::Value;
                }
                _ if is_ident(c) => {
                    let start = self.pos;
                    let dotted = start > 0 && self.s[start - 1] == b'.';
                    let word = self.ident().unwrap_or((start, start + 1));
                    let text = &self.s[word.0..word.1];
                    let top = parens.is_empty() && braces.is_empty();
                    let resume = self.pos;
                    match text {
                        b"exports" if !dotted => {
                            self.pos = start;
                            self.exports_assign(false);
                        }
                        b"module" if !dotted => {
                            let revert = self.pos;
                            if self.ws() == b'.' {
                                self.pos += 1;
                                self.ws();
                                if self.starts("exports", self.pos) {
                                    self.exports_assign(true);
                                }
                            }
                            self.pos = revert;
                        }
                        b"Object" if !dotted => {
                            self.pos = start;
                            self.object_define_or_keys(top);
                        }
                        b"require" if !dotted => {
                            self.pos = start;
                            if let Some(spec) = self.require_call() {
                                self.bind_star_export(start.saturating_sub(1), &spec);
                            }
                        }
                        b"_interopRequireWildcard" => {
                            self.pos = resume;
                            if self.peek() == b'(' && self.at(self.pos + 1) == b'r' {
                                self.pos += 1;
                                if let Some(spec) = self.require_call() {
                                    self.bind_star_export(start.saturating_sub(1), &spec);
                                }
                            }
                        }
                        b"__export" | b"__exportStar" => {
                            self.pos = resume;
                            if self.peek() == b'(' && self.at(self.pos + 1) == b'r' {
                                self.pos += 1;
                                if let Some(spec) = self.require_call() {
                                    self.add_reexport(spec);
                                }
                            }
                        }
                        _ => {}
                    }
                    self.pos = resume;
                    prev = Prev::Word(word.0, word.1);
                }
                _ => {
                    self.pos += 1;
                    prev = Prev::Punct(c);
                }
            }
        }
    }
}

pub(crate) fn lex_cjs_names(source: &str) -> CjsNames {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut lexer = Lexer {
        s: source.as_bytes(),
        pos: 0,
        exports: Vec::new(),
        reexports: Vec::new(),
        star_map: HashMap::new(),
    };
    lexer.run();
    CjsNames {
        exports: lexer.exports,
        reexports: lexer.reexports,
    }
}

impl Interpreter {
    pub(super) fn setup_node_host_cjs_lexer(&mut self) {
        let lex = self.create_function(JsFunction::native(
            "__host_cjs_lex".to_string(),
            1,
            |interp, _this, args| {
                let source = match interp
                    .to_string_lossy(args.first().unwrap_or(JsValue::undefined_ref()))
                {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let names = lex_cjs_names(&source);
                let to_array = |interp: &mut Interpreter, items: Vec<String>| {
                    let values = items
                        .iter()
                        .map(|s| JsValue::string(JsString::from_str(s)))
                        .collect();
                    interp.create_array(values)
                };
                let exports = to_array(interp, names.exports);
                let frame = interp.gc_root_frame();
                interp.gc_root_value(&exports);
                let reexports = to_array(interp, names.reexports);
                interp.gc_unroot_frame(frame);
                let frame = interp.gc_root_frame();
                interp.gc_root_value(&reexports);
                let pair = interp.create_array(vec![exports, reexports]);
                interp.gc_unroot_frame(frame);
                Completion::Normal(pair)
            },
        ));
        self.install_host_global("__host_cjs_lex", lex);
    }
}
