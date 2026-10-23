//! Class and destructuring-pattern half of the printer (see `js_print`).

use crate::js_ast::*;
use crate::js_pools::*;
use crate::js_print::{FnKind, Printer, pick_str};

impl Printer {
    pub(crate) fn key(&mut self, k: &Key, private: &str) {
        match k {
            Key::Name(i) => self.quoted(pick_str(KEYS, *i)),
            Key::Str(i) => {
                let s = pick_str(STRS, *i);
                self.w(if s == r#""constructor""# || s == r#""prototype""# {
                    "\"x\""
                } else {
                    s
                });
            }
            Key::Computed(e) => {
                self.w("[");
                self.paren(e);
                self.w("]");
            }
            Key::Private => self.w(private),
        }
    }

    pub fn class(&mut self, c: &ClassDef, name: &str) {
        let strict = std::mem::replace(&mut self.strict, true);
        self.w(&format!("class {name}"));
        if let Some(e) = &c.extends {
            self.w(" extends (");
            self.expr(e);
            self.w(")");
        }
        self.w(" {\n");
        let privates: Vec<String> = c
            .members
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                matches!(
                    &m.kind,
                    MemberKind::Field(Key::Private, _)
                        | MemberKind::Method(Key::Private, _)
                        | MemberKind::Get(Key::Private, _)
                        | MemberKind::Set(Key::Private, _)
                )
            })
            .map(|(i, _)| format!("#q{i}"))
            .collect();
        self.privs.push(privates);
        if let Some(ctor) = &c.ctor {
            let derived = c.extends.is_some();
            let saved = self.enter(FnKind::Ctor { derived }, false, false);
            self.w("constructor(");
            self.params(&ctor.params);
            self.w(") {\n");
            if derived && ctor.super_first {
                self.w("super();\n");
            }
            self.stmts(&ctor.body);
            self.w("}\n");
            self.leave(saved);
        }
        for (i, m) in c.members.iter().enumerate() {
            let private = format!("#q{i}");
            let prefix = if m.is_static { "static " } else { "" };
            match &m.kind {
                MemberKind::Field(k, init) => {
                    self.w(prefix);
                    self.key(k, &private);
                    if let Some(e) = init {
                        self.w(" = (");
                        let saved = self.enter(FnKind::Init, false, false);
                        self.expr(e);
                        self.leave(saved);
                        self.w(")");
                    }
                    self.w(";\n");
                }
                MemberKind::Method(k, f) => {
                    self.w(prefix);
                    self.w(if f.is_async { "async " } else { "" });
                    self.w(if f.is_gen { "*" } else { "" });
                    self.key(k, &private);
                    self.func_tail(FnKind::Method, f);
                    self.w("\n");
                }
                MemberKind::Get(k, b) | MemberKind::Set(k, b) => {
                    let setter = matches!(m.kind, MemberKind::Set(..));
                    self.w(prefix);
                    self.w(if setter { "set " } else { "get " });
                    self.key(k, &private);
                    self.accessor_tail(setter, b);
                    self.w("\n");
                }
                MemberKind::StaticBlock(b) => {
                    let saved = self.enter(FnKind::Init, false, false);
                    self.w("static {\n");
                    self.stmts(b);
                    self.w("}\n");
                    self.leave(saved);
                }
            }
        }
        self.privs.pop();
        self.w("}");
        self.strict = strict;
    }

    pub fn private_names(&self) -> Vec<String> {
        self.privs.iter().flatten().cloned().collect()
    }

    pub(crate) fn bind_name(t: &Target) -> &'static str {
        match t {
            Target::Id(i) => pick_str(&VARS, *i),
            Target::Member(_, p) => pick_str(&VARS, *p),
            Target::Index(..) => VARS[0],
        }
    }

    fn pat_elem(&mut self, e: &PatElem, binding: bool) {
        self.pat(&e.pat, binding);
        if let Some(d) = &e.default {
            self.w(" = (");
            self.expr(d);
            self.w(")");
        }
    }

    fn rest(&mut self, t: &Target, binding: bool) {
        self.w("...");
        if binding {
            self.w(Self::bind_name(t));
        } else {
            self.target(t);
        }
    }

    pub fn pat(&mut self, p: &Pat, binding: bool) {
        self.depth += 1;
        match p {
            Pat::Leaf(t) if binding => self.w(Self::bind_name(t)),
            Pat::Leaf(t) => self.target(t),
            _ if self.over() => self.w(if binding { "a" } else { "[]" }),
            Pat::Obj(props, rest) => {
                self.w("{");
                for (i, (k, e)) in props.iter().enumerate() {
                    self.w(if i > 0 { ", " } else { "" });
                    self.quoted(pick_str(PROPS, *k));
                    self.w(": ");
                    self.pat_elem(e, binding);
                }
                if let Some(t) = rest {
                    self.w(if props.is_empty() { "" } else { ", " });
                    self.rest(t, binding);
                }
                self.w("}");
            }
            Pat::Arr(elems, rest) => {
                self.w("[");
                for (i, e) in elems.iter().enumerate() {
                    self.w(if i > 0 { ", " } else { "" });
                    if let Some(e) = e {
                        self.pat_elem(e, binding);
                    }
                }
                if let Some(t) = rest {
                    self.w(if elems.is_empty() { "" } else { ", " });
                    self.rest(t, binding);
                }
                self.w("]");
            }
        }
        self.depth -= 1;
    }
}
