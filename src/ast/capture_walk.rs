use super::*;

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
    pub(crate) mentions: Vec<&'a str>,
}

impl<'a> CaptureWalk<'a> {
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

    fn mention(&mut self, name: &'a str) {
        if self.collect {
            self.mentions.push(name);
        }
    }

    fn visit_stmt(&mut self, stmt: &'a Statement) -> bool {
        match stmt {
            Statement::Empty
            | Statement::Debugger
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Return(None) => {}
            Statement::FunctionDeclaration(_)
            | Statement::ClassDeclaration(_)
            | Statement::With(..) => return true,
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
            Expression::Function(_) | Expression::ArrowFunction(_) | Expression::Class(_) => {
                return true;
            }
            Expression::Identifier(name) => {
                self.mention(name);
                return name == "eval";
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
            | Expression::Update(_, _, e)
            | Expression::Spread(e)
            | Expression::PreIterated(e)
            | Expression::ClassProtoParent(e)
            | Expression::Await(e)
            | Expression::Typeof(e)
            | Expression::Void(e)
            | Expression::Delete(e) => self.push_expr(e),
            Expression::Yield(arg, _) => {
                if let Some(arg) = arg {
                    self.push_expr(arg);
                }
            }
            Expression::Binary(_, l, r)
            | Expression::Logical(_, l, r)
            | Expression::Assign(_, l, r)
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
            Expression::Import(spec, opts)
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
