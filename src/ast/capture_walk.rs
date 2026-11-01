use super::*;
use std::collections::HashSet;

pub(super) enum CaptureNode<'a> {
    Stmt(&'a Statement),
    Expr(&'a Expression),
    Pattern(&'a Pattern),
}

/// Conservative scan for any construct that can hold on to, or reach into, the
/// lexical environment it runs in: a function, arrow function, class (method
/// and field closures), `with`, or any mention of `eval` (a direct eval can
/// create closures). Destructuring defaults and computed keys are visited too.
/// Explicit worklist, so a very long left-nested operator chain cannot
/// overflow the stack. Exhaustive over `Statement`/`Expression`/`Pattern`: a
/// new AST variant forces a decision here.
///
/// `collecting()` additionally records every identifier the walk reaches
/// (references, assignment targets and binding names alike) in `mentions`.
#[derive(Default)]
pub(crate) struct CaptureWalk<'a> {
    work: Vec<CaptureNode<'a>>,
    collect: bool,
    mode: WalkMode,
    nested: Vec<Nested<'a>>,
    /// `Record` and `Inside` modes: a direct `eval` or a `with` statement was
    /// met, so names can be reached that no mention accounts for.
    pub(crate) opaque: bool,
    /// `Inside` mode: the names already in `mentions`, so a large function
    /// body costs one entry per distinct name rather than one per mention.
    seen: HashSet<&'a str>,
    pub(crate) mentions: Vec<&'a str>,
}

/// What the walk does at a function, class, `with` or `eval`.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum WalkMode {
    /// Report it (`run` returns true).
    #[default]
    Stop,
    /// Note the function or class in `nested` and walk on past it.
    Record,
    /// Walk into it, collecting every identifier.
    Inside,
}

/// A function or class met by a `Record` walk.
enum Nested<'a> {
    Code(Option<&'a str>, &'a [Pattern], &'a [Statement]),
    Class(Option<&'a str>, Option<&'a Expression>, &'a [ClassElement]),
}

/// Every identifier mentioned inside the functions and classes nested in a
/// function literal (at any depth): the names a closure of the literal can
/// reach in the literal's scopes. Shadowing is ignored, which only makes the
/// set larger. `opaque` when a direct `eval` or `with` makes the set unsound.
pub(crate) struct NestedMentions<'a> {
    pub(crate) names: Vec<&'a str>,
    pub(crate) opaque: bool,
}

impl<'a> CaptureWalk<'a> {
    pub(crate) fn nested_mentions(
        params: &'a [Pattern],
        body: &'a [Statement],
    ) -> NestedMentions<'a> {
        let mut outer = CaptureWalk {
            mode: WalkMode::Record,
            ..CaptureWalk::default()
        };
        params.iter().for_each(|p| outer.push_pattern(p));
        body.iter().for_each(|s| outer.push_stmt(s));
        outer.run();
        let mut names = Vec::new();
        let mut opaque = outer.opaque;
        for nested in std::mem::take(&mut outer.nested) {
            let mut inside = CaptureWalk {
                collect: true,
                mode: WalkMode::Inside,
                ..CaptureWalk::default()
            };
            match nested {
                Nested::Code(name, params, body) => inside.walk_code(name, params, body),
                Nested::Class(name, heritage, elements) => {
                    inside.walk_class(name, heritage, elements)
                }
            }
            inside.run();
            opaque |= inside.opaque;
            names.append(&mut inside.mentions);
        }
        NestedMentions { names, opaque }
    }

    pub(crate) fn collecting() -> Self {
        Self {
            collect: true,
            ..Self::default()
        }
    }

    pub(crate) fn push_stmt(&mut self, s: &'a Statement) {
        self.work.push(CaptureNode::Stmt(s));
    }

    pub(crate) fn push_expr(&mut self, e: &'a Expression) {
        self.work.push(CaptureNode::Expr(e));
    }

    pub(crate) fn push_opt_expr(&mut self, e: Option<&'a Expression>) {
        if let Some(e) = e {
            self.push_expr(e);
        }
    }

    pub(crate) fn push_pattern(&mut self, p: &'a Pattern) {
        self.work.push(CaptureNode::Pattern(p));
    }

    pub(crate) fn push_decl(&mut self, d: &'a VariableDeclaration) {
        for declarator in &d.declarations {
            self.push_pattern(&declarator.pattern);
            self.push_opt_expr(declarator.init.as_ref());
        }
    }

    pub(crate) fn push_for_init(&mut self, init: Option<&'a ForInit>) {
        match init {
            Some(ForInit::Expression(e)) => self.push_expr(e),
            Some(ForInit::Variable(d)) => self.push_decl(d),
            None => {}
        }
    }

    pub(crate) fn push_for_in_of_left(&mut self, left: &'a ForInOfLeft) {
        match left {
            ForInOfLeft::Variable(d) => self.push_decl(d),
            ForInOfLeft::Pattern(p) => self.push_pattern(p),
            ForInOfLeft::Expression(e) => self.push_expr(e),
        }
    }

    fn push_key(&mut self, key: &'a PropertyKey) {
        if let PropertyKey::Computed(e) = key {
            self.push_expr(e);
        }
    }

    pub(crate) fn run(&mut self) -> bool {
        while let Some(node) = self.work.pop() {
            let captures = match node {
                CaptureNode::Stmt(s) => self.visit_stmt(s),
                CaptureNode::Expr(e) => self.visit_expr(e),
                CaptureNode::Pattern(p) => {
                    self.visit_pattern(p);
                    false
                }
            };
            if captures {
                return true;
            }
        }
        false
    }

    fn walk_code(&mut self, name: Option<&'a str>, params: &'a [Pattern], body: &'a [Statement]) {
        if let Some(name) = name {
            self.mention(name);
        }
        params.iter().for_each(|p| self.push_pattern(p));
        body.iter().for_each(|s| self.push_stmt(s));
    }

    fn walk_class(
        &mut self,
        name: Option<&'a str>,
        heritage: Option<&'a Expression>,
        elements: &'a [ClassElement],
    ) {
        if let Some(name) = name {
            self.mention(name);
        }
        self.push_opt_expr(heritage);
        for element in elements {
            match element {
                ClassElement::Method(m) => {
                    self.push_key(&m.key);
                    self.walk_code(None, &m.value.params, m.value.body.as_slice());
                }
                ClassElement::Property(p) | ClassElement::AutoAccessor(p) => {
                    self.push_key(&p.key);
                    self.push_opt_expr(p.value.as_ref());
                }
                ClassElement::StaticBlock(stmts) => stmts.iter().for_each(|s| self.push_stmt(s)),
            }
        }
    }

    /// A function met by the walk; true when the walk must stop.
    fn enter_code(
        &mut self,
        name: Option<&'a str>,
        params: &'a [Pattern],
        body: &'a [Statement],
    ) -> bool {
        match self.mode {
            WalkMode::Stop => return true,
            WalkMode::Record => self.nested.push(Nested::Code(name, params, body)),
            WalkMode::Inside => self.walk_code(name, params, body),
        }
        false
    }

    fn enter_class(
        &mut self,
        name: Option<&'a str>,
        heritage: Option<&'a Expression>,
        elements: &'a [ClassElement],
    ) -> bool {
        match self.mode {
            WalkMode::Stop => return true,
            WalkMode::Record => self.nested.push(Nested::Class(name, heritage, elements)),
            WalkMode::Inside => self.walk_class(name, heritage, elements),
        }
        false
    }

    fn mention(&mut self, name: &'a str) {
        if self.collect && (self.mode != WalkMode::Inside || self.seen.insert(name)) {
            self.mentions.push(name);
        }
    }

    fn visit_stmt(&mut self, stmt: &'a Statement) -> bool {
        match stmt {
            Statement::Empty
            | Statement::Debugger
            | Statement::Break(..)
            | Statement::Continue(_)
            | Statement::Return(None) => {}
            Statement::FunctionDeclaration(f) => {
                if self.enter_code(Some(&f.name), &f.params, f.body.as_slice()) {
                    return true;
                }
            }
            Statement::ClassDeclaration(c) => {
                if self.enter_class(Some(&c.name), c.super_class.as_deref(), &c.body) {
                    return true;
                }
            }
            Statement::With(object, body) => {
                if self.mode == WalkMode::Stop {
                    return true;
                }
                self.opaque = true;
                self.push_expr(object);
                self.push_stmt(body);
            }
            Statement::Expression(e) | Statement::Throw(e, _) | Statement::Return(Some(e)) => {
                self.push_expr(e)
            }
            Statement::Block(stmts) => stmts.iter().for_each(|s| self.push_stmt(s)),
            Statement::Variable(decl) => self.push_decl(decl),
            Statement::If(i) => {
                self.push_expr(&i.test);
                self.push_stmt(&i.consequent);
                if let Some(alt) = &i.alternate {
                    self.push_stmt(alt);
                }
            }
            Statement::While(w) => {
                self.push_expr(&w.test);
                self.push_stmt(&w.body);
            }
            Statement::DoWhile(d) => {
                self.push_expr(&d.test);
                self.push_stmt(&d.body);
            }
            Statement::For(f) => {
                self.push_for_init(f.init.as_ref());
                self.push_opt_expr(f.test.as_ref());
                self.push_opt_expr(f.update.as_deref());
                self.push_stmt(&f.body);
            }
            Statement::ForIn(f) => {
                self.push_for_in_of_left(&f.left);
                self.push_expr(&f.right);
                self.push_stmt(&f.body);
            }
            Statement::ForOf(f) => {
                self.push_for_in_of_left(&f.left);
                self.push_expr(&f.right);
                self.push_stmt(&f.body);
            }
            Statement::Try(t) => {
                t.block.iter().for_each(|s| self.push_stmt(s));
                if let Some(h) = &t.handler {
                    if let Some(param) = &h.param {
                        self.push_pattern(param);
                    }
                    h.body.iter().for_each(|s| self.push_stmt(s));
                }
                if let Some(f) = &t.finalizer {
                    f.iter().for_each(|s| self.push_stmt(s));
                }
            }
            Statement::Switch(s) => {
                self.push_expr(&s.discriminant);
                for case in &s.cases {
                    self.push_opt_expr(case.test.as_ref());
                    case.consequent.iter().for_each(|s| self.push_stmt(s));
                }
            }
            Statement::Labeled(_, inner) => self.push_stmt(inner),
        }
        false
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        match pattern {
            Pattern::Identifier(name) => self.mention(name),
            Pattern::Array(elems) => {
                for elem in elems.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                            self.push_pattern(p)
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(key, p) => {
                            self.push_key(key);
                            self.push_pattern(p);
                        }
                        ObjectPatternProperty::Rest(p) => self.push_pattern(p),
                        ObjectPatternProperty::Shorthand(name) => self.mention(name),
                    }
                }
            }
            Pattern::Assign(inner, default) => {
                self.push_pattern(inner);
                self.push_expr(default);
            }
            Pattern::Rest(inner) => self.push_pattern(inner),
            Pattern::MemberExpression(e) => self.push_expr(e),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expression) -> bool {
        match expr {
            Expression::Function(f) => {
                if self.enter_code(f.name.as_deref(), &f.params, f.body.as_slice()) {
                    return true;
                }
            }
            Expression::ArrowFunction(a) => {
                if self.enter_code(None, &a.params, a.body.body().as_slice()) {
                    return true;
                }
            }
            Expression::Class(c) => {
                if self.enter_class(c.name.as_deref(), c.super_class.as_deref(), &c.body) {
                    return true;
                }
            }
            Expression::Identifier(name) => {
                self.mention(name);
                if name == "eval" {
                    if self.mode == WalkMode::Stop {
                        return true;
                    }
                    self.opaque = true;
                }
            }
            Expression::Literal(_)
            | Expression::This
            | Expression::Super
            | Expression::ImportMeta
            | Expression::NewTarget
            | Expression::PrivateIdentifier(_) => {}
            Expression::Array(elems, _) => {
                for e in elems.iter().flatten() {
                    self.push_expr(e);
                }
            }
            Expression::Object(props, _) => {
                for p in props {
                    self.push_key(&p.key);
                    self.push_expr(&p.value);
                }
            }
            Expression::Unary(_, e)
            | Expression::Update(.., e, _)
            | Expression::Spread(e)
            | Expression::PreIterated(e)
            | Expression::ClassProtoParent(e, _)
            | Expression::Await(e, _)
            | Expression::Typeof(e)
            | Expression::Void(e)
            | Expression::Delete(e) => self.push_expr(e),
            Expression::Yield(arg, _) => {
                if let Some(arg) = arg {
                    self.push_expr(arg);
                }
            }
            Expression::Binary(_, l, r, _)
            | Expression::Logical(_, l, r)
            | Expression::Assign(_, l, r, _)
            | Expression::BoundCallee(l, r)
            | Expression::ResolvedHeritage(l, r)
            | Expression::OptionalChain(l, r) => {
                self.push_expr(l);
                self.push_expr(r);
            }
            Expression::Conditional(t, c, a) => {
                self.push_expr(t);
                self.push_expr(c);
                self.push_expr(a);
            }
            Expression::Call(callee, args, _) | Expression::New(callee, args, _) => {
                self.push_expr(callee);
                args.iter().for_each(|a| self.push_expr(a));
            }
            Expression::Member(object, property, _) => {
                self.push_expr(object);
                if let MemberProperty::Computed(e) = property {
                    self.push_expr(e);
                }
            }
            Expression::Comma(exprs) | Expression::Sequence(exprs) => {
                exprs.iter().for_each(|e| self.push_expr(e));
            }
            Expression::TaggedTemplate(tag, tpl) => {
                self.push_expr(tag);
                tpl.expressions.iter().for_each(|e| self.push_expr(e));
            }
            Expression::Template(tpl) => {
                tpl.expressions.iter().for_each(|e| self.push_expr(e));
            }
            Expression::Import(spec, opts, _)
            | Expression::ImportDefer(spec, opts)
            | Expression::ImportSource(spec, opts) => {
                self.push_expr(spec);
                if let Some(opts) = opts {
                    self.push_expr(opts);
                }
            }
        }
        false
    }
}
