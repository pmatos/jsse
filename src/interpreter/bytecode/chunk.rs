use crate::ast::{Body, Expression, FunctionLiteralInfo, SourceText, Statement};
use crate::interpreter::eval::{ClosureSpec, LiteralPropertySpec};
use crate::interpreter::{Atom, GlobalHint};
use crate::types::{JsBigInt, JsPropertyKey, JsString, JsValue};
use std::rc::Rc;

#[derive(Debug, Clone)]
pub(crate) enum Constant {
    Number(f64),
    String(JsString),
    BigInt(JsBigInt),
}

impl Constant {
    pub(crate) fn to_value(&self) -> JsValue {
        match self {
            Constant::Number(n) => JsValue::number(*n),
            Constant::String(s) => JsValue::string(s.clone()),
            Constant::BigInt(b) => JsValue::bigint(b.clone()),
        }
    }
}

/// A syntax node the VM hands back to the tree-walker. Held as a deep clone:
/// nested function bodies stay `Rc`-shared with the original AST, so body
/// identity (bytecode memo, hoist cache, inline-cache stores) is unchanged.
#[derive(Debug, Clone)]
pub(crate) enum AstNode {
    Expr(Expression),
    Stmt(Box<Statement>),
}

/// Where a `break`/`continue` that leaves an `ExecAst` statement resumes.
#[derive(Debug, Clone)]
pub(crate) struct EscapeEntry {
    pub(crate) is_continue: bool,
    pub(crate) label: Option<String>,
    /// Absolute code offset of the loop's break/continue target.
    pub(crate) target: u32,
    /// Block environments open at `target`; the VM unwinds to this depth
    /// before resuming there.
    pub(crate) scope_depth: u16,
}

/// What `PushEnv` instantiates in a fresh block environment
/// (BlockDeclarationInstantiation, sec-blockdeclarationinstantiation).
#[derive(Debug, Clone, Default)]
pub(crate) struct BlockScope {
    /// Function declaration statements (possibly labelled), closed over the
    /// new environment, in source order.
    pub(crate) functions: Vec<Statement>,
    /// `let`/`const`/class bindings entering their TDZ: `names` index and
    /// whether the binding is immutable.
    pub(crate) lexicals: Vec<(u16, bool)>,
}

/// A local-op slot operand with this bit set names a binding of a block
/// environment: bits 8-14 are the environment's index among the chunk's open
/// block environments (outermost first) and bits 0-7 its slot there. Without
/// it the operand is a slot in the frame (function) environment.
pub(crate) const BLOCK_SLOT_FLAG: u16 = 0x8000;

/// `InitLexical`'s slot operand when the compiler has no position to hint.
pub(crate) const NO_INIT_SLOT: u16 = u16::MAX;
pub(crate) const BLOCK_SLOT_MAX_SCOPES: usize = 0x80;
pub(crate) const BLOCK_SLOT_MAX_INDEX: usize = 0x100;

/// A function or arrow literal compiled to `MakeClosure`: what a closure of
/// it needs besides the environment, computed once at compile time so
/// creating a closure never re-reads the AST.
#[derive(Debug)]
pub(crate) struct FunctionTemplate {
    pub(crate) name: Option<String>,
    pub(crate) info: Rc<FunctionLiteralInfo>,
    pub(crate) body: Body,
    pub(crate) is_arrow: bool,
    pub(crate) is_async: bool,
    pub(crate) is_generator: bool,
    pub(crate) body_is_strict: bool,
    pub(crate) source_text: Option<SourceText>,
    /// The `name` NamedEvaluation gives the closure (the binding or property
    /// it is assigned to); `None` leaves the literal's own name.
    pub(crate) inferred_name: Option<JsString>,
}

impl FunctionTemplate {
    #[inline]
    pub(crate) fn spec(&self) -> ClosureSpec<'_> {
        ClosureSpec {
            name: self.name.as_deref(),
            info: &self.info,
            body: &self.body,
            is_arrow: self.is_arrow,
            is_async: self.is_async,
            is_generator: self.is_generator,
            body_is_strict: self.body_is_strict,
            source_text: &self.source_text,
        }
    }
}

/// An array literal whose elements are all primitive constants (or holes).
/// Each evaluation builds a fresh array from it.
#[derive(Debug)]
pub(crate) enum ArrayTemplate {
    Dense(Vec<JsValue>),
    Holey(Vec<Option<JsValue>>),
}

/// A regular expression literal: the `source` and `flags` a fresh RegExp
/// object is created with on every evaluation.
#[derive(Debug)]
pub(crate) struct RegExpTemplate {
    pub(crate) source: JsString,
    pub(crate) flags: JsString,
}

/// One statically keyed property of an object literal.
#[derive(Debug)]
pub(crate) struct ShapeProperty {
    /// Interned at compile time.
    pub(crate) key: JsPropertyKey,
    pub(crate) spec: LiteralPropertySpec,
}

/// An object literal's property list, in source order. `NewObject` pops one
/// value per entry.
#[derive(Debug)]
pub(crate) struct ObjectShape {
    pub(crate) props: Vec<ShapeProperty>,
}

/// `ExecAst` operand meaning "no break/continue can leave this statement".
pub(crate) const NO_ESCAPE_TABLE: u16 = u16::MAX;

#[derive(Debug, Default)]
pub(crate) struct Chunk {
    pub(crate) code: Vec<u8>,
    pub(crate) constants: Vec<Constant>,
    pub(crate) names: Vec<Atom>,
    /// One remembered global-object property slot per `names` entry, consulted
    /// by the name ops (`LoadName`, `ResolveName`, ...). Verified by key on
    /// every use, so it is never an invalidation concern. May be shorter than
    /// `names` (hand-built chunks): a missing entry means no hint.
    pub(crate) global_hints: Vec<GlobalHint>,
    pub(crate) var_names: Vec<u16>,
    pub(crate) max_stack: u16,
    pub(crate) max_refs: u16,
    /// Nodes executed on the tree-walker by `EvalAst`/`ExecAst`.
    pub(crate) ast: Vec<AstNode>,
    pub(crate) escape_tables: Vec<Vec<EscapeEntry>>,
    pub(crate) block_scopes: Vec<BlockScope>,
    pub(crate) functions: Vec<FunctionTemplate>,
    pub(crate) array_templates: Vec<ArrayTemplate>,
    pub(crate) object_shapes: Vec<ObjectShape>,
    pub(crate) regexps: Vec<RegExpTemplate>,
    /// The body declares functions or lexical bindings that the tree-walker's
    /// declaration instantiation must establish before the chunk runs.
    pub(crate) needs_instantiation: bool,
    /// Why each fallback node could not compile natively, one entry per node.
    #[cfg_attr(not(feature = "perf-counters"), allow(dead_code))]
    pub(crate) fallback_reasons: Vec<&'static str>,
}

impl Chunk {
    #[inline]
    pub(crate) fn global_hint(&self, name_idx: u16) -> Option<&GlobalHint> {
        self.global_hints.get(usize::from(name_idx))
    }
}
