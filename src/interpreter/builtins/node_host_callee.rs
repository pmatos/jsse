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
            parts.push(if word == "super" {
                "(intermediate value)".into()
            } else {
                word.into()
            });
            i = s;
            head = true;
        } else if last == b']' {
            let open = src[..i - 1].rfind('[')?;
            parts.push(render_index(&src[open + 1..i - 1])?);
            i = open;
        } else if last == b')' {
            let open = src[..i - 1].rfind('(')?;
            if src[open + 1..i - 1].contains(['(', ')', '\'', '"', '`']) {
                return None;
            }
            let before = skip_ws_back(b, open);
            let call =
                before > 0 && (ident_byte(b[before - 1]) || matches!(b[before - 1], b']' | b')'));
            if call {
                parts.push("(...)".into());
            } else {
                let (inner, start) = chain_before(src, i - 1)?;
                if skip_ws_fwd(b, open + 1) != start {
                    return None;
                }
                parts.push(inner);
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
    Some((parts.concat(), i))
}

/// The callee of the `new` whose keyword starts at byte `at`.
fn chain_after_new(src: &str, at: usize) -> Option<String> {
    let b = src.as_bytes();
    let mut i = skip_ws_fwd(b, at + 3);
    let mut out = String::new();
    loop {
        let s = i;
        while i < b.len() && ident_byte(b[i]) {
            i += 1;
        }
        if s == i {
            return None;
        }
        out.push_str(if &src[s..i] == "super" {
            "(intermediate value)"
        } else {
            &src[s..i]
        });
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
        if !self.node_host.stack.installed {
            return None;
        }
        let src = self.node_caller_source()?;
        callee_text(&src, self.node_host.call_pos, construct)
    }

    /// The TypeError message for calling a value that is not callable.
    pub(crate) fn not_callable_message(&self, value: &crate::types::JsValue) -> String {
        let shown = self
            .node_callee_text(false)
            .unwrap_or_else(|| self.describe_non_callable(value));
        format!("{shown} is not a function")
    }

    /// The TypeError for calling a class constructor without `new`.
    pub(crate) fn class_call_error(&mut self, class: u64) -> crate::types::JsValue {
        let name = self
            .node_host
            .stack
            .installed
            .then(|| self.own_data_string(class, "name"))
            .flatten();
        let text = match name {
            Some(name) => format!("Class constructor {name} cannot be invoked without 'new'"),
            None => "Class constructor cannot be invoked without 'new'".to_string(),
        };
        self.create_type_error(&text)
    }
}
