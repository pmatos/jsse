/// AST node types for ECMAScript.
/// Each node represents a syntactic element from the spec.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceType {
    Script,
    Module,
}

#[derive(Clone, Debug)]
pub struct Program {
    pub source_type: SourceType,
    pub body: Vec<Statement>,
    pub module_items: Vec<ModuleItem>,
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
}

#[derive(Clone, Debug)]
pub enum ImportSpecifier {
    Named { imported: String, local: String },
    Default(String),
    Namespace(String),
    DeferredNamespace(String),
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
    Array(Vec<Option<Expression>>),
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
    Call(Box<Expression>, Vec<Expression>),
    New(Box<Expression>, Vec<Expression>),
    Member(Box<Expression>, MemberProperty),
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
    String(String),
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
    pub body: Vec<Statement>,
    pub is_async: bool,
    pub is_generator: bool,
    pub source_text: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FunctionExpr {
    pub name: Option<String>,
    pub params: Vec<Pattern>,
    pub body: Vec<Statement>,
    pub is_async: bool,
    pub is_generator: bool,
    pub source_text: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ArrowFunction {
    pub params: Vec<Pattern>,
    pub body: ArrowBody,
    pub is_async: bool,
    pub source_text: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ArrowBody {
    Expression(Box<Expression>),
    Block(Vec<Statement>),
}

#[derive(Clone, Debug)]
pub struct ClassDecl {
    pub name: String,
    pub super_class: Option<Box<Expression>>,
    pub body: Vec<ClassElement>,
    pub source_text: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ClassExpr {
    pub name: Option<String>,
    pub super_class: Option<Box<Expression>>,
    pub body: Vec<ClassElement>,
    pub source_text: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ClassElement {
    Method(ClassMethod),
    Property(ClassProperty),
    StaticBlock(Vec<Statement>),
}

#[derive(Clone, Debug)]
pub struct ClassMethod {
    pub key: PropertyKey,
    pub kind: ClassMethodKind,
    pub value: FunctionExpr,
    pub is_static: bool,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    pub quasis: Vec<Option<String>>,
    pub raw_quasis: Vec<String>,
    pub expressions: Vec<Expression>,
}
