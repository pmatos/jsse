/// AST node types for ECMAScript.
/// Each node represents a syntactic element from the spec.
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMPLATE_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_template_id() -> u64 {
    NEXT_TEMPLATE_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Debug)]
pub struct SourceText {
    source: Rc<str>,
    start: usize,
    end: usize,
}

impl SourceText {
    pub fn new(source: Rc<str>, start: usize, end: usize) -> Self {
        Self { source, start, end }
    }

    pub fn as_str(&self) -> &str {
        &self.source[self.start..self.end]
    }
}

impl From<String> for SourceText {
    fn from(source: String) -> Self {
        let end = source.len();
        Self {
            source: Rc::from(source),
            start: 0,
            end,
        }
    }
}

impl fmt::Display for SourceText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceType {
    Script,
    Module,
}

/// Dense identifier for a call IC site within a single `Body`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct CallSiteId(pub u32);

/// Dense identifier for a property-access IC site within a single `Body`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct PropSiteId(pub u32);

impl CallSiteId {
    pub const UNASSIGNED: Self = Self(u32::MAX);
}

impl PropSiteId {
    pub const UNASSIGNED: Self = Self(u32::MAX);
}

/// Metadata describing the number of IC sites in a `Body`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BodyIcInfo {
    pub call_site_count: u32,
    pub prop_site_count: u32,
    pub assigned: bool,
}

/// A unit of executable ECMAScript syntax: a script, module, or function body.
/// Carries the statement vector and IC metadata; the runtime cache lives in the
/// interpreter, keyed by the body's identity.
#[derive(Clone, Debug)]
pub struct Body {
    pub statements: Rc<Vec<Statement>>,
    pub ic: BodyIcInfo,
}

impl Body {
    pub fn new(statements: Vec<Statement>) -> Self {
        Self {
            statements: Rc::new(statements),
            ic: BodyIcInfo::default(),
        }
    }

    pub fn as_slice(&self) -> &[Statement] {
        &self.statements
    }
}

/// Assign dense `CallSiteId` and `PropSiteId` values to every call, new, and
/// member site in `body`, and record the final counts in `body.ic`.
/// This is a single shared pass used by the parser, generator transform,
/// `eval`, and `new Function`.
pub fn assign_ic_sites(body: &mut Body) {
    let mut call_id = 0u32;
    let mut prop_id = 0u32;
    for stmt in Rc::make_mut(&mut body.statements).iter_mut() {
        assign_stmt_sites(stmt, &mut call_id, &mut prop_id);
    }
    body.ic.call_site_count = call_id;
    body.ic.prop_site_count = prop_id;
    body.ic.assigned = true;
}

/// Assign IC sites to a nested body that was created synthetically (e.g. an
/// arrow expression body or a dynamic `Function` body). Returns the number of
/// call and property sites found.
pub fn assign_ic_sites_for_body(body: &mut Body) -> (u32, u32) {
    let before_call = body.ic.call_site_count;
    let before_prop = body.ic.prop_site_count;
    if !body.ic.assigned {
        assign_ic_sites(body);
    }
    (
        body.ic.call_site_count - before_call,
        body.ic.prop_site_count - before_prop,
    )
}

/// Assign dense IC site ids to all call/new/member sites in a module-level
/// program. The module's top-level items are not stored in a `Body`, but they
/// share a single dense namespace keyed by the program's `body` field. This
/// keeps IC sites on module top-level executable expressions valid while the
/// interpreter is executing module items.
pub fn assign_ic_sites_for_module(program: &mut Program) {
    if program.source_type == SourceType::Script {
        assign_ic_sites(&mut program.body);
        return;
    }

    let mut call_id = 0u32;
    let mut prop_id = 0u32;
    for item in program.module_items.iter_mut() {
        assign_module_item_sites(item, &mut call_id, &mut prop_id);
    }
    program.body.ic.call_site_count = call_id;
    program.body.ic.prop_site_count = prop_id;
    program.body.ic.assigned = true;
}

fn assign_module_item_sites(item: &mut ModuleItem, call_id: &mut u32, prop_id: &mut u32) {
    match item {
        ModuleItem::Statement(stmt) => assign_stmt_sites(stmt, call_id, prop_id),
        ModuleItem::ImportDeclaration(_) => {}
        ModuleItem::ExportDeclaration(export) => assign_export_sites(export, call_id, prop_id),
    }
}

fn assign_export_sites(export: &mut ExportDeclaration, call_id: &mut u32, prop_id: &mut u32) {
    match export {
        ExportDeclaration::Named { declaration, .. } => {
            if let Some(decl) = declaration.as_mut() {
                assign_stmt_sites(decl, call_id, prop_id);
            }
        }
        ExportDeclaration::Default(expr) => assign_expr_sites(expr, call_id, prop_id),
        ExportDeclaration::DefaultFunction(f) => {
            assign_ic_sites(&mut f.body);
        }
        ExportDeclaration::DefaultClass(c) => assign_class_sites(c, call_id, prop_id),
        ExportDeclaration::All { .. } => {}
    }
}

#[derive(Clone, Debug)]
pub struct Program {
    pub source_type: SourceType,
    pub body: Body,
    pub module_items: Vec<ModuleItem>,
    pub body_is_strict: bool,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ModuleItem {
    Statement(Statement),
    ImportDeclaration(ImportDeclaration),
    ExportDeclaration(ExportDeclaration),
}

#[derive(Clone, Debug)]
pub struct ImportDeclaration {
    pub specifiers: Vec<ImportSpecifier>,
    pub source: String,
    pub attributes: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub enum ImportSpecifier {
    Named { imported: String, local: String },
    Default(String),
    Namespace(String),
    DeferredNamespace(String),
    SourcePhase(String),
}

#[derive(Clone, Debug)]
pub enum ExportDeclaration {
    Named {
        specifiers: Vec<ExportSpecifier>,
        source: Option<String>,
        declaration: Option<Box<Statement>>,
    },
    Default(Box<Expression>),
    DefaultFunction(FunctionDecl),
    DefaultClass(ClassDecl),
    All {
        exported: Option<String>,
        source: String,
    },
}

#[derive(Clone, Debug)]
pub struct ExportSpecifier {
    pub local: String,
    pub exported: String,
}

#[derive(Clone, Debug)]
pub enum Statement {
    Empty,
    Expression(Expression),
    Block(Vec<Statement>),
    Variable(VariableDeclaration),
    If(IfStatement),
    While(WhileStatement),
    DoWhile(DoWhileStatement),
    For(ForStatement),
    ForIn(ForInStatement),
    ForOf(ForOfStatement),
    Return(Option<Expression>),
    Break(Option<String>),
    Continue(Option<String>),
    Throw(Expression),
    Try(TryStatement),
    Switch(SwitchStatement),
    Labeled(String, Box<Statement>),
    With(Expression, Box<Statement>),
    Debugger,
    FunctionDeclaration(FunctionDecl),
    ClassDeclaration(ClassDecl),
}

#[derive(Clone, Debug)]
pub struct VariableDeclaration {
    pub kind: VarKind,
    pub declarations: Vec<VariableDeclarator>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarKind {
    Var,
    Let,
    Const,
    Using,
    AwaitUsing,
}

#[derive(Clone, Debug)]
pub struct VariableDeclarator {
    pub pattern: Pattern,
    pub init: Option<Expression>,
}

#[derive(Clone, Debug)]
pub enum Pattern {
    Identifier(String),
    Array(Vec<Option<ArrayPatternElement>>),
    Object(Vec<ObjectPatternProperty>),
    Assign(Box<Pattern>, Box<Expression>),
    Rest(Box<Pattern>),
    MemberExpression(Box<Expression>),
}

#[derive(Clone, Debug)]
pub enum ArrayPatternElement {
    Pattern(Pattern),
    Rest(Pattern),
}

#[derive(Clone, Debug)]
pub enum ObjectPatternProperty {
    KeyValue(PropertyKey, Pattern),
    Shorthand(String),
    Rest(Pattern),
}

#[derive(Clone, Debug)]
pub enum Expression {
    Literal(Literal),
    Identifier(String),
    This,
    Super,
    Array(Vec<Option<Expression>>, bool),
    Object(Vec<Property>),
    Function(FunctionExpr),
    ArrowFunction(ArrowFunction),
    Class(ClassExpr),
    Unary(UnaryOp, Box<Expression>),
    Binary(BinaryOp, Box<Expression>, Box<Expression>),
    Logical(LogicalOp, Box<Expression>, Box<Expression>),
    Update(UpdateOp, bool, Box<Expression>), // op, prefix, argument
    Assign(AssignOp, Box<Expression>, Box<Expression>),
    Conditional(Box<Expression>, Box<Expression>, Box<Expression>),
    /// Function call `f(args)` / `obj.method(args)`. Third field is a
    /// per-body call IC site identifier (issue #71, Phase 3).
    Call(Box<Expression>, Vec<Expression>, CallSiteId),
    /// Constructor invocation `new F(args)`. Carries its own call IC site id —
    /// not yet read in Phase-3 v1; the slot is allocated for forward
    /// compatibility (issue #71).
    New(Box<Expression>, Vec<Expression>, CallSiteId),
    /// Property access `obj.x` / `obj[key]`. Third field is a per-body
    /// property-access IC site identifier (issue #71). The runtime cache slot
    /// lives in the interpreter, keyed by the body identity.
    Member(Box<Expression>, MemberProperty, PropSiteId),
    OptionalChain(Box<Expression>, Box<Expression>),
    #[allow(dead_code)]
    Comma(Vec<Expression>),
    Spread(Box<Expression>),
    Yield(Option<Box<Expression>>, bool), // expr, delegate
    Await(Box<Expression>),
    TaggedTemplate(Box<Expression>, TemplateLiteral),
    Template(TemplateLiteral),
    Typeof(Box<Expression>),
    Void(Box<Expression>),
    Delete(Box<Expression>),
    Sequence(Vec<Expression>),
    Import(Box<Expression>, Option<Box<Expression>>), // dynamic import(specifier, options?)
    ImportDefer(Box<Expression>, Option<Box<Expression>>), // import.defer(specifier, options?)
    ImportSource(Box<Expression>, Option<Box<Expression>>), // import.source(specifier, options?)
    ImportMeta,
    NewTarget,
    PrivateIdentifier(String),
}

#[derive(Clone, Debug)]
pub enum MemberProperty {
    Dot(String),
    Computed(Box<Expression>),
    Private(String),
}

#[derive(Clone, Debug)]
pub enum Literal {
    Null,
    Boolean(bool),
    Number(f64),
    String(Vec<u16>),
    BigInt(String),
    RegExp(String, String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Minus,
    Plus,
    Not,
    BitNot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Exp,
    Eq,
    NotEq,
    StrictEq,
    StrictNotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    LShift,
    RShift,
    URShift,
    BitAnd,
    BitOr,
    BitXor,
    In,
    Instanceof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
    NullishCoalescing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateOp {
    Increment,
    Decrement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    ExpAssign,
    LShiftAssign,
    RShiftAssign,
    URShiftAssign,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
    LogicalAndAssign,
    LogicalOrAssign,
    NullishAssign,
}

#[derive(Clone, Debug)]
pub struct Property {
    pub key: PropertyKey,
    pub value: Expression,
    pub kind: PropertyKind,
    pub computed: bool,
    pub shorthand: bool,
    pub method: bool,
}

#[derive(Clone, Debug)]
pub enum PropertyKey {
    Identifier(String),
    String(String),
    Number(f64),
    Computed(Box<Expression>),
    Private(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyKind {
    Init,
    Get,
    Set,
}

#[derive(Clone, Debug)]
pub struct IfStatement {
    pub test: Expression,
    pub consequent: Box<Statement>,
    pub alternate: Option<Box<Statement>>,
}

#[derive(Clone, Debug)]
pub struct WhileStatement {
    pub test: Expression,
    pub body: Box<Statement>,
}

#[derive(Clone, Debug)]
pub struct DoWhileStatement {
    pub test: Expression,
    pub body: Box<Statement>,
}

#[derive(Clone, Debug)]
pub struct ForStatement {
    pub init: Option<ForInit>,
    pub test: Option<Expression>,
    pub update: Option<Expression>,
    pub body: Box<Statement>,
}

#[derive(Clone, Debug)]
pub enum ForInit {
    Variable(VariableDeclaration),
    Expression(Expression),
}

#[derive(Clone, Debug)]
pub struct ForInStatement {
    pub left: ForInOfLeft,
    pub right: Expression,
    pub body: Box<Statement>,
}

#[derive(Clone, Debug)]
pub struct ForOfStatement {
    pub left: ForInOfLeft,
    pub right: Expression,
    pub body: Box<Statement>,
    pub is_await: bool,
}

#[derive(Clone, Debug)]
pub enum ForInOfLeft {
    Variable(VariableDeclaration),
    Pattern(Pattern),
    Expression(Expression),
}

#[derive(Clone, Debug)]
pub struct TryStatement {
    pub block: Vec<Statement>,
    pub handler: Option<CatchClause>,
    pub finalizer: Option<Vec<Statement>>,
}

#[derive(Clone, Debug)]
pub struct CatchClause {
    pub param: Option<Pattern>,
    pub body: Vec<Statement>,
}

#[derive(Clone, Debug)]
pub struct SwitchStatement {
    pub discriminant: Expression,
    pub cases: Vec<SwitchCase>,
}

#[derive(Clone, Debug)]
pub struct SwitchCase {
    pub test: Option<Expression>,
    pub consequent: Vec<Statement>,
}

#[derive(Clone, Debug)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Pattern>,
    pub body: Body,
    pub is_async: bool,
    pub is_generator: bool,
    pub source_text: Option<SourceText>,
    pub body_is_strict: bool,
}

#[derive(Clone, Debug)]
pub struct FunctionExpr {
    pub name: Option<String>,
    pub params: Vec<Pattern>,
    pub body: Body,
    pub is_async: bool,
    pub is_generator: bool,
    pub source_text: Option<SourceText>,
    pub body_is_strict: bool,
}

#[derive(Clone, Debug)]
pub struct ArrowFunction {
    pub params: Vec<Pattern>,
    pub body: ArrowBody,
    pub is_async: bool,
    pub source_text: Option<SourceText>,
    pub body_is_strict: bool,
}

#[derive(Clone, Debug)]
pub enum ArrowBody {
    /// Concise arrow-function body: `() => expr`. The `Body` contains a single
    /// `Statement::Expression` so it participates in the same per-body IC
    /// numbering and store as a block arrow body.
    Expression(Body),
    /// Block arrow-function body: `() => { ... }`.
    Block(Body),
}

impl ArrowBody {
    pub fn body(&self) -> &Body {
        match self {
            ArrowBody::Expression(b) | ArrowBody::Block(b) => b,
        }
    }

    pub fn body_mut(&mut self) -> &mut Body {
        match self {
            ArrowBody::Expression(b) | ArrowBody::Block(b) => b,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClassDecl {
    pub name: String,
    pub super_class: Option<Box<Expression>>,
    pub body: Vec<ClassElement>,
    pub source_text: Option<SourceText>,
}

#[derive(Clone, Debug)]
pub struct ClassExpr {
    pub name: Option<String>,
    pub super_class: Option<Box<Expression>>,
    pub body: Vec<ClassElement>,
    pub source_text: Option<SourceText>,
}

#[derive(Clone, Debug)]
pub enum ClassElement {
    Method(ClassMethod),
    Property(ClassProperty),
    AutoAccessor(ClassProperty),
    StaticBlock(Vec<Statement>),
}

#[derive(Clone, Debug)]
pub struct ClassMethod {
    pub key: PropertyKey,
    pub kind: ClassMethodKind,
    pub value: FunctionExpr,
    pub is_static: bool,
    pub computed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassMethodKind {
    Method,
    Get,
    Set,
    Constructor,
}

#[derive(Clone, Debug)]
pub struct ClassProperty {
    pub key: PropertyKey,
    pub value: Option<Expression>,
    pub is_static: bool,
    pub computed: bool,
}

impl Expression {
    /// Per spec §13.2.1.2 — returns true only for function/class/arrow
    /// expressions that have no binding name of their own.
    pub fn is_anonymous_function_definition(&self) -> bool {
        match self {
            Expression::Function(f) => f.name.as_ref().is_none_or(|n| n.is_empty()),
            Expression::ArrowFunction(_) => true,
            Expression::Class(c) => c.name.as_ref().is_none_or(|n| n.is_empty()),
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TemplateLiteral {
    pub id: u64,
    pub quasis: Vec<Option<Vec<u16>>>,
    pub raw_quasis: Vec<String>,
    pub expressions: Vec<Expression>,
}

/// Check if a function (body + params) references the `arguments` identifier.
/// Also checks parameter default expressions (which can reference arguments).
pub fn func_uses_arguments(params: &[Pattern], body: &Body) -> bool {
    params_use_arguments(params) || stmts_use_arguments(body.as_slice())
}

/// A "simple" parameter list (§15.1.3 IsSimpleParameterList) is one consisting
/// solely of single-name (identifier) bindings — no rest, defaults, or
/// destructuring. This gates the fast parameter-binding path and mapped
/// `arguments` objects, so it is cached on `JsFunction::User` at creation time.
pub fn params_are_simple(params: &[Pattern]) -> bool {
    params.iter().all(|p| matches!(p, Pattern::Identifier(_)))
}

fn params_use_arguments(params: &[Pattern]) -> bool {
    params.iter().any(pattern_uses_arguments)
}

/// Check if a function body references the `arguments` identifier.
/// Recurses into arrow functions (they inherit arguments) but not into
/// regular functions, generators, or class methods (they have their own).
pub fn stmts_use_arguments(stmts: &[Statement]) -> bool {
    stmts.iter().any(stmt_uses_arguments)
}

fn stmt_uses_arguments(stmt: &Statement) -> bool {
    match stmt {
        Statement::Expression(e) => expr_uses_arguments(e),
        Statement::Block(stmts) => stmts.iter().any(stmt_uses_arguments),
        Statement::Variable(decl) => decl.declarations.iter().any(|d| {
            pattern_uses_arguments(&d.pattern) || d.init.as_ref().is_some_and(expr_uses_arguments)
        }),
        Statement::If(i) => {
            expr_uses_arguments(&i.test)
                || stmt_uses_arguments(&i.consequent)
                || i.alternate.as_ref().is_some_and(|s| stmt_uses_arguments(s))
        }
        Statement::While(w) => expr_uses_arguments(&w.test) || stmt_uses_arguments(&w.body),
        Statement::DoWhile(d) => stmt_uses_arguments(&d.body) || expr_uses_arguments(&d.test),
        Statement::For(f) => {
            f.init.as_ref().is_some_and(|i| match i {
                ForInit::Expression(e) => expr_uses_arguments(e),
                ForInit::Variable(d) => d.declarations.iter().any(|d| {
                    pattern_uses_arguments(&d.pattern)
                        || d.init.as_ref().is_some_and(expr_uses_arguments)
                }),
            }) || f.test.as_ref().is_some_and(expr_uses_arguments)
                || f.update.as_ref().is_some_and(expr_uses_arguments)
                || stmt_uses_arguments(&f.body)
        }
        Statement::ForIn(f) => {
            for_in_of_left_uses_arguments(&f.left)
                || expr_uses_arguments(&f.right)
                || stmt_uses_arguments(&f.body)
        }
        Statement::ForOf(f) => {
            for_in_of_left_uses_arguments(&f.left)
                || expr_uses_arguments(&f.right)
                || stmt_uses_arguments(&f.body)
        }
        Statement::Return(e) => e.as_ref().is_some_and(expr_uses_arguments),
        Statement::Throw(e) => expr_uses_arguments(e),
        Statement::Try(t) => {
            stmts_use_arguments(&t.block)
                || t.handler.as_ref().is_some_and(|h| {
                    h.param.as_ref().is_some_and(pattern_uses_arguments)
                        || stmts_use_arguments(&h.body)
                })
                || t.finalizer.as_ref().is_some_and(|f| stmts_use_arguments(f))
        }
        Statement::Switch(s) => {
            expr_uses_arguments(&s.discriminant)
                || s.cases.iter().any(|c| {
                    c.test.as_ref().is_some_and(expr_uses_arguments)
                        || stmts_use_arguments(&c.consequent)
                })
        }
        Statement::Labeled(_, s) => stmt_uses_arguments(s),
        Statement::With(e, s) => expr_uses_arguments(e) || stmt_uses_arguments(s),
        // Nested function declarations have their own `arguments`; classes have
        // their own scope for method bodies, but `extends` and computed element
        // keys evaluate in the enclosing scope and may reference `arguments`.
        Statement::FunctionDeclaration(_) => false,
        Statement::ClassDeclaration(c) => {
            class_extends_or_computed_keys_use_arguments(c.super_class.as_deref(), &c.body)
        }
        Statement::Empty | Statement::Break(_) | Statement::Continue(_) | Statement::Debugger => {
            false
        }
    }
}

fn expr_uses_arguments(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(name) => name == "arguments",
        Expression::Literal(_)
        | Expression::This
        | Expression::Super
        | Expression::ImportMeta
        | Expression::NewTarget
        | Expression::PrivateIdentifier(_) => false,
        // Regular functions have their own `arguments`. Classes have their own
        // scope for method bodies, but `extends` and computed class element keys
        // evaluate in the enclosing scope and may reference `arguments`.
        Expression::Function(_) => false,
        Expression::Class(c) => {
            class_extends_or_computed_keys_use_arguments(c.super_class.as_deref(), &c.body)
        }
        // DO recurse into arrow functions (they inherit arguments)
        Expression::ArrowFunction(a) => {
            let body = a.body.body();
            match body.statements.as_slice() {
                [Statement::Return(Some(e))] => expr_uses_arguments(e),
                stmts => stmts_use_arguments(stmts),
            }
        }
        Expression::Array(elems, _) => elems
            .iter()
            .any(|e| e.as_ref().is_some_and(expr_uses_arguments)),
        Expression::Object(props) => props.iter().any(|p| {
            expr_uses_arguments(&p.value)
                || matches!(&p.key, PropertyKey::Computed(e) if expr_uses_arguments(e))
        }),
        Expression::Unary(_, e)
        | Expression::Update(_, _, e)
        | Expression::Spread(e)
        | Expression::Yield(Some(e), _)
        | Expression::Await(e)
        | Expression::Typeof(e)
        | Expression::Void(e)
        | Expression::Delete(e) => expr_uses_arguments(e),
        Expression::Yield(None, _) => false,
        Expression::Binary(_, l, r)
        | Expression::Logical(_, l, r)
        | Expression::Assign(_, l, r) => expr_uses_arguments(l) || expr_uses_arguments(r),
        Expression::Conditional(t, c, a) => {
            expr_uses_arguments(t) || expr_uses_arguments(c) || expr_uses_arguments(a)
        }
        Expression::Call(callee, args, _) => {
            matches!(&**callee, Expression::Identifier(name) if name == "eval")
                || expr_uses_arguments(callee)
                || args.iter().any(expr_uses_arguments)
        }
        Expression::New(callee, args, _) => {
            expr_uses_arguments(callee) || args.iter().any(expr_uses_arguments)
        }
        Expression::Member(obj, prop, _) => {
            expr_uses_arguments(obj)
                || matches!(prop, MemberProperty::Computed(e) if expr_uses_arguments(e))
        }
        Expression::OptionalChain(base, chain) => {
            expr_uses_arguments(base) || expr_uses_arguments(chain)
        }
        Expression::Comma(exprs) | Expression::Sequence(exprs) => {
            exprs.iter().any(expr_uses_arguments)
        }
        Expression::TaggedTemplate(tag, tpl) => {
            expr_uses_arguments(tag) || tpl.expressions.iter().any(expr_uses_arguments)
        }
        Expression::Template(tpl) => tpl.expressions.iter().any(expr_uses_arguments),
        Expression::Import(spec, opts)
        | Expression::ImportDefer(spec, opts)
        | Expression::ImportSource(spec, opts) => {
            expr_uses_arguments(spec) || opts.as_ref().is_some_and(|e| expr_uses_arguments(e))
        }
    }
}

fn pattern_uses_arguments(pat: &Pattern) -> bool {
    match pat {
        Pattern::Identifier(name) => name == "arguments",
        Pattern::Array(elems) => elems.iter().any(|e| {
            e.as_ref().is_some_and(|e| match e {
                ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                    pattern_uses_arguments(p)
                }
            })
        }),
        Pattern::Object(props) => props.iter().any(|p| match p {
            ObjectPatternProperty::KeyValue(key, pat) => {
                matches!(key, PropertyKey::Computed(e) if expr_uses_arguments(e))
                    || pattern_uses_arguments(pat)
            }
            ObjectPatternProperty::Rest(pat) => pattern_uses_arguments(pat),
            ObjectPatternProperty::Shorthand(name) => name == "arguments",
        }),
        Pattern::Assign(pat, expr) => pattern_uses_arguments(pat) || expr_uses_arguments(expr),
        Pattern::Rest(pat) => pattern_uses_arguments(pat),
        Pattern::MemberExpression(e) => expr_uses_arguments(e),
    }
}

fn for_in_of_left_uses_arguments(left: &ForInOfLeft) -> bool {
    match left {
        ForInOfLeft::Variable(d) => d.declarations.iter().any(|d| {
            pattern_uses_arguments(&d.pattern) || d.init.as_ref().is_some_and(expr_uses_arguments)
        }),
        ForInOfLeft::Pattern(p) => pattern_uses_arguments(p),
        ForInOfLeft::Expression(e) => expr_uses_arguments(e),
    }
}

fn assign_stmt_sites(stmt: &mut Statement, call_id: &mut u32, prop_id: &mut u32) {
    match stmt {
        Statement::Expression(e) => assign_expr_sites(e, call_id, prop_id),
        Statement::Block(stmts) => {
            for s in stmts.iter_mut() {
                assign_stmt_sites(s, call_id, prop_id);
            }
        }
        Statement::Variable(decl) => {
            for d in decl.declarations.iter_mut() {
                assign_pattern_sites(&mut d.pattern, call_id, prop_id);
                if let Some(init) = d.init.as_mut() {
                    assign_expr_sites(init, call_id, prop_id);
                }
            }
        }
        Statement::If(i) => {
            assign_expr_sites(&mut i.test, call_id, prop_id);
            assign_stmt_sites(&mut i.consequent, call_id, prop_id);
            if let Some(alt) = i.alternate.as_mut() {
                assign_stmt_sites(alt, call_id, prop_id);
            }
        }
        Statement::While(w) => {
            assign_expr_sites(&mut w.test, call_id, prop_id);
            assign_stmt_sites(&mut w.body, call_id, prop_id);
        }
        Statement::DoWhile(d) => {
            assign_stmt_sites(&mut d.body, call_id, prop_id);
            assign_expr_sites(&mut d.test, call_id, prop_id);
        }
        Statement::For(f) => {
            if let Some(init) = f.init.as_mut() {
                match init {
                    ForInit::Expression(e) => assign_expr_sites(e, call_id, prop_id),
                    ForInit::Variable(decl) => {
                        for d in decl.declarations.iter_mut() {
                            assign_pattern_sites(&mut d.pattern, call_id, prop_id);
                            if let Some(init) = d.init.as_mut() {
                                assign_expr_sites(init, call_id, prop_id);
                            }
                        }
                    }
                }
            }
            if let Some(test) = f.test.as_mut() {
                assign_expr_sites(test, call_id, prop_id);
            }
            if let Some(update) = f.update.as_mut() {
                assign_expr_sites(update, call_id, prop_id);
            }
            assign_stmt_sites(&mut f.body, call_id, prop_id);
        }
        Statement::ForIn(f) => {
            match &mut f.left {
                ForInOfLeft::Variable(decl) => {
                    for d in decl.declarations.iter_mut() {
                        assign_pattern_sites(&mut d.pattern, call_id, prop_id);
                        if let Some(init) = d.init.as_mut() {
                            assign_expr_sites(init, call_id, prop_id);
                        }
                    }
                }
                ForInOfLeft::Pattern(p) => assign_pattern_sites(p, call_id, prop_id),
                ForInOfLeft::Expression(e) => assign_expr_sites(e, call_id, prop_id),
            }
            assign_expr_sites(&mut f.right, call_id, prop_id);
            assign_stmt_sites(&mut f.body, call_id, prop_id);
        }
        Statement::ForOf(f) => {
            match &mut f.left {
                ForInOfLeft::Variable(decl) => {
                    for d in decl.declarations.iter_mut() {
                        assign_pattern_sites(&mut d.pattern, call_id, prop_id);
                        if let Some(init) = d.init.as_mut() {
                            assign_expr_sites(init, call_id, prop_id);
                        }
                    }
                }
                ForInOfLeft::Pattern(p) => assign_pattern_sites(p, call_id, prop_id),
                ForInOfLeft::Expression(e) => assign_expr_sites(e, call_id, prop_id),
            }
            assign_expr_sites(&mut f.right, call_id, prop_id);
            assign_stmt_sites(&mut f.body, call_id, prop_id);
        }
        Statement::Return(e) => {
            if let Some(e) = e.as_mut() {
                assign_expr_sites(e, call_id, prop_id);
            }
        }
        Statement::Throw(e) => assign_expr_sites(e, call_id, prop_id),
        Statement::Try(t) => {
            for s in t.block.iter_mut() {
                assign_stmt_sites(s, call_id, prop_id);
            }
            if let Some(h) = t.handler.as_mut() {
                if let Some(param) = h.param.as_mut() {
                    assign_pattern_sites(param, call_id, prop_id);
                }
                for s in h.body.iter_mut() {
                    assign_stmt_sites(s, call_id, prop_id);
                }
            }
            if let Some(f) = t.finalizer.as_mut() {
                for s in f.iter_mut() {
                    assign_stmt_sites(s, call_id, prop_id);
                }
            }
        }
        Statement::Switch(s) => {
            assign_expr_sites(&mut s.discriminant, call_id, prop_id);
            for c in s.cases.iter_mut() {
                if let Some(test) = c.test.as_mut() {
                    assign_expr_sites(test, call_id, prop_id);
                }
                for stmt in c.consequent.iter_mut() {
                    assign_stmt_sites(stmt, call_id, prop_id);
                }
            }
        }
        Statement::Labeled(_, s) => assign_stmt_sites(s, call_id, prop_id),
        Statement::With(e, s) => {
            assign_expr_sites(e, call_id, prop_id);
            assign_stmt_sites(s, call_id, prop_id);
        }
        Statement::FunctionDeclaration(f) => {
            assign_ic_sites(&mut f.body);
        }
        Statement::ClassDeclaration(c) => assign_class_sites(c, call_id, prop_id),
        Statement::Empty | Statement::Break(_) | Statement::Continue(_) | Statement::Debugger => {}
    }
}

fn assign_class_sites(c: &mut ClassDecl, call_id: &mut u32, prop_id: &mut u32) {
    if let Some(super_class) = c.super_class.as_mut() {
        assign_expr_sites(super_class, call_id, prop_id);
    }
    for el in c.body.iter_mut() {
        match el {
            ClassElement::Method(m) => assign_class_method_sites(m, call_id, prop_id),
            ClassElement::Property(p) | ClassElement::AutoAccessor(p) => {
                // Computed keys are evaluated once, at class-definition time,
                // under the surrounding body's IC handle — number them here.
                if let PropertyKey::Computed(e) = &mut p.key {
                    assign_expr_sites(e, call_id, prop_id);
                }
                // Static field initializers also run at class-definition time,
                // so their sites belong to this body. Instance field and
                // instance auto-accessor initializers run later, during
                // construction, under whatever body invokes the constructor —
                // numbering their sites here would make them index that body's
                // (possibly smaller) IC store and panic. Leave them UNASSIGNED
                // so they take the IC slow path wherever they execute.
                if p.is_static
                    && let Some(v) = p.value.as_mut()
                {
                    assign_expr_sites(v, call_id, prop_id);
                }
            }
            ClassElement::StaticBlock(stmts) => {
                for s in stmts.iter_mut() {
                    assign_stmt_sites(s, call_id, prop_id);
                }
            }
        }
    }
}

fn assign_class_method_sites(m: &mut ClassMethod, call_id: &mut u32, prop_id: &mut u32) {
    if let PropertyKey::Computed(e) = &mut m.key {
        assign_expr_sites(e, call_id, prop_id);
    }
    assign_ic_sites(&mut m.value.body);
}

fn assign_pattern_sites(pat: &mut Pattern, call_id: &mut u32, prop_id: &mut u32) {
    match pat {
        Pattern::Identifier(_) => {}
        Pattern::Array(elems) => {
            for e in elems.iter_mut().flatten() {
                match e {
                    ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                        assign_pattern_sites(p, call_id, prop_id)
                    }
                }
            }
        }
        Pattern::Object(props) => {
            for p in props.iter_mut() {
                match p {
                    ObjectPatternProperty::KeyValue(key, pat) => {
                        if let PropertyKey::Computed(e) = key {
                            assign_expr_sites(e, call_id, prop_id);
                        }
                        assign_pattern_sites(pat, call_id, prop_id);
                    }
                    ObjectPatternProperty::Shorthand(_) => {}
                    ObjectPatternProperty::Rest(pat) => assign_pattern_sites(pat, call_id, prop_id),
                }
            }
        }
        Pattern::Assign(pat, expr) => {
            assign_pattern_sites(pat, call_id, prop_id);
            assign_expr_sites(expr, call_id, prop_id);
        }
        Pattern::Rest(pat) => assign_pattern_sites(pat, call_id, prop_id),
        Pattern::MemberExpression(e) => {
            // A member pattern is an assignment target, not a property access,
            // so the top-level Member does not get an IC site. Sub-expressions
            // (computed key, base object) are still traversed.
            match &mut **e {
                Expression::Member(obj, prop, _) => {
                    assign_expr_sites(obj, call_id, prop_id);
                    if let MemberProperty::Computed(e) = prop {
                        assign_expr_sites(e, call_id, prop_id);
                    }
                }
                other => assign_expr_sites(other, call_id, prop_id),
            }
        }
    }
}

fn assign_expr_sites(expr: &mut Expression, call_id: &mut u32, prop_id: &mut u32) {
    match expr {
        Expression::Call(callee, args, site) => {
            assign_expr_sites(callee, call_id, prop_id);
            for a in args.iter_mut() {
                assign_expr_sites(a, call_id, prop_id);
            }
            *site = CallSiteId(*call_id);
            *call_id += 1;
        }
        Expression::New(callee, args, site) => {
            assign_expr_sites(callee, call_id, prop_id);
            for a in args.iter_mut() {
                assign_expr_sites(a, call_id, prop_id);
            }
            *site = CallSiteId(*call_id);
            *call_id += 1;
        }
        Expression::Member(obj, prop, site) => {
            assign_expr_sites(obj, call_id, prop_id);
            if let MemberProperty::Computed(e) = prop {
                assign_expr_sites(e, call_id, prop_id);
            }
            *site = PropSiteId(*prop_id);
            *prop_id += 1;
        }
        Expression::OptionalChain(base, chain) => {
            assign_expr_sites(base, call_id, prop_id);
            assign_expr_sites(chain, call_id, prop_id);
        }
        Expression::Unary(_, e)
        | Expression::Update(_, _, e)
        | Expression::Spread(e)
        | Expression::Yield(Some(e), _)
        | Expression::Await(e)
        | Expression::Typeof(e)
        | Expression::Void(e)
        | Expression::Delete(e) => assign_expr_sites(e, call_id, prop_id),
        Expression::Yield(None, _) => {}
        Expression::Binary(_, l, r)
        | Expression::Logical(_, l, r)
        | Expression::Assign(_, l, r) => {
            assign_expr_sites(l, call_id, prop_id);
            assign_expr_sites(r, call_id, prop_id);
        }
        Expression::Conditional(t, c, a) => {
            assign_expr_sites(t, call_id, prop_id);
            assign_expr_sites(c, call_id, prop_id);
            assign_expr_sites(a, call_id, prop_id);
        }
        Expression::Array(elems, _) => {
            for e in elems.iter_mut().flatten() {
                assign_expr_sites(e, call_id, prop_id);
            }
        }
        Expression::Object(props) => {
            for p in props.iter_mut() {
                if let PropertyKey::Computed(e) = &mut p.key {
                    assign_expr_sites(e, call_id, prop_id);
                }
                assign_expr_sites(&mut p.value, call_id, prop_id);
            }
        }
        Expression::Function(f) => {
            assign_ic_sites(&mut f.body);
        }
        Expression::ArrowFunction(a) => {
            assign_ic_sites(a.body.body_mut());
        }
        Expression::Class(c) => {
            if let Some(super_class) = c.super_class.as_mut() {
                assign_expr_sites(super_class, call_id, prop_id);
            }
            for el in c.body.iter_mut() {
                match el {
                    ClassElement::Method(m) => assign_class_method_sites(m, call_id, prop_id),
                    ClassElement::Property(p) | ClassElement::AutoAccessor(p) => {
                        // See assign_class_sites: number computed keys and static
                        // initializers (evaluated at class-definition time), but
                        // leave instance field / auto-accessor initializers
                        // UNASSIGNED so they take the IC slow path when they run
                        // during construction under the constructor's handle.
                        if let PropertyKey::Computed(e) = &mut p.key {
                            assign_expr_sites(e, call_id, prop_id);
                        }
                        if p.is_static
                            && let Some(v) = p.value.as_mut()
                        {
                            assign_expr_sites(v, call_id, prop_id);
                        }
                    }
                    ClassElement::StaticBlock(stmts) => {
                        for s in stmts.iter_mut() {
                            assign_stmt_sites(s, call_id, prop_id);
                        }
                    }
                }
            }
        }
        Expression::TaggedTemplate(tag, tpl) => {
            assign_expr_sites(tag, call_id, prop_id);
            for e in tpl.expressions.iter_mut() {
                assign_expr_sites(e, call_id, prop_id);
            }
        }
        Expression::Template(tpl) => {
            for e in tpl.expressions.iter_mut() {
                assign_expr_sites(e, call_id, prop_id);
            }
        }
        Expression::Comma(exprs) | Expression::Sequence(exprs) => {
            for e in exprs.iter_mut() {
                assign_expr_sites(e, call_id, prop_id);
            }
        }
        Expression::Import(spec, opts)
        | Expression::ImportDefer(spec, opts)
        | Expression::ImportSource(spec, opts) => {
            assign_expr_sites(spec, call_id, prop_id);
            if let Some(opts) = opts.as_mut() {
                assign_expr_sites(opts, call_id, prop_id);
            }
        }
        Expression::Literal(_)
        | Expression::Identifier(_)
        | Expression::This
        | Expression::Super
        | Expression::ImportMeta
        | Expression::NewTarget
        | Expression::PrivateIdentifier(_) => {}
    }
}

/// Reset the inline-cache site ids reachable from `expr` (without descending
/// into nested function/arrow bodies) to `UNASSIGNED`, forcing the IC slow path.
///
/// Used for generator/async state-machine *terminator* expressions — yield /
/// await / return / throw values, `ConditionalGoto` and `SwitchDispatch`
/// conditions, and `ForOfInit` iterables. The state driver evaluates these after
/// `exec_body` has restored the previous `current_ic_handle`, i.e. under whatever
/// body is driving the generator (often the caller), not under any state body's
/// store. A numbered site would then index the wrong store and panic, so these
/// expressions must stay unnumbered. Nested function/arrow bodies are left intact
/// because they execute under their own stores; a class literal's `extends`,
/// computed keys, and static-field initializers are cleared because they evaluate
/// at class-definition time (i.e. when the terminator expression runs).
pub fn clear_expr_ic_sites(expr: &mut Expression) {
    match expr {
        Expression::Call(callee, args, site) => {
            clear_expr_ic_sites(callee);
            for a in args.iter_mut() {
                clear_expr_ic_sites(a);
            }
            *site = CallSiteId::UNASSIGNED;
        }
        Expression::New(callee, args, site) => {
            clear_expr_ic_sites(callee);
            for a in args.iter_mut() {
                clear_expr_ic_sites(a);
            }
            *site = CallSiteId::UNASSIGNED;
        }
        Expression::Member(obj, prop, site) => {
            clear_expr_ic_sites(obj);
            if let MemberProperty::Computed(e) = prop {
                clear_expr_ic_sites(e);
            }
            *site = PropSiteId::UNASSIGNED;
        }
        Expression::OptionalChain(base, chain) => {
            clear_expr_ic_sites(base);
            clear_expr_ic_sites(chain);
        }
        Expression::Unary(_, e)
        | Expression::Update(_, _, e)
        | Expression::Spread(e)
        | Expression::Yield(Some(e), _)
        | Expression::Await(e)
        | Expression::Typeof(e)
        | Expression::Void(e)
        | Expression::Delete(e) => clear_expr_ic_sites(e),
        Expression::Yield(None, _) => {}
        Expression::Binary(_, l, r)
        | Expression::Logical(_, l, r)
        | Expression::Assign(_, l, r) => {
            clear_expr_ic_sites(l);
            clear_expr_ic_sites(r);
        }
        Expression::Conditional(t, c, a) => {
            clear_expr_ic_sites(t);
            clear_expr_ic_sites(c);
            clear_expr_ic_sites(a);
        }
        Expression::Array(elems, _) => {
            for e in elems.iter_mut().flatten() {
                clear_expr_ic_sites(e);
            }
        }
        Expression::Object(props) => {
            for p in props.iter_mut() {
                if let PropertyKey::Computed(e) = &mut p.key {
                    clear_expr_ic_sites(e);
                }
                clear_expr_ic_sites(&mut p.value);
            }
        }
        // Nested function/arrow bodies run under their own IC store — leave them.
        Expression::Function(_) | Expression::ArrowFunction(_) => {}
        Expression::Class(c) => {
            if let Some(sc) = c.super_class.as_mut() {
                clear_expr_ic_sites(sc);
            }
            for el in c.body.iter_mut() {
                match el {
                    ClassElement::Property(p) | ClassElement::AutoAccessor(p) => {
                        if let PropertyKey::Computed(e) = &mut p.key {
                            clear_expr_ic_sites(e);
                        }
                        if p.is_static
                            && let Some(v) = p.value.as_mut()
                        {
                            clear_expr_ic_sites(v);
                        }
                    }
                    ClassElement::Method(m) => {
                        if let PropertyKey::Computed(e) = &mut m.key {
                            clear_expr_ic_sites(e);
                        }
                    }
                    ClassElement::StaticBlock(_) => {}
                }
            }
        }
        Expression::TaggedTemplate(tag, tpl) => {
            clear_expr_ic_sites(tag);
            for e in tpl.expressions.iter_mut() {
                clear_expr_ic_sites(e);
            }
        }
        Expression::Template(tpl) => {
            for e in tpl.expressions.iter_mut() {
                clear_expr_ic_sites(e);
            }
        }
        Expression::Comma(exprs) | Expression::Sequence(exprs) => {
            for e in exprs.iter_mut() {
                clear_expr_ic_sites(e);
            }
        }
        Expression::Import(spec, opts)
        | Expression::ImportDefer(spec, opts)
        | Expression::ImportSource(spec, opts) => {
            clear_expr_ic_sites(spec);
            if let Some(opts) = opts.as_mut() {
                clear_expr_ic_sites(opts);
            }
        }
        Expression::Literal(_)
        | Expression::Identifier(_)
        | Expression::This
        | Expression::Super
        | Expression::ImportMeta
        | Expression::NewTarget
        | Expression::PrivateIdentifier(_) => {}
    }
}

fn class_extends_or_computed_keys_use_arguments(
    super_class: Option<&Expression>,
    body: &[ClassElement],
) -> bool {
    if super_class.is_some_and(expr_uses_arguments) {
        return true;
    }
    body.iter().any(|el| match el {
        ClassElement::Method(m) => {
            matches!(&m.key, PropertyKey::Computed(e) if expr_uses_arguments(e))
        }
        ClassElement::Property(p) | ClassElement::AutoAccessor(p) => {
            matches!(&p.key, PropertyKey::Computed(e) if expr_uses_arguments(e))
        }
        // Static blocks have their own scope per spec §15.7.13 — do not recurse.
        ClassElement::StaticBlock(_) => false,
    })
}

#[cfg(test)]
mod ic_site_tests {
    use super::*;

    fn ident(name: &str) -> Expression {
        Expression::Identifier(name.to_string())
    }

    fn call(callee: Expression, args: Vec<Expression>) -> Expression {
        Expression::Call(Box::new(callee), args, CallSiteId::UNASSIGNED)
    }

    fn prop(obj: Expression, name: &str) -> Expression {
        Expression::Member(
            Box::new(obj),
            MemberProperty::Dot(name.to_string()),
            PropSiteId::UNASSIGNED,
        )
    }

    fn expr_stmt(e: Expression) -> Statement {
        Statement::Expression(e)
    }

    fn body_with(stmts: Vec<Statement>) -> Body {
        Body::new(stmts)
    }

    #[test]
    fn single_call_site_gets_id_zero() {
        let mut body = body_with(vec![expr_stmt(call(ident("f"), vec![]))]);
        assign_ic_sites(&mut body);
        assert!(body.ic.assigned);
        assert_eq!(body.ic.call_site_count, 1);
        assert_eq!(body.ic.prop_site_count, 0);
        match &body.statements[0] {
            Statement::Expression(Expression::Call(_, _, id)) => assert_eq!(id.0, 0),
            _ => panic!("expected Call"),
        }
    }

    #[test]
    fn mixed_call_and_prop_sites_numbered_separately() {
        let mut body = body_with(vec![expr_stmt(call(
            prop(ident("o"), "m"),
            vec![prop(ident("o"), "x")],
        ))]);
        assign_ic_sites(&mut body);
        assert_eq!(body.ic.call_site_count, 1);
        assert_eq!(body.ic.prop_site_count, 2);
        match &body.statements[0] {
            Statement::Expression(Expression::Call(callee, _, call_id)) => {
                assert_eq!(call_id.0, 0);
                match callee.as_ref() {
                    Expression::Member(_, _, id) => assert_eq!(id.0, 0),
                    _ => panic!("expected Member callee"),
                }
            }
            _ => panic!("expected Call"),
        }
    }

    #[test]
    fn nested_function_body_resets_counters() {
        let inner_body = body_with(vec![expr_stmt(call(ident("g"), vec![]))]);
        let func = FunctionExpr {
            name: None,
            params: vec![],
            body: inner_body,
            is_async: false,
            is_generator: false,
            source_text: None,
            body_is_strict: false,
        };
        let mut outer = body_with(vec![expr_stmt(call(
            Expression::Function(func),
            vec![prop(ident("o"), "x")],
        ))]);
        assign_ic_sites(&mut outer);

        assert_eq!(outer.ic.call_site_count, 1);
        assert_eq!(outer.ic.prop_site_count, 1);

        match &outer.statements[0] {
            Statement::Expression(Expression::Call(_, _, id)) => assert_eq!(id.0, 0),
            _ => panic!("expected outer Call"),
        }

        let inner_func = match &outer.statements[0] {
            Statement::Expression(Expression::Call(callee, _, _)) => match callee.as_ref() {
                Expression::Function(f) => f,
                _ => panic!("expected Function"),
            },
            _ => panic!("expected Call"),
        };
        assert_eq!(inner_func.body.ic.call_site_count, 1);
        assert_eq!(inner_func.body.ic.prop_site_count, 0);
        match &inner_func.body.statements[0] {
            Statement::Expression(Expression::Call(_, _, id)) => assert_eq!(id.0, 0),
            _ => panic!("expected inner Call"),
        }
    }

    #[test]
    fn assign_ic_sites_for_body_counts_only_inner_body() {
        let mut body = body_with(vec![expr_stmt(call(ident("f"), vec![]))]);
        assign_ic_sites_for_body(&mut body);
        assert_eq!(body.ic.call_site_count, 1);
        assert_eq!(body.ic.prop_site_count, 0);
    }

    #[test]
    fn assign_ic_sites_for_module_numbers_top_level_items() {
        let mut program = Program {
            source_type: SourceType::Module,
            body: Body::new(vec![]),
            module_items: vec![
                ModuleItem::ExportDeclaration(ExportDeclaration::Default(Box::new(call(
                    ident("f"),
                    vec![prop(ident("o"), "x")],
                )))),
                ModuleItem::Statement(expr_stmt(call(ident("g"), vec![]))),
            ],
            body_is_strict: true,
        };
        assign_ic_sites_for_module(&mut program);
        assert!(program.body.ic.assigned);
        assert_eq!(program.body.ic.call_site_count, 2);
        assert_eq!(program.body.ic.prop_site_count, 1);
    }
}
