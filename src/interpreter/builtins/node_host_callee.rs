//! V8's wording for a call or `new` on a value that is not callable, under
//! `--node` only (#1419): `o.f is not a function`, `x is not a constructor`.
//!
//! V8 reprints the callee from its AST (`CallPrinter`). Here the callee is read
//! back from the source text at the call position the stack hooks already
//! record, so the tree-walker and the bytecode VM share one path and the AST
//! and the bytecode stay as they are. Anything the scanner does not recognise
//! returns `None` and the caller keeps the engine's own wording.

use super::super::Interpreter;
use crate::interpreter::types::JsFunction;
use std::rc::Rc;

fn ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$' | b'#') || c >= 0x80
}

fn skip_ws_back(b: &[u8], mut i: usize) -> usize {
    while i > 0 && b[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    i
}

fn skip_ws_fwd(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

const KEYWORDS: [&str; 19] = [
    "if", "while", "for", "switch", "catch", "return", "typeof", "void", "delete", "in", "of",
    "with", "await", "yield", "new", "throw", "else", "do", "case",
];

/// V8 prints a `super` base as an unnamed intermediate value.
fn word_text(word: &str) -> &str {
    if word == "super" {
        "(intermediate value)"
    } else {
        word
    }
}

/// The text V8 prints for the key inside `[...]`: `.name` for a string
/// literal, `[1]` or `[i]` for a number or a plain name chain, `None` else.
fn render_index(inner: &str) -> Option<String> {
    let inner = inner.trim();
    let first = *inner.as_bytes().first()?;
    if matches!(first, b'\'' | b'"') {
        let body = inner
            .strip_prefix(first as char)?
            .strip_suffix(first as char)?;
        let plain = !body.contains(['\'', '"', '\\', '\n']);
        return plain.then(|| format!(".{body}"));
    }
    let chain = inner.bytes().all(|c| ident_byte(c) || c == b'.');
    (chain && !inner.starts_with('.') && !inner.ends_with('.')).then(|| format!("[{inner}]"))
}

/// The index of the quote that opens the string literal closed by the
/// quote at `close`.
fn string_start(b: &[u8], close: usize) -> Option<usize> {
    let quote = b[close];
    let mut at = close;
    while at > 0 {
        at -= 1;
        if b[at] == quote {
            let slashes = b[..at].iter().rev().take_while(|c| **c == b'\\').count();
            if slashes % 2 == 0 {
                return Some(at);
            }
        } else if b[at] == b'\n' && quote != b'`' {
            return None;
        }
    }
    None
}

/// The index of the opener that matches the closer at `close`, skipping
/// string literals.
fn opener_before(b: &[u8], close: usize, (open, shut): (u8, u8)) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = close + 1;
    while at > 0 {
        at -= 1;
        match b[at] {
            b'"' | b'\'' | b'`' => at = string_start(b, at).unwrap_or(at),
            c if c == shut => depth += 1,
            c if c == open => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// The index of the `(` that matches the `)` at `close`.
fn open_before(b: &[u8], close: usize) -> Option<usize> {
    opener_before(b, close, (b'(', b')'))
}

/// The index of the `)` that matches the `(` at `open`.
fn close_after(b: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = open;
    while at < b.len() {
        match b[at] {
            q @ (b'"' | b'\'') => {
                at += 1;
                while at < b.len() && b[at] != q {
                    at += if b[at] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// `s` split at the top-level (outside brackets and quotes) `sep`.
fn split_top<'a>(s: &'a str, sep: &str) -> Vec<&'a str> {
    let b = s.as_bytes();
    let (mut parts, mut depth, mut quote, mut from, mut at) = (Vec::new(), 0i32, 0u8, 0, 0);
    while at < b.len() {
        let c = b[at];
        if quote != 0 {
            quote = if c == quote { 0 } else { quote };
        } else if matches!(c, b'"' | b'\'' | b'`') {
            quote = c;
        } else if matches!(c, b'(' | b'[' | b'{') {
            depth += 1;
        } else if matches!(c, b')' | b']' | b'}') {
            depth -= 1;
        } else if depth == 0 && b[at..].starts_with(sep.as_bytes()) {
            parts.push(&s[from..at]);
            at += sep.len();
            from = at;
            continue;
        }
        at += 1;
    }
    parts.push(&s[from..]);
    parts
}

fn is_function_like(s: &str) -> bool {
    ["function", "async", "class", "await"].iter().any(|word| {
        s.strip_prefix(word)
            .is_some_and(|rest| !rest.starts_with(ident_byte_char))
    }) || split_top(s, "=>").len() > 1
}

fn ident_byte_char(c: char) -> bool {
    c.is_ascii() && ident_byte(c as u8)
}

/// V8's text for the expression `s`, read from its parse tree: the shapes the
/// scanner does not follow (operators, assignments, conditionals, array
/// literals).
fn ast_text(s: &str) -> Option<String> {
    let wrapped = format!("({s});");
    let mut parser = crate::parser::Parser::new(&wrapped).ok()?;
    let program = parser.parse_program().ok()?;
    match program.body.as_slice() {
        [crate::ast::Statement::Expression(expr)] => {
            Some(super::node_host_expr_text::expr_text(expr))
        }
        _ => None,
    }
}

/// The identifier that ends just before byte `end`.
fn word_before(src: &str, end: usize) -> &str {
    let b = src.as_bytes();
    let mut start = end;
    while start > 0 && ident_byte(b[start - 1]) {
        start -= 1;
    }
    &src[start..end]
}

/// The index of the `[` that matches the `]` at `close`.
fn bracket_before(b: &[u8], close: usize) -> Option<usize> {
    opener_before(b, close, (b'[', b']'))
}

/// V8's text for the expression `s`, if it is one of the shapes it prints.
fn render_expr(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if is_function_like(s) {
        return Some("(intermediate value)".into());
    }
    for op in ["||", "&&", "??"] {
        let parts = split_top(s, op);
        if parts.len() > 1 {
            let shown: Option<Vec<String>> = parts.iter().map(|p| render_expr(p)).collect();
            return Some(format!("({})", shown?.join(&format!(" {op} "))));
        }
    }
    for word in ["void", "typeof", "delete"] {
        if let Some(rest) = s.strip_prefix(word).filter(|r| r.starts_with(' ')) {
            return Some(format!("({word} {})", render_expr(rest)?));
        }
    }
    if let Some(body) = s.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        let props = split_top(body, ",")
            .into_iter()
            .filter(|p| !p.trim().is_empty())
            .count();
        return Some(format!("{{{}}}", "(intermediate value)".repeat(props)));
    }
    if s.starts_with(['"', '\'']) {
        let quote = s.chars().next()?;
        let body = s.strip_prefix(quote)?.strip_suffix(quote)?;
        let plain = !body.contains(['\'', '"', '\\', '\n']);
        return plain.then(|| format!("\"{body}\""));
    }
    if s.starts_with('(') && close_after(s.as_bytes(), 0)? == s.len() - 1 {
        return render_group(&s[1..s.len() - 1]);
    }
    match chain_before(s, s.len()) {
        Some((text, 0)) => Some(text),
        _ => ast_text(s),
    }
}

/// V8's text for the parenthesised expression `inner`: a comma sequence
/// prints as `(a , b)`.
fn render_group(inner: &str) -> Option<String> {
    let inner = inner.trim();
    let commas = split_top(inner, ",");
    if commas.len() > 1 && !is_function_like(inner) {
        let shown: Option<Vec<String>> = commas.iter().map(|p| render_expr(p)).collect();
        return Some(format!("({})", shown?.join(" , ")));
    }
    render_expr(inner)
}

/// The callee that ends just before byte `end`, read right to left, with the
/// byte where it starts.
fn chain_before(src: &str, end: usize) -> Option<(String, usize)> {
    let b = src.as_bytes();
    let mut parts: Vec<String> = Vec::new();
    let mut i = end;
    loop {
        i = skip_ws_back(b, i);
        let last = *b.get(i.checked_sub(1)?)?;
        let mut head = false;
        if ident_byte(last) {
            let mut s = i;
            while s > 0 && ident_byte(b[s - 1]) {
                s -= 1;
            }
            let word = &src[s..i];
            if KEYWORDS.contains(&word) {
                return None;
            }
            parts.push(word_text(word).into());
            i = s;
            head = true;
        } else if last == b']' {
            let open = bracket_before(b, i - 1)?;
            let before = skip_ws_back(b, open);
            let member = before > 0
                && (ident_byte(b[before - 1]) && !KEYWORDS.contains(&word_before(src, before))
                    || matches!(b[before - 1], b']' | b')' | b'"' | b'\'' | b'`'));
            if member {
                parts.push(render_index(&src[open + 1..i - 1])?);
            } else {
                parts.push(ast_text(&src[open..i])?);
                head = true;
            }
            i = open;
        } else if last == b'`' {
            let open = src[..i - 1].rfind('`')?;
            if src[open..i].contains("${") {
                return None;
            }
            parts.push("(...)".into());
            i = open;
        } else if matches!(last, b'"' | b'\'') {
            let open = src[..i - 1].rfind(last as char)?;
            let body = &src[open + 1..i - 1];
            if body.contains(['\\', '\n']) {
                return None;
            }
            parts.push(format!("\"{body}\""));
            i = open;
            head = true;
        } else if last == b')' {
            let open = open_before(b, i - 1)?;
            let before = skip_ws_back(b, open);
            let call = before > 0
                && (ident_byte(b[before - 1]) && !KEYWORDS.contains(&word_before(src, before))
                    || matches!(b[before - 1], b']' | b')'));
            if call {
                if src[open + 1..i - 1].contains(['\'', '"', '`']) {
                    return None;
                }
                parts.push("(...)".into());
            } else {
                parts.push(render_group(&src[open + 1..i - 1])?);
                head = true;
            }
            i = open;
        } else {
            return None;
        }
        if head {
            let k = skip_ws_back(b, i);
            if k > 0 && b[k - 1] == b'.' && !(k > 1 && b[k - 2] == b'.') {
                let optional = k > 1 && b[k - 2] == b'?';
                parts.push(if optional { "?." } else { "." }.into());
                i = if optional { k - 2 } else { k - 1 };
                continue;
            }
            break;
        }
    }
    parts.reverse();
    let k = skip_ws_back(b, i);
    if k >= 3 && &src[k - 3..k] == "new" && (k == 3 || !ident_byte(b[k - 4])) {
        let call = parts.iter().position(|part| part == "(...)")?;
        parts.splice(..=call, ["(intermediate value)".to_string()]);
        i = k - 3;
    }
    Some((parts.concat(), i))
}

/// The callee of the `new` whose keyword starts at byte `at`.
fn chain_after_new(src: &str, at: usize) -> Option<String> {
    let b = src.as_bytes();
    let mut i = skip_ws_fwd(b, at + 3);
    let mut out = String::new();
    let mut grouped = false;
    if b.get(i) == Some(&b'(') {
        let close = close_after(b, i)?;
        out = render_group(&src[i + 1..close])?;
        i = close + 1;
        grouped = true;
    }
    loop {
        if !grouped {
            let s = i;
            while i < b.len() && ident_byte(b[i]) {
                i += 1;
            }
            if s == i {
                return None;
            }
            out.push_str(word_text(&src[s..i]));
        }
        grouped = false;
        loop {
            let k = skip_ws_fwd(b, i);
            match b.get(k) {
                Some(b'.') if b.get(k + 1) != Some(&b'.') => {
                    out.push('.');
                    i = skip_ws_fwd(b, k + 1);
                    break;
                }
                Some(b'?') if b.get(k + 1) == Some(&b'.') => {
                    out.push_str("?.");
                    i = skip_ws_fwd(b, k + 2);
                    break;
                }
                Some(b'[') => {
                    let close = k + src[k..].find(']')?;
                    out.push_str(&render_index(&src[k + 1..close])?);
                    i = close + 1;
                }
                _ => return Some(out),
            }
        }
    }
}

/// The callee text of the call (or, with `construct`, the `new`) recorded at
/// `pos` (a source offset plus one) in `src`.
pub(super) fn callee_text(src: &str, pos: u32, construct: bool) -> Option<String> {
    let at = usize::try_from(pos).ok()?.checked_sub(1)?;
    let b = src.as_bytes();
    if construct {
        return (src.get(at..)?.starts_with("new") && !ident_byte(*b.get(at + 3)?))
            .then(|| chain_after_new(src, at))
            .flatten();
    }
    let end = match *b.get(at)? {
        b'(' => at,
        b'`' => at,
        c if ident_byte(c) => {
            let mut e = at;
            while e < b.len() && ident_byte(b[e]) {
                e += 1;
            }
            let k = skip_ws_fwd(b, e);
            if !(b.get(k) == Some(&b'(') || src[k..].starts_with("?.(")) {
                return None;
            }
            e
        }
        _ => return None,
    };
    chain_before(src, end).map(|(text, _)| text)
}

impl Interpreter {
    /// A member access that is about to throw on a null or undefined base:
    /// the frame's position becomes the access's, as V8 reports it.
    pub(crate) fn note_member_pos(&mut self, site: crate::ast::PropSiteId) {
        if site.1 != 0 {
            self.node_host.call_pos = site.1;
        }
    }

    /// A tagged template about to call its tag: the frame's position becomes
    /// the template's (see `Parser::template_id`).
    pub(crate) fn note_template_pos(&mut self, id: u64) {
        if let Ok(pos) = u32::try_from(id >> 32)
            && pos != 0
        {
            self.node_host.call_pos = pos;
        }
    }

    /// The source text of the frame making the call being dispatched.
    fn node_caller_source(&self) -> Option<Rc<str>> {
        let func = self.call_stack_frames.last()?.func_obj_id;
        let user = self
            .get_object(func)
            .and_then(|obj| match obj.borrow().callable.as_deref() {
                Some(JsFunction::User {
                    source_text: Some(t),
                    ..
                }) => Some(t.origin().0.clone()),
                _ => None,
            });
        user.or_else(|| self.node_host.stack.script_frame_source(func))
    }

    /// V8's text for the callee of the call (or `new`) being dispatched,
    /// under `--node` only.
    pub(crate) fn node_callee_text(&self, construct: bool) -> Option<String> {
        if self.top_native().is_some() {
            return None;
        }
        self.node_callee_text_any(construct)
    }

    /// The same when a builtin is running: the call is still the one the
    /// script made at the recorded position (`Reflect.apply(p, ...)` reads as
    /// `Reflect.apply`).
    pub(crate) fn node_callee_text_any(&self, construct: bool) -> Option<String> {
        if !self.node_host.stack.installed {
            return None;
        }
        let src = self.node_caller_source()?;
        callee_text(&src, self.node_host.call_pos, construct)
    }
}
