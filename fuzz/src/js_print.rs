//! Renders a `js_ast::Program` to JavaScript source that parses almost
//! always. Context rules (where `break`, `return`, `yield`, `await`, `super`,
//! labels and private names are legal) are enforced while printing: an AST
//! node that would be an early error in its position degrades to a harmless
//! one, so the engine sees syntactically valid programs. Operands are always
//! parenthesised, which sidesteps precedence and ASI hazards.

use crate::js_ast::*;
use crate::js_pools::*;

pub const MAX_DEPTH: u32 = 10;
/// Loop bodies share the `$g` counter, so a program runs at most this many
/// iterations in total instead of burning the step budget in an endless loop.
const LOOP_FUEL: u32 = 48;
const SOFT_CAP: usize = 16_000;

#[derive(Default, Clone)]
pub(crate) struct Flow {
    pub in_fn: bool,
    pub in_gen: bool,
    pub in_async: bool,
    pub in_loop: bool,
    pub in_switch: bool,
    pub super_prop: bool,
    pub super_call: bool,
    pub new_target: bool,
    pub args: bool,
    pub labels: Vec<(usize, bool)>,
}

pub(crate) enum FnKind {
    Plain,
    Method,
    Ctor { derived: bool },
    Arrow,
    Init,
}

pub(crate) struct Saved {
    flow: Flow,
    strict: bool,
}

pub(crate) struct Printer {
    pub out: String,
    pub flow: Flow,
    pub depth: u32,
    pub strict: bool,
    scopes: Vec<Vec<String>>,
    pub(crate) privs: Vec<Vec<String>>,
}

pub fn print(program: &Program) -> String {
    let mut p = Printer {
        out: String::new(),
        flow: Flow::default(),
        depth: 0,
        strict: program.strict,
        scopes: vec![vec![]],
        privs: vec![],
    };
    if program.strict {
        p.w("\"use strict\";\n");
    }
    p.w("var a, b, c, d, e, f, $g = 0;\n");
    p.stmts(&program.body);
    p.out
}

pub(crate) fn is_ident_name(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn pick<'a>(pool: &[&'a str], ix: Ix) -> &'a str {
    pool[ix as usize % pool.len()]
}

pub(crate) fn pick_str(pool: &[&'static str], ix: Ix) -> &'static str {
    pick(pool, ix)
}

impl Printer {
    pub fn w(&mut self, s: &str) {
        self.out.push_str(s);
    }

    pub fn over(&self) -> bool {
        self.depth > MAX_DEPTH || self.out.len() > SOFT_CAP
    }

    pub fn name(&self, ix: Ix) -> String {
        let lex: Vec<&String> = self.scopes.iter().flatten().collect();
        let i = ix as usize % (VARS.len() + lex.len());
        match i.checked_sub(VARS.len()) {
            None => VARS[i].to_string(),
            Some(j) => lex[j].clone(),
        }
    }

    fn free_name(&self, ix: Ix) -> Option<String> {
        let scope = self.scopes.last()?;
        (0..LEX_COUNT)
            .map(|k| format!("n{}", (ix as usize + k) % LEX_COUNT))
            .find(|n| !scope.contains(n))
    }

    fn bind(&mut self, name: String) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(name);
        }
    }

    pub fn quoted(&mut self, s: &str) {
        self.w(&format!("\"{s}\""));
    }

    pub fn enter(&mut self, kind: FnKind, is_async: bool, is_gen: bool) -> Saved {
        let old = self.flow.clone();
        let mut f = Flow {
            in_fn: true,
            in_gen: is_gen,
            in_async: is_async,
            new_target: true,
            args: true,
            ..Flow::default()
        };
        match kind {
            FnKind::Plain => {}
            FnKind::Method => f.super_prop = true,
            FnKind::Ctor { derived } => {
                f.super_prop = true;
                f.super_call = derived;
            }
            FnKind::Arrow => {
                f.super_prop = old.super_prop;
                f.super_call = old.super_call;
                f.new_target = old.new_target;
                f.args = old.args;
            }
            FnKind::Init => {
                f.in_fn = false;
                f.args = false;
                f.super_prop = true;
            }
        }
        self.scopes.push(vec![]);
        Saved {
            flow: std::mem::replace(&mut self.flow, f),
            strict: self.strict,
        }
    }

    pub fn leave(&mut self, saved: Saved) {
        self.scopes.pop();
        self.flow = saved.flow;
        self.strict = saved.strict;
    }

    pub(crate) fn params(&mut self, p: &Params) -> bool {
        let n = (p.count % 4) as usize;
        let names = |k: usize| PARAMS[..k].join(", ");
        let (is_async, is_gen) = (self.flow.in_async, self.flow.in_gen);
        let super_call = std::mem::replace(&mut self.flow.super_call, false);
        self.flow.in_async = false;
        self.flow.in_gen = false;
        let simple = matches!(p.style, PStyle::Simple);
        match &p.style {
            PStyle::Simple => self.w(&names(n)),
            PStyle::Defaults(e) => {
                for name in &PARAMS[..n] {
                    self.bind(name.to_string());
                }
                if n > 0 {
                    self.w(&format!("{} = (", PARAMS[0]));
                    self.expr(e);
                    self.w(")");
                    if n > 1 {
                        self.w(&format!(", {}", PARAMS[1..n].join(", ")));
                    }
                }
            }
            PStyle::Rest => {
                let n = n.max(1);
                if n > 1 {
                    self.w(&format!("{}, ", names(n - 1)));
                }
                self.w(&format!("...{}", PARAMS[n - 1]));
                for name in &PARAMS[..n] {
                    self.bind(name.to_string());
                }
            }
            PStyle::DestrObj => self.w(&format!("{{{}}}", names(n))),
            PStyle::DestrArr => self.w(&format!("[{}]", names(n))),
        }
        if !matches!(p.style, PStyle::Defaults(_) | PStyle::Rest) {
            for name in &PARAMS[..n] {
                self.bind(name.to_string());
            }
        }
        self.flow.in_async = is_async;
        self.flow.in_gen = is_gen;
        self.flow.super_call = super_call;
        simple
    }

    pub fn func_tail(&mut self, kind: FnKind, d: &FuncDef) {
        let saved = self.enter(kind, d.is_async, d.is_gen);
        self.w("(");
        let simple = self.params(&d.params);
        self.w(") {\n");
        if d.strict && simple {
            self.w("\"use strict\";\n");
            self.strict = true;
        }
        self.stmts(&d.body);
        self.w("}");
        self.leave(saved);
    }

    pub fn accessor_tail(&mut self, setter: bool, body: &Block) {
        let saved = self.enter(FnKind::Method, false, false);
        if setter {
            self.bind(PARAMS[0].to_string());
        }
        self.w(if setter { "(p0) {\n" } else { "() {\n" });
        self.stmts(body);
        self.w("}");
        self.leave(saved);
    }

    pub fn func_header(&mut self, d: &FuncDef, name: &str) {
        self.w(if d.is_async {
            "async function"
        } else {
            "function"
        });
        self.w(if d.is_gen { "* " } else { " " });
        self.w(name);
    }

    pub fn arrow(&mut self, a: &Arrow) {
        let saved = self.enter(FnKind::Arrow, a.is_async, false);
        self.w(if a.is_async { "async (" } else { "(" });
        self.params(&a.params);
        self.w(") => ");
        match &a.body {
            ArrowBody::Expr(e) => self.paren(e),
            ArrowBody::Block(b) => {
                self.w("{\n");
                self.stmts(b);
                self.w("}");
            }
        }
        self.leave(saved);
    }

    pub fn stmts(&mut self, b: &Block) {
        for s in b {
            self.stmt(s);
        }
    }

    fn block(&mut self, b: &Block) {
        self.w("{\n");
        self.scopes.push(vec![]);
        self.stmts(b);
        self.scopes.pop();
        self.w("}");
    }

    fn loop_block(&mut self, b: &Block) {
        let saved = std::mem::replace(&mut self.flow.in_loop, true);
        self.w("{\nif ($g++ > ");
        self.w(&LOOP_FUEL.to_string());
        self.w(") break;\n");
        self.scopes.push(vec![]);
        self.stmts(b);
        self.scopes.pop();
        self.w("}");
        self.flow.in_loop = saved;
    }

    fn label_ok(&self, l: Option<Ix>, loops_only: bool) -> Option<usize> {
        let l = l? as usize % LABELS.len();
        self.flow
            .labels
            .iter()
            .any(|(x, is_loop)| *x == l && (*is_loop || !loops_only))
            .then_some(l)
    }

    fn for_binding(&mut self, b: &ForBinding) -> Option<String> {
        match b {
            ForBinding::Var(p) => {
                self.w("var ");
                self.pat(p, true);
            }
            ForBinding::Target(t) => self.target(t),
            ForBinding::Let(i) | ForBinding::Const(i) => {
                let name = self.free_name(*i).unwrap_or_else(|| "n0".to_string());
                self.w(if matches!(b, ForBinding::Let(_)) {
                    "let "
                } else {
                    "const "
                });
                self.w(&name);
                return Some(name);
            }
        }
        None
    }

    pub fn stmt(&mut self, s: &Stmt) {
        if self.over() {
            self.w(";\n");
            return;
        }
        self.depth += 1;
        self.stmt_inner(s);
        self.depth -= 1;
        self.w("\n");
    }

    fn stmt_inner(&mut self, s: &Stmt) {
        match s {
            Stmt::Expr(e) => {
                self.w("(");
                self.expr(e);
                self.w(");");
            }
            Stmt::Var(p, init) => {
                self.w("var ");
                self.pat(p, true);
                match (init, p) {
                    (Some(e), _) => {
                        self.w(" = (");
                        self.expr(e);
                        self.w(")");
                    }
                    (None, Pat::Obj(..)) => self.w(" = {}"),
                    (None, Pat::Arr(..)) => self.w(" = []"),
                    _ => {}
                }
                self.w(";");
            }
            Stmt::Lex(is_const, ix, init) => {
                let name = self.free_name(*ix);
                if let Some(n) = &name {
                    self.w(&format!(
                        "{} {n} = (",
                        if *is_const { "const" } else { "let" }
                    ));
                } else {
                    self.w("((");
                }
                self.expr(init);
                self.w(if name.is_some() { ");" } else { "));" });
                if let Some(n) = name {
                    self.bind(n);
                }
            }
            Stmt::If(c, t, e) => {
                self.w("if (");
                self.expr(c);
                self.w(") ");
                self.block(t);
                if !e.is_empty() {
                    self.w(" else ");
                    self.block(e);
                }
            }
            Stmt::For(init, test, update, body) => {
                self.scopes.push(vec![]);
                self.w("for (");
                match init {
                    ForInit::None => {}
                    ForInit::Expr(e) => self.paren(e),
                    ForInit::Var(i, e) => {
                        self.w(&format!("var {} = (", pick(&VARS, *i)));
                        self.expr(e);
                        self.w(")");
                    }
                    ForInit::Let(i, e) => {
                        let name = self.free_name(*i).unwrap_or_else(|| "n0".to_string());
                        self.w(&format!("let {name} = ("));
                        self.expr(e);
                        self.w(")");
                        self.bind(name);
                    }
                }
                self.w("; ");
                for (i, part) in [test, update].into_iter().enumerate() {
                    if let Some(e) = part {
                        self.paren(e);
                    }
                    self.w(if i == 0 { "; " } else { "" });
                }
                self.w(") ");
                self.loop_block(body);
                self.scopes.pop();
            }
            Stmt::ForIn(b, e, body) | Stmt::ForOf(b, e, body, _) => {
                let is_of = matches!(s, Stmt::ForOf(..));
                let is_await = matches!(s, Stmt::ForOf(_, _, _, true)) && self.flow.in_async;
                self.scopes.push(vec![]);
                self.w(if is_await { "for await (" } else { "for (" });
                let bound = self.for_binding(b);
                self.w(if is_of { " of (" } else { " in (" });
                self.expr(e);
                self.w(")) ");
                if let Some(n) = bound {
                    self.bind(n);
                }
                self.loop_block(body);
                self.scopes.pop();
            }
            Stmt::While(c, b) => {
                self.w("while (");
                self.expr(c);
                self.w(") ");
                self.loop_block(b);
            }
            Stmt::DoWhile(b, c) => {
                self.w("do ");
                self.loop_block(b);
                self.w(" while (");
                self.expr(c);
                self.w(");");
            }
            Stmt::Block(b) => self.block(b),
            Stmt::Try(b, catch, fin) => {
                self.w("try ");
                self.block(b);
                if let Some(c) = catch {
                    self.scopes.push(vec![]);
                    match c.param {
                        Some(p) => {
                            let name = pick(&CATCH, p);
                            self.w(&format!(" catch ({name}) "));
                            self.bind(name.to_string());
                        }
                        None => self.w(" catch "),
                    }
                    self.block(&c.body);
                    self.scopes.pop();
                }
                if let Some(f) = fin {
                    self.w(" finally ");
                    self.block(f);
                } else if catch.is_none() {
                    self.w(" finally {}");
                }
            }
            Stmt::Throw(e) => {
                self.w("throw (");
                self.expr(e);
                self.w(");");
            }
            Stmt::Return(e) => match (self.flow.in_fn, e) {
                (true, Some(e)) => {
                    self.w("return (");
                    self.expr(e);
                    self.w(");");
                }
                (true, None) => self.w("return;"),
                _ => self.w(";"),
            },
            Stmt::Break(l) => match self.label_ok(*l, false) {
                Some(l) => self.w(&format!("break {};", LABELS[l])),
                None if self.flow.in_loop || self.flow.in_switch => self.w("break;"),
                None => self.w(";"),
            },
            Stmt::Continue(l) => match self.label_ok(*l, true) {
                Some(l) => self.w(&format!("continue {};", LABELS[l])),
                None if self.flow.in_loop => self.w("continue;"),
                None => self.w(";"),
            },
            Stmt::Switch(d, cases) => {
                self.w("switch (");
                self.expr(d);
                self.w(") {\n");
                self.scopes.push(vec![]);
                let saved = std::mem::replace(&mut self.flow.in_switch, true);
                let mut has_default = false;
                for c in cases {
                    match &c.test {
                        None if !has_default => {
                            has_default = true;
                            self.w("default:\n");
                        }
                        t => {
                            self.w("case (");
                            match t {
                                Some(e) => self.expr(e),
                                None => self.w("0"),
                            }
                            self.w("):\n");
                        }
                    }
                    self.stmts(&c.body);
                }
                self.flow.in_switch = saved;
                self.scopes.pop();
                self.w("}");
            }
            Stmt::Labeled(l, inner) => {
                let l = *l as usize % LABELS.len();
                if self.flow.labels.iter().any(|(x, _)| *x == l) {
                    return self.stmt_inner(inner);
                }
                let is_loop = matches!(
                    **inner,
                    Stmt::For(..)
                        | Stmt::ForIn(..)
                        | Stmt::ForOf(..)
                        | Stmt::While(..)
                        | Stmt::DoWhile(..)
                );
                let wrap = matches!(**inner, Stmt::Lex(..) | Stmt::Func(..) | Stmt::Class(..));
                self.w(&format!("{}: ", LABELS[l]));
                self.flow.labels.push((l, is_loop));
                if wrap {
                    self.w("{\n");
                    self.scopes.push(vec![]);
                    self.stmt(inner);
                    self.scopes.pop();
                    self.w("}");
                } else {
                    self.stmt_inner(inner);
                }
                self.flow.labels.pop();
            }
            Stmt::With(e, b) => {
                if !self.strict {
                    self.w("with (");
                    self.expr(e);
                    self.w(") ");
                }
                self.block(b);
            }
            Stmt::Func(ix, d) => match self.free_name(*ix) {
                Some(name) => {
                    self.bind(name.clone());
                    self.func_header(d, &name);
                    self.func_tail(FnKind::Plain, d);
                }
                None => self.w(";"),
            },
            Stmt::Class(ix, c) => match self.free_name(*ix) {
                Some(name) => {
                    self.bind(name.clone());
                    self.class(c, &name);
                }
                None => self.w(";"),
            },
        }
    }
}
