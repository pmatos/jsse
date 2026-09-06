/// AST node types for ECMAScript.
/// Each node represents a syntactic element from the spec.
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMPLATE_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_template_id() -> u64 {
    NEXT_TEMPLATE_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Debug)]
pub(crate) struct SourceText {
    source: Rc<str>,
    start: usize,
    end: usize,
}

impl SourceText {
    pub(crate) fn new(source: Rc<str>, start: usize, end: usize) -> Self {
        Self { source, start, end }
    }

    pub(crate) fn as_str(&self) -> &str {
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
pub(crate) enum SourceType {
    Script,
    Module,
}

/// Dense identifier for a call IC site within a single `Body`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub(crate) struct CallSiteId(pub u32);

/// Dense identifier for a property-access IC site within a single `Body`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub(crate) struct PropSiteId(pub u32);

impl CallSiteId {
    pub(crate) const UNASSIGNED: Self = Self(u32::MAX);
}

impl PropSiteId {
    pub(crate) const UNASSIGNED: Self = Self(u32::MAX);
}

/// Metadata describing the number of IC sites in a `Body`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct BodyIcInfo {
    pub call_site_count: u32,
    pub prop_site_count: u32,
    pub assigned: bool,
}

/// A unit of executable ECMAScript syntax: a script, module, or function body.
/// Carries the statement vector and IC metadata; the runtime cache lives in the
/// interpreter, keyed by the body's identity.
#[derive(Clone, Debug)]
pub(crate) struct Body {
    pub statements: Rc<Vec<Statement>>,
    pub ic: BodyIcInfo,
}

impl Body {
    pub(crate) fn new(statements: Vec<Statement>) -> Self {
        Self {
            statements: Rc::new(statements),
            ic: BodyIcInfo::default(),
        }
    }

    pub(crate) fn as_slice(&self) -> &[Statement] {
        &self.statements
    }

    /// Identity of this Body, for side tables keyed by Body (`interpreter::ic_store`,
    /// `interpreter::hoist_cache`). Stable across `Body` clones, since they share
    /// the statement `Rc`. The pointer is an identity only and is never
    /// dereferenced; a table keyed by it must pin the `Rc` for as long as the key
    /// lives, so a freed Body's address cannot be reused for an unrelated entry
    /// (ABA).
    pub(crate) fn key(&self) -> *const Vec<Statement> {
        Rc::as_ptr(&self.statements)
    }
}

/// Assign dense `CallSiteId` and `PropSiteId` values to every call, new, and
/// member site in `body`, and record the final counts in `body.ic`.
/// This is a single shared pass used by the parser, generator transform,
/// `eval`, and `new Function`.
pub(crate) fn assign_ic_sites(body: &mut Body) {
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
pub(crate) fn assign_ic_sites_for_body(body: &mut Body) -> (u32, u32) {
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
pub(crate) fn assign_ic_sites_for_module(program: &mut Program) {
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
pub(crate) struct Program {
    pub source_type: SourceType,
    pub body: Body,
    pub module_items: Vec<ModuleItem>,
    pub body_is_strict: bool,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum ModuleItem {
    Statement(Statement),
    ImportDeclaration(ImportDeclaration),
    ExportDeclaration(ExportDeclaration),
}

#[derive(Clone, Debug)]
pub(crate) struct ImportDeclaration {
    pub specifiers: Vec<ImportSpecifier>,
    pub source: String,
    pub attributes: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub(crate) enum ImportSpecifier {
    Named { imported: String, local: String },
    Default(String),
    Namespace(String),
    DeferredNamespace(String),
    SourcePhase(String),
}

#[derive(Clone, Debug)]
pub(crate) enum ExportDeclaration {
    Named {
        specifiers: Vec<ExportSpecifier>,
        source: Option<String>,
        attributes: Vec<(String, String)>,
        declaration: Option<Box<Statement>>,
    },
    Default(Box<Expression>),
    DefaultFunction(FunctionDecl),
    DefaultClass(ClassDecl),
    All {
        exported: Option<String>,
        source: String,
        attributes: Vec<(String, String)>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct ExportSpecifier {
    pub local: String,
    pub exported: String,
}

#[derive(Clone, Debug)]
pub(crate) enum Statement {
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
pub(crate) struct VariableDeclaration {
    pub kind: VarKind,
    pub declarations: Vec<VariableDeclarator>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VarKind {
    Var,
    Let,
    Const,
    Using,
    AwaitUsing,
}

#[derive(Clone, Debug)]
pub(crate) struct VariableDeclarator {
    pub pattern: Pattern,
    pub init: Option<Expression>,
}

#[derive(Clone, Debug)]
pub(crate) enum Pattern {
    Identifier(String),
    Array(Vec<Option<ArrayPatternElement>>),
    Object(Vec<ObjectPatternProperty>),
    Assign(Box<Pattern>, Box<Expression>),
    Rest(Box<Pattern>),
    MemberExpression(Box<Expression>),
}

#[derive(Clone, Debug)]
pub(crate) enum ArrayPatternElement {
    Pattern(Pattern),
    Rest(Pattern),
}

#[derive(Clone, Debug)]
pub(crate) enum ObjectPatternProperty {
    KeyValue(PropertyKey, Pattern),
    Shorthand(String),
    Rest(Pattern),
}

impl Pattern {
    /// Static Semantics: BoundNames — append the identifier names this binding
    /// pattern introduces, in source order, to `out`.
    ///
    /// Property keys and default-value expressions contribute no names; a
    /// `MemberExpression` target (only reachable in destructuring assignment)
    /// binds nothing. Holes in an array pattern are skipped.
    pub(crate) fn bound_names(&self, out: &mut Vec<String>) {
        match self {
            Pattern::Identifier(name) => out.push(name.clone()),
            Pattern::Array(elems) => {
                for elem in elems.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                            p.bound_names(out);
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                            p.bound_names(out);
                        }
                        ObjectPatternProperty::Shorthand(name) => out.push(name.clone()),
                    }
                }
            }
            Pattern::Assign(inner, _) | Pattern::Rest(inner) => inner.bound_names(out),
            Pattern::MemberExpression(_) => {}
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Expression {
    Literal(Literal),
    Identifier(String),
    This,
    Super,
    Array(Vec<Option<Expression>>, bool),
    Object(Vec<Property>, bool),
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
pub(crate) enum MemberProperty {
    Dot(String),
    Computed(Box<Expression>),
    Private(String),
}

#[derive(Clone, Debug)]
pub(crate) enum Literal {
    Null,
    Boolean(bool),
    Number(f64),
    String(Vec<u16>),
    BigInt(String),
    RegExp(String, String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnaryOp {
    Minus,
    Plus,
    Not,
    BitNot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BinaryOp {
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
pub(crate) enum LogicalOp {
    And,
    Or,
    NullishCoalescing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateOp {
    Increment,
    Decrement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssignOp {
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
pub(crate) struct Property {
    pub key: PropertyKey,
    pub value: Expression,
    pub kind: PropertyKind,
    pub computed: bool,
    pub shorthand: bool,
    pub method: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum PropertyKey {
    Identifier(String),
    String(Vec<u16>),
    Number(f64),
    Computed(Box<Expression>),
    Private(String),
}

impl PropertyKey {
    pub(crate) fn matches_name(&self, name: &str) -> bool {
        match self {
            Self::Identifier(identifier) => identifier == name,
            Self::String(units) => units.iter().copied().eq(name.encode_utf16()),
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PropertyKind {
    Init,
    Get,
    Set,
}

#[derive(Clone, Debug)]
pub(crate) struct IfStatement {
    pub test: Expression,
    pub consequent: Box<Statement>,
    pub alternate: Option<Box<Statement>>,
}

#[derive(Clone, Debug)]
pub(crate) struct WhileStatement {
    pub test: Expression,
    pub body: Box<Statement>,
}

#[derive(Clone, Debug)]
pub(crate) struct DoWhileStatement {
    pub test: Expression,
    pub body: Box<Statement>,
}

#[derive(Clone, Debug)]
pub(crate) struct ForStatement {
    pub init: Option<ForInit>,
    pub test: Option<Expression>,
    pub update: Option<Expression>,
    pub body: Box<Statement>,
}

#[derive(Clone, Debug)]
pub(crate) enum ForInit {
    Variable(VariableDeclaration),
    Expression(Expression),
}

#[derive(Clone, Debug)]
pub(crate) struct ForInStatement {
    pub left: ForInOfLeft,
    pub right: Expression,
    pub body: Box<Statement>,
}

#[derive(Clone, Debug)]
pub(crate) struct ForOfStatement {
    pub left: ForInOfLeft,
    pub right: Expression,
    pub body: Box<Statement>,
    pub is_await: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum ForInOfLeft {
    Variable(VariableDeclaration),
    Pattern(Pattern),
    Expression(Expression),
}

#[derive(Clone, Debug)]
pub(crate) struct TryStatement {
    pub block: Vec<Statement>,
    pub handler: Option<CatchClause>,
    pub finalizer: Option<Vec<Statement>>,
}

#[derive(Clone, Debug)]
pub(crate) struct CatchClause {
    pub param: Option<Pattern>,
    pub body: Vec<Statement>,
}

#[derive(Clone, Debug)]
pub(crate) struct SwitchStatement {
    pub discriminant: Expression,
    pub cases: Vec<SwitchCase>,
}

#[derive(Clone, Debug)]
pub(crate) struct SwitchCase {
    pub test: Option<Expression>,
    pub consequent: Vec<Statement>,
}

#[derive(Clone, Debug)]
pub(crate) struct FunctionDecl {
    pub name: String,
    pub params: Vec<Pattern>,
    pub body: Body,
    pub is_async: bool,
    pub is_generator: bool,
    pub source_text: Option<SourceText>,
    pub body_is_strict: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct FunctionExpr {
    pub name: Option<String>,
    pub params: Vec<Pattern>,
    pub body: Body,
    pub is_async: bool,
    pub is_generator: bool,
    pub source_text: Option<SourceText>,
    pub body_is_strict: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ArrowFunction {
    pub params: Vec<Pattern>,
    pub body: ArrowBody,
    pub is_async: bool,
    pub source_text: Option<SourceText>,
    pub body_is_strict: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum ArrowBody {
    /// Concise arrow-function body: `() => expr`. The `Body` contains a single
    /// `Statement::Expression` so it participates in the same per-body IC
    /// numbering and store as a block arrow body.
    Expression(Body),
    /// Block arrow-function body: `() => { ... }`.
    Block(Body),
}

impl ArrowBody {
    pub(crate) fn body(&self) -> &Body {
        match self {
            ArrowBody::Expression(b) | ArrowBody::Block(b) => b,
        }
    }

    pub(crate) fn body_mut(&mut self) -> &mut Body {
        match self {
            ArrowBody::Expression(b) | ArrowBody::Block(b) => b,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ClassDecl {
    pub name: String,
    pub super_class: Option<Box<Expression>>,
    pub body: Vec<ClassElement>,
    pub source_text: Option<SourceText>,
}

#[derive(Clone, Debug)]
pub(crate) struct ClassExpr {
    pub name: Option<String>,
    pub super_class: Option<Box<Expression>>,
    pub body: Vec<ClassElement>,
    pub source_text: Option<SourceText>,
}

#[derive(Clone, Debug)]
pub(crate) enum ClassElement {
    Method(ClassMethod),
    Property(ClassProperty),
    AutoAccessor(ClassProperty),
    StaticBlock(Vec<Statement>),
}

#[derive(Clone, Debug)]
pub(crate) struct ClassMethod {
    pub key: PropertyKey,
    pub kind: ClassMethodKind,
    pub value: FunctionExpr,
    pub is_static: bool,
    pub computed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClassMethodKind {
    Method,
    Get,
    Set,
    Constructor,
}

#[derive(Clone, Debug)]
pub(crate) struct ClassProperty {
    pub key: PropertyKey,
    pub value: Option<Expression>,
    pub is_static: bool,
    pub computed: bool,
}

impl Expression {
    /// Per spec §13.2.1.2 — returns true only for function/class/arrow
    /// expressions that have no binding name of their own.
    pub(crate) fn is_anonymous_function_definition(&self) -> bool {
        match self {
            Expression::Function(f) => f.name.as_ref().is_none_or(|n| n.is_empty()),
            Expression::ArrowFunction(_) => true,
            Expression::Class(c) => c.name.as_ref().is_none_or(|n| n.is_empty()),
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TemplateLiteral {
    pub id: u64,
    pub quasis: Vec<Option<Vec<u16>>>,
    pub raw_quasis: Vec<String>,
    pub expressions: Vec<Expression>,
}

/// Check if a function (body + params) references the `arguments` identifier.
/// Also checks parameter default expressions (which can reference arguments).
pub(crate) fn func_uses_arguments(params: &[Pattern], body: &Body) -> bool {
    params_use_arguments(params) || stmts_use_arguments(body.as_slice())
}

/// A "simple" parameter list (§15.1.3 IsSimpleParameterList) is one consisting
/// solely of single-name (identifier) bindings — no rest, defaults, or
/// destructuring. This gates the fast parameter-binding path and mapped
/// `arguments` objects, so it is cached on `JsFunction::User` at creation time.
pub(crate) fn params_are_simple(params: &[Pattern]) -> bool {
    params.iter().all(|p| matches!(p, Pattern::Identifier(_)))
}

fn params_use_arguments(params: &[Pattern]) -> bool {
    params.iter().any(pattern_uses_arguments)
}

/// Check if a function body references the `arguments` identifier.
/// Recurses into arrow functions (they inherit arguments) but not into
/// regular functions, generators, or class methods (they have their own).
pub(crate) fn stmts_use_arguments(stmts: &[Statement]) -> bool {
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

/// `ContainsArguments` over an expression, walked with an explicit stack.
///
/// Flat operand chains (`a + b + c + ...`) and member/call chains
/// (`a.b.b.b...`) parse in a continuation loop, so they are not bounded by
/// `MAX_PARSE_DEPTH` and nest as deeply as the source is long. Native
/// recursion here overflowed the engine stack on such input (jsse#612), and
/// this predicate runs at function-object creation time, where there is no way
/// to report a failure. The explicit stack makes it O(1) native stack.
///
/// The stack pops in a different order than the old short-circuiting `||`
/// chain visited children, which is immaterial: this is a side-effect-free
/// predicate, so only *whether* a matching node exists is observable.
fn expr_uses_arguments(expr: &Expression) -> bool {
    let mut stack = vec![expr];
    while let Some(e) = stack.pop() {
        match e {
            Expression::Identifier(name) => {
                if name == "arguments" {
                    return true;
                }
            }
            Expression::Literal(_)
            | Expression::This
            | Expression::Super
            | Expression::ImportMeta
            | Expression::NewTarget
            | Expression::PrivateIdentifier(_) => {}
            // Regular functions have their own `arguments`. Classes have their own
            // scope for method bodies, but `extends` and computed class element keys
            // evaluate in the enclosing scope and may reference `arguments`.
            Expression::Function(_) => {}
            Expression::Class(c) => {
                if class_extends_or_computed_keys_use_arguments(c.super_class.as_deref(), &c.body) {
                    return true;
                }
            }
            // DO recurse into arrow functions (they inherit arguments)
            Expression::ArrowFunction(a) => {
                let body = a.body.body();
                match body.statements.as_slice() {
                    [Statement::Return(Some(e))] => stack.push(e),
                    stmts => {
                        if stmts_use_arguments(stmts) {
                            return true;
                        }
                    }
                }
            }
            Expression::Array(elems, _) => stack.extend(elems.iter().flatten()),
            Expression::Object(props, _) => {
                for p in props.iter() {
                    stack.push(&p.value);
                    if let PropertyKey::Computed(e) = &p.key {
                        stack.push(e);
                    }
                }
            }
            Expression::Unary(_, e)
            | Expression::Update(_, _, e)
            | Expression::Spread(e)
            | Expression::Yield(Some(e), _)
            | Expression::Await(e)
            | Expression::Typeof(e)
            | Expression::Void(e)
            | Expression::Delete(e) => stack.push(e),
            Expression::Yield(None, _) => {}
            Expression::Binary(_, l, r)
            | Expression::Logical(_, l, r)
            | Expression::Assign(_, l, r) => {
                stack.push(l);
                stack.push(r);
            }
            Expression::Conditional(t, c, a) => {
                stack.push(t);
                stack.push(c);
                stack.push(a);
            }
            Expression::Call(callee, args, _) => {
                // A direct `eval(...)` call can itself reference `arguments`.
                if matches!(&**callee, Expression::Identifier(name) if name == "eval") {
                    return true;
                }
                stack.push(callee);
                stack.extend(args.iter());
            }
            Expression::New(callee, args, _) => {
                stack.push(callee);
                stack.extend(args.iter());
            }
            Expression::Member(obj, prop, _) => {
                stack.push(obj);
                if let MemberProperty::Computed(e) = prop {
                    stack.push(e);
                }
            }
            Expression::OptionalChain(base, chain) => {
                stack.push(base);
                stack.push(chain);
            }
            Expression::Comma(exprs) | Expression::Sequence(exprs) => stack.extend(exprs.iter()),
            Expression::TaggedTemplate(tag, tpl) => {
                stack.push(tag);
                stack.extend(tpl.expressions.iter());
            }
            Expression::Template(tpl) => stack.extend(tpl.expressions.iter()),
            Expression::Import(spec, opts)
            | Expression::ImportDefer(spec, opts)
            | Expression::ImportSource(spec, opts) => {
                stack.push(spec);
                if let Some(opts) = opts.as_deref() {
                    stack.push(opts);
                }
            }
        }
    }
    false
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

/// Predicate for the `ContainsArguments` static semantic: an unqualified
/// reference to the `arguments` identifier.
pub(crate) fn is_arguments_reference(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(name) if name == "arguments")
}

/// Predicate for the `Contains SuperCall` static semantic: a `super(...)` call.
/// `super.prop` is a SuperProperty (not a SuperCall) and does not match.
pub(crate) fn is_super_call(expr: &Expression) -> bool {
    matches!(expr, Expression::Call(callee, _, _) if matches!(&**callee, Expression::Super))
}

/// Returns true if `pred` matches any expression syntactically reachable from
/// `stmts` within the same function scope.
///
/// This is the single traversal behind the `ContainsArguments` and
/// `Contains SuperCall` static-semantic early errors — class field
/// initializers, class static blocks, and direct `eval` textually inside
/// either. Arrow bodies and class computed keys ARE traversed (they execute in
/// the enclosing scope); nested function, method, getter/setter, and non-arrow
/// bodies are opaque, and binding patterns (which introduce names rather than
/// reference them) are not visited. This deliberately differs from
/// [`stmts_use_arguments`], which drives the `arguments`-object allocation
/// optimization and therefore also inspects binding names and nested `eval`.
///
/// The match is exhaustive over `Statement`/`Expression`: a new AST variant
/// forces this one traversal to be updated rather than silently slipping past a
/// hand-maintained copy.
pub(crate) fn stmts_contain_matching(
    stmts: &[Statement],
    pred: &dyn Fn(&Expression) -> bool,
) -> bool {
    stmts.iter().any(|s| stmt_contains_matching(s, pred))
}

fn stmt_contains_matching(stmt: &Statement, pred: &dyn Fn(&Expression) -> bool) -> bool {
    match stmt {
        Statement::Empty
        | Statement::Debugger
        | Statement::Break(_)
        | Statement::Continue(_)
        | Statement::Return(None)
        // Nested functions own their `arguments`/`super`; opaque.
        | Statement::FunctionDeclaration(_) => false,
        Statement::Expression(e) | Statement::Throw(e) => expr_contains_matching(e, pred),
        Statement::Return(Some(e)) => expr_contains_matching(e, pred),
        Statement::Block(stmts) => stmts_contain_matching(stmts, pred),
        Statement::Variable(decl) => decl
            .declarations
            .iter()
            .any(|d| d.init.as_ref().is_some_and(|e| expr_contains_matching(e, pred))),
        Statement::If(i) => {
            expr_contains_matching(&i.test, pred)
                || stmt_contains_matching(&i.consequent, pred)
                || i.alternate
                    .as_ref()
                    .is_some_and(|a| stmt_contains_matching(a, pred))
        }
        Statement::While(w) => {
            expr_contains_matching(&w.test, pred) || stmt_contains_matching(&w.body, pred)
        }
        Statement::DoWhile(d) => {
            expr_contains_matching(&d.test, pred) || stmt_contains_matching(&d.body, pred)
        }
        Statement::For(f) => {
            f.init.as_ref().is_some_and(|i| match i {
                ForInit::Expression(e) => expr_contains_matching(e, pred),
                ForInit::Variable(d) => d
                    .declarations
                    .iter()
                    .any(|dd| dd.init.as_ref().is_some_and(|e| expr_contains_matching(e, pred))),
            }) || f.test.as_ref().is_some_and(|e| expr_contains_matching(e, pred))
                || f.update.as_ref().is_some_and(|e| expr_contains_matching(e, pred))
                || stmt_contains_matching(&f.body, pred)
        }
        Statement::ForIn(f) => {
            expr_contains_matching(&f.right, pred) || stmt_contains_matching(&f.body, pred)
        }
        Statement::ForOf(f) => {
            expr_contains_matching(&f.right, pred) || stmt_contains_matching(&f.body, pred)
        }
        Statement::Try(t) => {
            stmts_contain_matching(&t.block, pred)
                || t.handler
                    .as_ref()
                    .is_some_and(|h| stmts_contain_matching(&h.body, pred))
                || t.finalizer
                    .as_ref()
                    .is_some_and(|f| stmts_contain_matching(f, pred))
        }
        Statement::Switch(s) => {
            expr_contains_matching(&s.discriminant, pred)
                || s.cases.iter().any(|c| {
                    c.test
                        .as_ref()
                        .is_some_and(|e| expr_contains_matching(e, pred))
                        || stmts_contain_matching(&c.consequent, pred)
                })
        }
        Statement::Labeled(_, s) => stmt_contains_matching(s, pred),
        Statement::With(e, s) => {
            expr_contains_matching(e, pred) || stmt_contains_matching(s, pred)
        }
        // Method bodies are opaque, but `extends` and computed element keys
        // evaluate in the enclosing scope.
        Statement::ClassDeclaration(cls) => {
            cls.super_class
                .as_ref()
                .is_some_and(|sc| expr_contains_matching(sc, pred))
                || class_elements_contain_matching(&cls.body, pred)
        }
    }
}

pub(crate) fn expr_contains_matching(
    expr: &Expression,
    pred: &dyn Fn(&Expression) -> bool,
) -> bool {
    if pred(expr) {
        return true;
    }
    match expr {
        // Leaves with no in-scope child expressions, plus nested regular
        // functions (opaque to `arguments`/`super`).
        Expression::Literal(_)
        | Expression::Identifier(_)
        | Expression::This
        | Expression::Super
        | Expression::NewTarget
        | Expression::ImportMeta
        | Expression::PrivateIdentifier(_)
        | Expression::Function(_) => false,
        Expression::Array(elems, _) => elems
            .iter()
            .any(|e| e.as_ref().is_some_and(|e| expr_contains_matching(e, pred))),
        Expression::Object(props, _) => props.iter().any(|p| {
            expr_contains_matching(&p.value, pred)
                || matches!(&p.key, PropertyKey::Computed(e) if expr_contains_matching(e, pred))
        }),
        Expression::Member(object, property, _) => {
            expr_contains_matching(object, pred)
                || matches!(property, MemberProperty::Computed(e) if expr_contains_matching(e, pred))
        }
        Expression::Call(callee, args, _) | Expression::New(callee, args, _) => {
            expr_contains_matching(callee, pred)
                || args.iter().any(|a| expr_contains_matching(a, pred))
        }
        Expression::Binary(_, l, r)
        | Expression::Logical(_, l, r)
        | Expression::Assign(_, l, r) => {
            expr_contains_matching(l, pred) || expr_contains_matching(r, pred)
        }
        Expression::Unary(_, e)
        | Expression::Update(_, _, e)
        | Expression::Spread(e)
        | Expression::Await(e)
        | Expression::Typeof(e)
        | Expression::Void(e)
        | Expression::Delete(e) => expr_contains_matching(e, pred),
        Expression::Yield(opt, _) => opt
            .as_ref()
            .is_some_and(|e| expr_contains_matching(e, pred)),
        Expression::Conditional(t, c, a) => {
            expr_contains_matching(t, pred)
                || expr_contains_matching(c, pred)
                || expr_contains_matching(a, pred)
        }
        Expression::Sequence(exprs) | Expression::Comma(exprs) => {
            exprs.iter().any(|e| expr_contains_matching(e, pred))
        }
        Expression::Template(tl) => tl
            .expressions
            .iter()
            .any(|e| expr_contains_matching(e, pred)),
        Expression::TaggedTemplate(tag, tl) => {
            expr_contains_matching(tag, pred)
                || tl
                    .expressions
                    .iter()
                    .any(|e| expr_contains_matching(e, pred))
        }
        Expression::OptionalChain(object, chain) => {
            expr_contains_matching(object, pred) || expr_contains_matching(chain, pred)
        }
        Expression::Import(inner, opts)
        | Expression::ImportDefer(inner, opts)
        | Expression::ImportSource(inner, opts) => {
            expr_contains_matching(inner, pred)
                || opts
                    .as_ref()
                    .is_some_and(|e| expr_contains_matching(e, pred))
        }
        // Arrow functions inherit `arguments`/`super`, so the body executes in
        // the enclosing scope and IS traversed.
        Expression::ArrowFunction(af) => match af.body.body().statements.as_slice() {
            [Statement::Return(Some(e))] => expr_contains_matching(e, pred),
            stmts => stmts_contain_matching(stmts, pred),
        },
        // Method/field bodies are opaque, but `extends` and computed element
        // keys evaluate in the enclosing scope.
        Expression::Class(cls) => {
            cls.super_class
                .as_ref()
                .is_some_and(|sc| expr_contains_matching(sc, pred))
                || class_elements_contain_matching(&cls.body, pred)
        }
    }
}

fn class_elements_contain_matching(
    body: &[ClassElement],
    pred: &dyn Fn(&Expression) -> bool,
) -> bool {
    body.iter().any(|elem| match elem {
        ClassElement::Method(m) => {
            matches!(&m.key, PropertyKey::Computed(e) if expr_contains_matching(e, pred))
        }
        ClassElement::Property(p) | ClassElement::AutoAccessor(p) => {
            matches!(&p.key, PropertyKey::Computed(e) if expr_contains_matching(e, pred))
        }
        ClassElement::StaticBlock(_) => false,
    })
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

/// One unit of work for [`assign_expr_sites`]'s explicit stack.
///
/// Site ids are assigned in *post-order*: every id inside a node is handed out
/// before the node's own. `IcStore` sizes its slot arrays from the final counts
/// and both the tree-walker and the bytecode compiler index them by these ids,
/// so the numbering is part of the contract and must not shift. An explicit
/// stack preserves it by pushing the `Set*` marker *before* the node's
/// children, so it pops back off *after* all of them.
enum SiteWork<'a> {
    Expr(&'a mut Expression),
    SetCall(&'a mut CallSiteId),
    SetProp(&'a mut PropSiteId),
}

/// Number every call/new/member site reachable from `expr`, walked with an
/// explicit stack.
///
/// Flat operand chains (`a + b + c + ...`) and member/call chains
/// (`a.b.b.b...`) parse in a continuation loop rather than by recursive
/// descent, so they are deliberately not bounded by `MAX_PARSE_DEPTH` and nest
/// as deeply as the source is long. Native recursion here overflowed the engine
/// stack — SIGABRT, uncatchable — at ~122k operands on a debug build and ~3-4M
/// on release (jsse#612). This pass runs on every successful parse and has no
/// way to report a failure, so it must use O(1) native stack.
///
/// Nested function, arrow, and class-method bodies each open a *fresh* id
/// namespace via [`assign_ic_sites`], so they stay ordinary nested calls: that
/// recursion is bounded by function/class nesting depth, which recursive
/// descent does funnel through `MAX_PARSE_DEPTH`.
fn assign_expr_sites<'a>(expr: &'a mut Expression, call_id: &mut u32, prop_id: &mut u32) {
    let mut stack: Vec<SiteWork<'a>> = vec![SiteWork::Expr(expr)];
    while let Some(work) = stack.pop() {
        let expr = match work {
            SiteWork::SetCall(site) => {
                *site = CallSiteId(*call_id);
                *call_id += 1;
                continue;
            }
            SiteWork::SetProp(site) => {
                *site = PropSiteId(*prop_id);
                *prop_id += 1;
                continue;
            }
            SiteWork::Expr(expr) => expr,
        };
        match expr {
            Expression::Call(callee, args, site) | Expression::New(callee, args, site) => {
                stack.push(SiteWork::SetCall(site));
                for a in args.iter_mut().rev() {
                    stack.push(SiteWork::Expr(a));
                }
                stack.push(SiteWork::Expr(callee));
            }
            Expression::Member(obj, prop, site) => {
                stack.push(SiteWork::SetProp(site));
                if let MemberProperty::Computed(e) = prop {
                    stack.push(SiteWork::Expr(e));
                }
                stack.push(SiteWork::Expr(obj));
            }
            Expression::OptionalChain(base, chain) => {
                stack.push(SiteWork::Expr(chain));
                stack.push(SiteWork::Expr(base));
            }
            Expression::Unary(_, e)
            | Expression::Update(_, _, e)
            | Expression::Spread(e)
            | Expression::Yield(Some(e), _)
            | Expression::Await(e)
            | Expression::Typeof(e)
            | Expression::Void(e)
            | Expression::Delete(e) => stack.push(SiteWork::Expr(e)),
            Expression::Yield(None, _) => {}
            Expression::Binary(_, l, r)
            | Expression::Logical(_, l, r)
            | Expression::Assign(_, l, r) => {
                stack.push(SiteWork::Expr(r));
                stack.push(SiteWork::Expr(l));
            }
            Expression::Conditional(t, c, a) => {
                stack.push(SiteWork::Expr(a));
                stack.push(SiteWork::Expr(c));
                stack.push(SiteWork::Expr(t));
            }
            Expression::Array(elems, _) => {
                for e in elems.iter_mut().rev().flatten() {
                    stack.push(SiteWork::Expr(e));
                }
            }
            Expression::Object(props, _) => {
                for p in props.iter_mut().rev() {
                    stack.push(SiteWork::Expr(&mut p.value));
                    if let PropertyKey::Computed(e) = &mut p.key {
                        stack.push(SiteWork::Expr(e));
                    }
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
                for e in tpl.expressions.iter_mut().rev() {
                    stack.push(SiteWork::Expr(e));
                }
                stack.push(SiteWork::Expr(tag));
            }
            Expression::Template(tpl) => {
                for e in tpl.expressions.iter_mut().rev() {
                    stack.push(SiteWork::Expr(e));
                }
            }
            Expression::Comma(exprs) | Expression::Sequence(exprs) => {
                for e in exprs.iter_mut().rev() {
                    stack.push(SiteWork::Expr(e));
                }
            }
            Expression::Import(spec, opts)
            | Expression::ImportDefer(spec, opts)
            | Expression::ImportSource(spec, opts) => {
                if let Some(opts) = opts.as_deref_mut() {
                    stack.push(SiteWork::Expr(opts));
                }
                stack.push(SiteWork::Expr(spec));
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
pub(crate) fn clear_expr_ic_sites(expr: &mut Expression) {
    // Walked with an explicit stack for the same reason as `assign_expr_sites`:
    // flat operand and member/call chains nest as deeply as the source is long
    // (they parse in a continuation loop, so `MAX_PARSE_DEPTH` does not bound
    // them), and this pass cannot report a failure — jsse#612. Order is
    // irrelevant here, since every site is simply reset to `UNASSIGNED`.
    let mut stack = vec![expr];
    while let Some(expr) = stack.pop() {
        match expr {
            Expression::Call(callee, args, site) | Expression::New(callee, args, site) => {
                *site = CallSiteId::UNASSIGNED;
                stack.push(callee);
                stack.extend(args.iter_mut());
            }
            Expression::Member(obj, prop, site) => {
                *site = PropSiteId::UNASSIGNED;
                stack.push(obj);
                if let MemberProperty::Computed(e) = prop {
                    stack.push(e);
                }
            }
            Expression::OptionalChain(base, chain) => {
                stack.push(base);
                stack.push(chain);
            }
            Expression::Unary(_, e)
            | Expression::Update(_, _, e)
            | Expression::Spread(e)
            | Expression::Yield(Some(e), _)
            | Expression::Await(e)
            | Expression::Typeof(e)
            | Expression::Void(e)
            | Expression::Delete(e) => stack.push(e),
            Expression::Yield(None, _) => {}
            Expression::Binary(_, l, r)
            | Expression::Logical(_, l, r)
            | Expression::Assign(_, l, r) => {
                stack.push(l);
                stack.push(r);
            }
            Expression::Conditional(t, c, a) => {
                stack.push(t);
                stack.push(c);
                stack.push(a);
            }
            Expression::Array(elems, _) => stack.extend(elems.iter_mut().flatten()),
            Expression::Object(props, _) => {
                for p in props.iter_mut() {
                    stack.push(&mut p.value);
                    if let PropertyKey::Computed(e) = &mut p.key {
                        stack.push(e);
                    }
                }
            }
            // Nested function/arrow bodies run under their own IC store — leave them.
            Expression::Function(_) | Expression::ArrowFunction(_) => {}
            Expression::Class(c) => {
                clear_class_def_ic_sites(c.super_class.as_deref_mut(), &mut c.body);
            }
            Expression::TaggedTemplate(tag, tpl) => {
                stack.push(tag);
                stack.extend(tpl.expressions.iter_mut());
            }
            Expression::Template(tpl) => stack.extend(tpl.expressions.iter_mut()),
            Expression::Comma(exprs) | Expression::Sequence(exprs) => {
                stack.extend(exprs.iter_mut())
            }
            Expression::Import(spec, opts)
            | Expression::ImportDefer(spec, opts)
            | Expression::ImportSource(spec, opts) => {
                stack.push(spec);
                if let Some(opts) = opts.as_deref_mut() {
                    stack.push(opts);
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
}

/// Clear IC sites in the parts of a class literal that are evaluated at
/// class-definition time (`extends`, computed keys, static-field initializers,
/// static blocks). Used from `clear_expr_ic_sites` when a class appears inside a
/// generator/async terminator expression: those parts run while the terminator
/// is evaluated (under the caller's IC handle), so their sites must be cleared.
/// Method bodies and instance-field initializers run under their own stores /
/// the construction handle and are left untouched.
fn clear_class_def_ic_sites(super_class: Option<&mut Expression>, body: &mut [ClassElement]) {
    if let Some(sc) = super_class {
        clear_expr_ic_sites(sc);
    }
    for el in body.iter_mut() {
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
            ClassElement::StaticBlock(stmts) => {
                for s in stmts.iter_mut() {
                    clear_stmt_ic_sites(s);
                }
            }
        }
    }
}

/// Statement companion to [`clear_expr_ic_sites`]. Resets IC sites in statements
/// reachable from a generator/async terminator (e.g. inside a class static block
/// that runs at class-definition time). Nested function-declaration bodies are
/// left intact — they execute under their own stores.
fn clear_stmt_ic_sites(stmt: &mut Statement) {
    match stmt {
        Statement::Expression(e) => clear_expr_ic_sites(e),
        Statement::Block(stmts) => {
            for s in stmts.iter_mut() {
                clear_stmt_ic_sites(s);
            }
        }
        Statement::Variable(decl) => {
            for d in decl.declarations.iter_mut() {
                clear_pattern_ic_sites(&mut d.pattern);
                if let Some(init) = d.init.as_mut() {
                    clear_expr_ic_sites(init);
                }
            }
        }
        Statement::If(i) => {
            clear_expr_ic_sites(&mut i.test);
            clear_stmt_ic_sites(&mut i.consequent);
            if let Some(alt) = i.alternate.as_mut() {
                clear_stmt_ic_sites(alt);
            }
        }
        Statement::While(w) => {
            clear_expr_ic_sites(&mut w.test);
            clear_stmt_ic_sites(&mut w.body);
        }
        Statement::DoWhile(d) => {
            clear_stmt_ic_sites(&mut d.body);
            clear_expr_ic_sites(&mut d.test);
        }
        Statement::For(f) => {
            if let Some(init) = f.init.as_mut() {
                match init {
                    ForInit::Expression(e) => clear_expr_ic_sites(e),
                    ForInit::Variable(decl) => {
                        for d in decl.declarations.iter_mut() {
                            clear_pattern_ic_sites(&mut d.pattern);
                            if let Some(init) = d.init.as_mut() {
                                clear_expr_ic_sites(init);
                            }
                        }
                    }
                }
            }
            if let Some(test) = f.test.as_mut() {
                clear_expr_ic_sites(test);
            }
            if let Some(update) = f.update.as_mut() {
                clear_expr_ic_sites(update);
            }
            clear_stmt_ic_sites(&mut f.body);
        }
        Statement::ForIn(f) => {
            clear_for_in_of_left(&mut f.left);
            clear_expr_ic_sites(&mut f.right);
            clear_stmt_ic_sites(&mut f.body);
        }
        Statement::ForOf(f) => {
            clear_for_in_of_left(&mut f.left);
            clear_expr_ic_sites(&mut f.right);
            clear_stmt_ic_sites(&mut f.body);
        }
        Statement::Return(e) => {
            if let Some(e) = e.as_mut() {
                clear_expr_ic_sites(e);
            }
        }
        Statement::Throw(e) => clear_expr_ic_sites(e),
        Statement::Try(t) => {
            for s in t.block.iter_mut() {
                clear_stmt_ic_sites(s);
            }
            if let Some(h) = t.handler.as_mut() {
                if let Some(param) = h.param.as_mut() {
                    clear_pattern_ic_sites(param);
                }
                for s in h.body.iter_mut() {
                    clear_stmt_ic_sites(s);
                }
            }
            if let Some(f) = t.finalizer.as_mut() {
                for s in f.iter_mut() {
                    clear_stmt_ic_sites(s);
                }
            }
        }
        Statement::Switch(s) => {
            clear_expr_ic_sites(&mut s.discriminant);
            for c in s.cases.iter_mut() {
                if let Some(test) = c.test.as_mut() {
                    clear_expr_ic_sites(test);
                }
                for stmt in c.consequent.iter_mut() {
                    clear_stmt_ic_sites(stmt);
                }
            }
        }
        Statement::Labeled(_, s) => clear_stmt_ic_sites(s),
        Statement::With(e, s) => {
            clear_expr_ic_sites(e);
            clear_stmt_ic_sites(s);
        }
        // Nested function bodies execute under their own IC store — leave them.
        Statement::FunctionDeclaration(_) => {}
        Statement::ClassDeclaration(c) => {
            clear_class_def_ic_sites(c.super_class.as_deref_mut(), &mut c.body);
        }
        Statement::Empty | Statement::Break(_) | Statement::Continue(_) | Statement::Debugger => {}
    }
}

pub(crate) fn clear_for_in_of_left(left: &mut ForInOfLeft) {
    match left {
        ForInOfLeft::Variable(decl) => {
            for d in decl.declarations.iter_mut() {
                clear_pattern_ic_sites(&mut d.pattern);
                if let Some(init) = d.init.as_mut() {
                    clear_expr_ic_sites(init);
                }
            }
        }
        ForInOfLeft::Pattern(p) => clear_pattern_ic_sites(p),
        ForInOfLeft::Expression(e) => clear_expr_ic_sites(e),
    }
}

/// Pattern companion to [`clear_expr_ic_sites`], for patterns reachable from a
/// terminator (e.g. destructuring declarations inside a class static block, or
/// a for-of binding / catch parameter evaluated at a state transition).
pub(crate) fn clear_pattern_ic_sites(pat: &mut Pattern) {
    match pat {
        Pattern::Identifier(_) => {}
        Pattern::Array(elems) => {
            for e in elems.iter_mut().flatten() {
                match e {
                    ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                        clear_pattern_ic_sites(p)
                    }
                }
            }
        }
        Pattern::Object(props) => {
            for p in props.iter_mut() {
                match p {
                    ObjectPatternProperty::KeyValue(key, pat) => {
                        if let PropertyKey::Computed(e) = key {
                            clear_expr_ic_sites(e);
                        }
                        clear_pattern_ic_sites(pat);
                    }
                    ObjectPatternProperty::Shorthand(_) => {}
                    ObjectPatternProperty::Rest(pat) => clear_pattern_ic_sites(pat),
                }
            }
        }
        Pattern::Assign(pat, expr) => {
            clear_pattern_ic_sites(pat);
            clear_expr_ic_sites(expr);
        }
        Pattern::Rest(pat) => clear_pattern_ic_sites(pat),
        Pattern::MemberExpression(e) => {
            // The top-level member is an assignment target (no IC site of its
            // own); only its sub-expressions carry sites.
            match &mut **e {
                Expression::Member(obj, prop, _) => {
                    clear_expr_ic_sites(obj);
                    if let MemberProperty::Computed(e) = prop {
                        clear_expr_ic_sites(e);
                    }
                }
                other => clear_expr_ic_sites(other),
            }
        }
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

    fn num(v: f64) -> Expression {
        Expression::Literal(Literal::Number(v))
    }

    fn add(l: Expression, r: Expression) -> Expression {
        Expression::Binary(BinaryOp::Add, Box::new(l), Box::new(r))
    }

    /// `a.b.b.b…`, `depth` members deep. Built with a loop, so the *builder*
    /// is not what is under test.
    fn deep_member_chain(depth: usize) -> Expression {
        let mut e = ident("a");
        for _ in 0..depth {
            e = prop(e, "b");
        }
        e
    }

    /// `seed + 1 + 1 + …`, `depth` additions deep — the flat operand chain
    /// from jsse#612.
    fn deep_add_chain(seed: Expression, depth: usize) -> Expression {
        let mut e = seed;
        for _ in 0..depth {
            e = add(e, num(1.0));
        }
        e
    }

    /// Nesting depth used by the stack-safety tests below. Chosen so that the
    /// *recursive* form of these passes could not possibly fit in
    /// `SMALL_STACK`: at the measured frame costs that is ~100 MB on debug and
    /// ~3 MB on release, against a 256 KiB stack. An iterative pass keeps its
    /// state on the heap and needs O(1) native stack, so it passes in both
    /// profiles — which is the point, since the abort ceiling itself is very
    /// profile-dependent (~122k debug vs ~3-4M release) and a fixed source
    /// size could never regression-test both.
    const DEEP: usize = 100_000;
    const SMALL_STACK: usize = 256 * 1024;

    /// Runs `f` on a thread far too small to hold the old recursive walks.
    ///
    /// A native stack overflow aborts the process rather than unwinding, so a
    /// regression here fails the whole test binary loudly instead of reporting
    /// a tidy assertion failure. That is intended: the bug being guarded
    /// against is precisely an uncatchable SIGABRT.
    fn on_small_stack<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
        std::thread::Builder::new()
            .stack_size(SMALL_STACK)
            .spawn(f)
            .expect("spawn probe thread")
            .join()
            .expect("probe thread panicked")
    }

    #[test]
    fn deep_member_chain_is_numbered_without_native_recursion() {
        on_small_stack(|| {
            let mut body = body_with(vec![expr_stmt(deep_member_chain(DEEP))]);
            assign_ic_sites(&mut body);
            assert_eq!(body.ic.prop_site_count, DEEP as u32);
            assert_eq!(body.ic.call_site_count, 0);

            // Post-order numbering: the innermost member is 0 and the
            // outermost DEEP-1. Walking out-to-in must see them descending
            // with no gaps — that density is what `IcStore` slot indexing
            // relies on.
            let mut cur = match &body.statements[0] {
                Statement::Expression(e) => e,
                other => panic!("expected an expression statement, got {other:?}"),
            };
            for expected in (0..DEEP as u32).rev() {
                match cur {
                    Expression::Member(obj, _, id) => {
                        assert_eq!(id.0, expected, "member at depth {expected}");
                        cur = obj;
                    }
                    other => panic!("expected Member at depth {expected}, got {other:?}"),
                }
            }
            leak_deep(body);
        });
    }

    #[test]
    fn deep_call_chain_is_numbered_without_native_recursion() {
        on_small_stack(|| {
            // `a()()()…` — the other production that parses in a continuation
            // loop and so is not bounded by `MAX_PARSE_DEPTH`.
            let mut e = ident("a");
            for _ in 0..DEEP {
                e = call(e, vec![]);
            }
            let mut body = body_with(vec![expr_stmt(e)]);
            assign_ic_sites(&mut body);
            assert_eq!(body.ic.call_site_count, DEEP as u32);
            assert_eq!(body.ic.prop_site_count, 0);
            leak_deep(body);
        });
    }

    #[test]
    fn deep_operand_chain_arguments_scan_has_no_native_recursion() {
        let (without, with_at_far_end) = on_small_stack(|| {
            // No `arguments` anywhere: the scan cannot short-circuit and must
            // walk every operand — the worst case for stack use.
            let clean = body_with(vec![Statement::Return(Some(deep_add_chain(
                num(1.0),
                DEEP,
            )))]);
            let without = func_uses_arguments(&[], &clean);

            // Same shape, with `arguments` buried at the far end of the chain.
            let dirty = body_with(vec![Statement::Return(Some(deep_add_chain(
                ident("arguments"),
                DEEP,
            )))]);
            let with_at_far_end = func_uses_arguments(&[], &dirty);

            leak_deep(clean);
            leak_deep(dirty);
            (without, with_at_far_end)
        });
        assert!(!without, "a chain of literals does not reference arguments");
        assert!(
            with_at_far_end,
            "`arguments` at the deep end of the chain must still be found"
        );
    }

    #[test]
    fn deep_member_chain_is_cleared_without_native_recursion() {
        on_small_stack(|| {
            let mut e = deep_member_chain(DEEP);
            let (mut call_id, mut prop_id) = (0u32, 0u32);
            assign_expr_sites(&mut e, &mut call_id, &mut prop_id);
            assert_eq!(prop_id, DEEP as u32);

            clear_expr_ic_sites(&mut e);
            let mut cur = &e;
            for depth in (0..DEEP).rev() {
                match cur {
                    Expression::Member(obj, _, id) => {
                        assert_eq!(*id, PropSiteId::UNASSIGNED, "member at depth {depth}");
                        cur = obj;
                    }
                    other => panic!("expected Member at depth {depth}, got {other:?}"),
                }
            }
            leak_deep(e);
        });
    }

    /// Deliberately leaks a deeply nested AST.
    ///
    /// `Expression`'s *drop glue* is still natively recursive — it is the one
    /// mandatory pass jsse#612 leaves alone, because `impl Drop for Expression`
    /// would make every by-value destructure of an `Expression` in the crate an
    /// E0509. Dropping a chain this deep would therefore overflow the small
    /// probe stack for a reason unrelated to the pass under test, so the tree
    /// is leaked instead; the process is about to exit anyway.
    fn leak_deep<T>(deep: T) {
        std::mem::forget(deep);
    }
}

#[cfg(test)]
mod bound_names_tests {
    use super::*;

    fn names_of(pat: &Pattern) -> Vec<String> {
        let mut out = Vec::new();
        pat.bound_names(&mut out);
        out
    }

    fn ident(name: &str) -> Pattern {
        Pattern::Identifier(name.to_string())
    }

    #[test]
    fn identifier_binds_its_name() {
        assert_eq!(names_of(&ident("x")), vec!["x"]);
    }

    #[test]
    fn member_expression_binds_nothing() {
        // `[obj.prop] = ...` — an assignment target, not a declaration.
        let pat = Pattern::MemberExpression(Box::new(Expression::Identifier("obj".to_string())));
        assert_eq!(names_of(&pat), Vec::<String>::new());
    }

    #[test]
    fn assignment_default_binds_only_the_inner_name() {
        // `a = 5` binds `a`; the default-value expression contributes no names.
        let pat = Pattern::Assign(
            Box::new(ident("a")),
            Box::new(Expression::Identifier("unused".to_string())),
        );
        assert_eq!(names_of(&pat), vec!["a"]);
    }

    #[test]
    fn rest_binds_inner_name() {
        let pat = Pattern::Rest(Box::new(ident("r")));
        assert_eq!(names_of(&pat), vec!["r"]);
    }

    #[test]
    fn array_pattern_binds_in_source_order_skipping_holes() {
        // `[a, , ...b]`
        let pat = Pattern::Array(vec![
            Some(ArrayPatternElement::Pattern(ident("a"))),
            None,
            Some(ArrayPatternElement::Rest(ident("b"))),
        ]);
        assert_eq!(names_of(&pat), vec!["a", "b"]);
    }

    #[test]
    fn object_pattern_binds_shorthand_and_value_and_rest_but_not_key() {
        // `{ s, k: v, ...r }` binds s, v, r — the property key `k` is not bound.
        let pat = Pattern::Object(vec![
            ObjectPatternProperty::Shorthand("s".to_string()),
            ObjectPatternProperty::KeyValue(PropertyKey::Identifier("k".to_string()), ident("v")),
            ObjectPatternProperty::Rest(ident("r")),
        ]);
        assert_eq!(names_of(&pat), vec!["s", "v", "r"]);
    }

    #[test]
    fn nested_pattern_flattens_in_source_order() {
        // `[{ a, k: [b, ...c] }]`
        let inner_array = Pattern::Array(vec![
            Some(ArrayPatternElement::Pattern(ident("b"))),
            Some(ArrayPatternElement::Rest(ident("c"))),
        ]);
        let inner_object = Pattern::Object(vec![
            ObjectPatternProperty::Shorthand("a".to_string()),
            ObjectPatternProperty::KeyValue(PropertyKey::Identifier("k".to_string()), inner_array),
        ]);
        let pat = Pattern::Array(vec![Some(ArrayPatternElement::Pattern(inner_object))]);
        assert_eq!(names_of(&pat), vec!["a", "b", "c"]);
    }

    #[test]
    fn appends_to_existing_vec() {
        let mut out = vec!["pre".to_string()];
        ident("x").bound_names(&mut out);
        assert_eq!(out, vec!["pre", "x"]);
    }
}
