//! Compact JavaScript AST generated from fuzzer bytes through `Arbitrary`.
//!
//! Every recursive enum lists a leaf variant first: when `Unstructured` runs
//! out of bytes it yields the lowest choice, so exhaustion terminates the
//! recursion. Identifiers, properties, strings and so on are `Ix` indexes into
//! the pools in `js_pools`; `js_print` renders the tree to source.

use arbitrary::Arbitrary;

pub type Ix = u8;
pub type Block = Vec<Stmt>;

#[derive(Debug, Clone, Arbitrary)]
pub struct Program {
    pub strict: bool,
    pub body: Block,
}

#[derive(Debug, Clone, Arbitrary)]
pub enum Target {
    Id(Ix),
    Member(Box<Expr>, Ix),
    Index(Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Arbitrary)]
pub struct PatElem {
    pub pat: Pat,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone, Arbitrary)]
pub enum Pat {
    Leaf(Target),
    Obj(Vec<(Ix, PatElem)>, Option<Target>),
    Arr(Vec<Option<PatElem>>, Option<Target>),
}

#[derive(Debug, Clone, Arbitrary)]
pub enum PStyle {
    Simple,
    Defaults(Expr),
    Rest,
    DestrObj,
    DestrArr,
}

#[derive(Debug, Clone, Arbitrary)]
pub struct Params {
    pub count: u8,
    pub style: PStyle,
}

#[derive(Debug, Clone, Arbitrary)]
pub struct FuncDef {
    pub is_async: bool,
    pub is_gen: bool,
    pub strict: bool,
    pub params: Params,
    pub body: Block,
}

#[derive(Debug, Clone, Arbitrary)]
pub enum ArrowBody {
    Expr(Box<Expr>),
    Block(Block),
}

#[derive(Debug, Clone, Arbitrary)]
pub struct Arrow {
    pub is_async: bool,
    pub params: Params,
    pub body: ArrowBody,
}

#[derive(Debug, Clone, Arbitrary)]
pub enum Key {
    Name(Ix),
    Str(Ix),
    Computed(Expr),
    Private,
}

#[derive(Debug, Clone, Arbitrary)]
pub enum MemberKind {
    Field(Key, Option<Expr>),
    Method(Key, FuncDef),
    Get(Key, Block),
    Set(Key, Block),
    StaticBlock(Block),
}

#[derive(Debug, Clone, Arbitrary)]
pub struct Member {
    pub is_static: bool,
    pub kind: MemberKind,
}

#[derive(Debug, Clone, Arbitrary)]
pub struct Ctor {
    pub params: Params,
    pub super_first: bool,
    pub body: Block,
}

#[derive(Debug, Clone, Arbitrary)]
pub struct ClassDef {
    pub extends: Option<Box<Expr>>,
    pub ctor: Option<Ctor>,
    pub members: Vec<Member>,
}

#[derive(Debug, Clone, Arbitrary)]
pub enum Elem {
    Hole,
    Val(Expr),
    Spread(Expr),
}

#[derive(Debug, Clone, Arbitrary)]
pub enum ObjProp {
    Shorthand(Ix),
    Kv(Ix, Expr),
    Computed(Expr, Expr),
    Method(Ix, FuncDef),
    Get(Ix, Block),
    Set(Ix, Block),
    Spread(Expr),
}

#[derive(Debug, Clone, Copy, Arbitrary)]
pub enum UnOp {
    Neg,
    Pos,
    Not,
    BitNot,
    Typeof,
    Void,
}

#[derive(Debug, Clone, Copy, Arbitrary)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Exp,
    Eq,
    Ne,
    StrictEq,
    StrictNe,
    Lt,
    Le,
    Gt,
    Ge,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
    In,
    InstanceOf,
    And,
    Or,
    Nullish,
}

#[derive(Debug, Clone, Copy, Arbitrary)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Exp,
    Shl,
    Shr,
    UShr,
    BitAnd,
    BitOr,
    BitXor,
    And,
    Or,
    Nullish,
}

#[derive(Debug, Clone, Arbitrary)]
pub enum Expr {
    Int(i16),
    Num(Ix),
    Str(Ix),
    Bool(bool),
    Null,
    Undef,
    BigInt(i8),
    This,
    Ident(Ix),
    Global(Ix),
    Arguments,
    NewTarget,
    WellKnown(Ix),
    Regex(Ix, Ix),
    Eval(Ix),
    SuperMember(Ix),
    Template(Vec<(Ix, Expr)>, Ix),
    Tagged(Box<Expr>, Vec<(Ix, Expr)>, Ix),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Update(bool, bool, Box<Target>),
    Assign(Box<Target>, AssignOp, Box<Expr>),
    DestructAssign(Box<Pat>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Seq(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Elem>, bool),
    MethodCall(Box<Expr>, Ix, Vec<Elem>, bool),
    StaticCall(Ix, Ix, Vec<Elem>),
    New(Box<Expr>, Vec<Elem>),
    SuperCall(Vec<Elem>),
    Member(Box<Expr>, Ix, bool),
    Index(Box<Expr>, Box<Expr>, bool),
    PrivateGet(Box<Expr>, Ix),
    PrivateIn(Ix, Box<Expr>),
    Delete(Box<Target>),
    Array(Vec<Elem>),
    Object(Vec<ObjProp>),
    Arrow(Box<Arrow>),
    Func(Box<FuncDef>),
    Class(Box<ClassDef>),
    Await(Box<Expr>),
    Yield(bool, Option<Box<Expr>>),
}

#[derive(Debug, Clone, Arbitrary)]
pub enum ForInit {
    None,
    Expr(Expr),
    Var(Ix, Expr),
    Let(Ix, Expr),
}

#[derive(Debug, Clone, Arbitrary)]
pub enum ForBinding {
    Var(Pat),
    Let(Ix),
    Const(Ix),
    Target(Target),
}

#[derive(Debug, Clone, Arbitrary)]
pub struct Case {
    pub test: Option<Expr>,
    pub body: Block,
}

#[derive(Debug, Clone, Arbitrary)]
pub struct Catch {
    pub param: Option<Ix>,
    pub body: Block,
}

#[derive(Debug, Clone, Arbitrary)]
pub enum Stmt {
    Expr(Expr),
    Var(Pat, Option<Expr>),
    Lex(bool, Ix, Expr),
    If(Expr, Block, Block),
    For(ForInit, Option<Expr>, Option<Expr>, Block),
    ForIn(ForBinding, Expr, Block),
    ForOf(ForBinding, Expr, Block, bool),
    While(Expr, Block),
    DoWhile(Block, Expr),
    Block(Block),
    Try(Block, Option<Catch>, Option<Block>),
    Throw(Expr),
    Return(Option<Expr>),
    Break(Option<Ix>),
    Continue(Option<Ix>),
    Switch(Expr, Vec<Case>),
    Labeled(Ix, Box<Stmt>),
    With(Expr, Block),
    Func(Ix, FuncDef),
    Class(Ix, ClassDef),
}
