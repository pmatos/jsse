//! The grammar of the CommonJS export detection (see
//! `node_host_cjs_lexer.rs`): the forms of `exports` assignment that Node's
//! cjs-module-lexer recognises.

use super::node_host_cjs_lexer::{Lexer, is_ident};

impl Lexer<'_> {
    pub(super) fn add_export(&mut self, name: String) {
        if !self.exports.contains(&name) {
            self.exports.push(name);
        }
    }

    pub(super) fn add_reexport(&mut self, name: String) {
        if !self.reexports.contains(&name) {
            self.reexports.push(name);
        }
    }

    /// `require ( 'x' )` at `pos` (the `r`); leaves `pos` on the closing paren.
    pub(super) fn require_call(&mut self) -> Option<String> {
        if !self.starts("require", self.pos) {
            return None;
        }
        self.pos += 7;
        if self.ws() != b'(' {
            return None;
        }
        self.pos += 1;
        let q = self.ws();
        if q != b'\'' && q != b'"' {
            return None;
        }
        let spec = self.string()?;
        (self.ws() == b')').then_some(spec)
    }

    /// `exports` or `module.exports` at `pos`.
    pub(super) fn exports_ref(&mut self) -> bool {
        let revert = self.pos;
        if self.starts("module", self.pos) {
            self.pos += 6;
            if self.ws() != b'.' {
                self.pos = revert;
                return false;
            }
            self.pos += 1;
            self.ws();
        }
        if self.starts("exports", self.pos) {
            self.pos += 7;
            return true;
        }
        self.pos = revert;
        false
    }

    /// After the word `exports`: `.name =`, `['name'] =`, and with `assign`
    /// (reached through `module.`) `= { .. }` and `= require(..)`.
    pub(super) fn exports_assign(&mut self, assign: bool) {
        self.pos += 7;
        let revert = self.pos - 1;
        match self.ws() {
            b'.' => {
                self.pos += 1;
                self.ws();
                if let Some((a, b)) = self.ident()
                    && self.ws() == b'='
                {
                    let name = self.text(a, b);
                    self.add_export(name);
                    return;
                }
            }
            b'[' => {
                self.pos += 1;
                let q = self.ws();
                if (q == b'\'' || q == b'"')
                    && let Some(name) = self.string()
                    && self.ws() == b']'
                {
                    self.pos += 1;
                    if self.ws() == b'=' {
                        self.add_export(name);
                    }
                }
            }
            b'=' if assign => {
                self.reexports.clear();
                self.pos += 1;
                match self.ws() {
                    b'{' => return self.literal_exports(),
                    b'r' => {
                        if let Some(spec) = self.require_call() {
                            self.add_reexport(spec);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        self.pos = revert;
    }

    pub(super) fn literal_exports(&mut self) {
        let revert = self.pos;
        loop {
            self.pos += 1;
            let c = self.ws();
            let mut next: u8;
            if let Some((a, b)) = self.ident() {
                next = self.ws();
                if next == b':' {
                    self.pos += 1;
                    self.ws();
                    if self.ident().is_none() {
                        self.pos = revert;
                        return;
                    }
                    next = self.peek();
                }
                let name = self.text(a, b);
                if name == "get"
                    && (next.is_ascii_alphabetic() || matches!(next, b'_' | b'$') || next >= 0x80)
                {
                    self.pos -= 1;
                    return;
                }
                self.add_export(name);
            } else if c == b'.' && self.starts("..", self.pos + 1) {
                self.pos += 3;
                if self.peek() == b'r'
                    && let Some(spec) = self.require_call()
                {
                    self.add_reexport(spec);
                    self.pos += 1;
                } else if self.ident().is_none() {
                    self.pos -= 1;
                    return;
                }
                next = self.ws();
            } else if c == b'\'' || c == b'"' {
                let Some(name) = self.string() else { return };
                next = self.ws();
                if next == b':' {
                    self.pos += 1;
                    self.ws();
                    if self.ident().is_none() {
                        self.pos = revert;
                        return;
                    }
                    next = self.peek();
                    self.add_export(name);
                }
            } else {
                self.pos -= 1;
                return;
            }
            if next == b'}' {
                return;
            }
            if next != b',' {
                self.pos -= 1;
                return;
            }
        }
    }

    /// After `Object`: `.defineProperty(exports, 'x', { .. })` and, at the top
    /// level, Babel's `Object.keys(_x).forEach(function (key) { .. })`.
    pub(super) fn object_define_or_keys(&mut self, top_level: bool) {
        self.pos += 6;
        let revert = self.pos - 1;
        if self.ws() == b'.' {
            self.pos += 1;
            self.ws();
            if self.starts("defineProperty", self.pos) {
                self.pos += 14;
                self.define_property();
            } else if top_level && self.starts("keys", self.pos) {
                self.pos += 4;
                self.keys_for_each();
            }
        }
        self.pos = revert;
    }

    pub(super) fn define_property(&mut self) {
        let mut name: Option<String> = None;
        let ok = (|| -> Option<()> {
            if self.ws() != b'(' {
                return None;
            }
            self.pos += 1;
            self.ws();
            if !self.exports_ref() || self.ws() != b',' {
                return None;
            }
            self.pos += 1;
            let q = self.ws();
            if q != b'\'' && q != b'"' {
                return None;
            }
            name = Some(self.string()?);
            if self.ws() != b',' {
                return None;
            }
            self.pos += 1;
            if self.ws() != b'{' {
                return None;
            }
            self.pos += 1;
            let mut c = self.ws();
            if c == b'e' {
                if !self.starts("numerable", self.pos + 1) {
                    return None;
                }
                self.pos += 10;
                if self.ws() != b':' {
                    return None;
                }
                self.pos += 1;
                self.ws();
                if !self.starts("true", self.pos) {
                    return None;
                }
                self.pos += 4;
                if self.ws() != b',' {
                    return None;
                }
                self.pos += 1;
                c = self.ws();
            }
            if c == b'v' {
                if !self.starts("value", self.pos) {
                    return None;
                }
                self.pos += 5;
                return (self.ws() == b':').then_some(());
            }
            if c == b'g' && self.starts("et", self.pos + 1) {
                self.pos += 3;
                return self.getter_body();
            }
            None
        })();
        if let (Some(name), Some(())) = (name, ok) {
            self.add_export(name);
        }
    }

    /// `[: function [name]] () { return a[.b | ['b']] [;] } [,] })`.
    pub(super) fn getter_body(&mut self) -> Option<()> {
        let mut c = self.ws();
        if c == b':' {
            self.pos += 1;
            self.ws();
            if !self.starts("function", self.pos) {
                return None;
            }
            self.pos += 8;
            let last = self.pos;
            c = self.ws();
            if c != b'(' && (last == self.pos || self.ident().is_none()) {
                return None;
            }
            c = self.ws();
        }
        for want in *b"(){" {
            if c != want {
                return None;
            }
            self.pos += 1;
            c = self.ws();
        }
        if !self.starts("return", self.pos) {
            return None;
        }
        self.pos += 6;
        self.ws();
        self.ident()?;
        c = self.ws();
        if c == b'.' {
            self.pos += 1;
            self.ws();
            self.ident()?;
            c = self.ws();
        } else if c == b'[' {
            self.pos += 1;
            let q = self.ws();
            if q != b'\'' && q != b'"' {
                return None;
            }
            self.string()?;
            if self.ws() != b']' {
                return None;
            }
            self.pos += 1;
            c = self.ws();
        }
        if c == b';' {
            self.pos += 1;
            c = self.ws();
        }
        for (i, want) in b"}}".iter().copied().enumerate() {
            if c != want {
                return None;
            }
            self.pos += 1;
            c = self.ws();
            if i == 0 && c == b',' {
                self.pos += 1;
                c = self.ws();
            }
        }
        (c == b')').then_some(())
    }

    /// `Object.keys(ID).forEach(function (K) { .. })` where `ID` was bound to
    /// `require('x')` and the body skips `default` and `__esModule` and copies
    /// the key onto `exports`: a re-export of `x`.
    pub(super) fn keys_for_each(&mut self) {
        let resume = self.pos;
        let found = (|| -> Option<String> {
            if self.ws() != b'(' {
                return None;
            }
            self.pos += 1;
            self.ws();
            let (a, b) = self.ident()?;
            let id = self.text(a, b);
            if self.ws() != b')' {
                return None;
            }
            self.pos += 1;
            if self.ws() != b'.' {
                return None;
            }
            self.pos += 1;
            self.ws();
            if !self.starts("forEach", self.pos) {
                return None;
            }
            self.pos += 7;
            if self.ws() != b'(' {
                return None;
            }
            self.pos += 1;
            self.ws();
            if !self.starts("function", self.pos) {
                return None;
            }
            self.pos += 8;
            if self.ws() != b'(' {
                return None;
            }
            self.pos += 1;
            self.ws();
            self.ident()?;
            if self.ws() != b')' {
                return None;
            }
            self.pos += 1;
            if self.ws() != b'{' {
                return None;
            }
            self.pos += 1;
            let start = self.pos;
            let mut depth = 1usize;
            while self.pos < self.s.len() && depth > 0 {
                match self.peek() {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    b'\'' | b'"' => {
                        self.string()?;
                        continue;
                    }
                    _ => {}
                }
                self.pos += 1;
            }
            let body = &self.s[start..self.pos.min(self.s.len())];
            let has = |needle: &str| body.windows(needle.len()).any(|w| w == needle.as_bytes());
            let skips = (has("'default'") || has("\"default\"")) && has("__esModule");
            (skips && has("exports")).then(|| self.star_map.get(&id).cloned())?
        })();
        if let Some(spec) = found {
            self.add_reexport(spec);
        }
        self.pos = resume;
    }

    /// After `var|let|const ID = [_interopRequireWildcard(]require('x')`:
    /// remember that `ID` stands for `x`.
    pub(super) fn bind_star_export(&mut self, mut b: usize, spec: &str) {
        let at = |s: &Self, i: usize| if i < s.s.len() { s.s[i] } else { 0 };
        let skip_spaces = |s: &Self, b: &mut usize| {
            while *b > 0 && at(s, *b) == b' ' {
                *b -= 1;
            }
        };
        skip_spaces(self, &mut b);
        if at(self, b) != b'=' || b == 0 {
            return;
        }
        b -= 1;
        skip_spaces(self, &mut b);
        let end = b;
        while b > 0 && is_ident(at(self, b)) {
            b -= 1;
        }
        if b == end || at(self, b) != b' ' {
            return;
        }
        let id = self.text(b + 1, end + 1);
        skip_spaces(self, &mut b);
        let declared = match at(self, b) {
            b'r' => b >= 2 && self.starts("va", b - 2),
            b't' => (b >= 2 && self.starts("le", b - 2)) || (b >= 4 && self.starts("cons", b - 4)),
            _ => false,
        };
        if declared {
            self.star_map.insert(id, spec.to_string());
        }
    }
}
