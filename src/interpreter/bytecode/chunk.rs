use crate::ast::{Body, Expression, FunctionLiteralInfo, Pattern, SourceText, Statement};
use crate::interpreter::eval::{ClosureSpec, LiteralPropertySpec};
use crate::interpreter::types::BindingKind;
use crate::interpreter::{Atom, GlobalHint};
use crate::types::{JsBigInt, JsPropertyKey, JsString, JsValue};
use std::cell::Cell;
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
    /// Persistent operand-stack slots (for-in enumerators, completion
    /// records) live at `target`; the VM drops any above it.
    pub(crate) stack_height: u16,
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
    /// The environment is a `catch (identifier)` parameter scope, which B.3.4
    /// lets a `var` of the same name redeclare.
    pub(crate) simple_catch: bool,
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

#[derive(Debug)]
pub(crate) struct RegExpTemplate {
    pub(crate) source: JsString,
    pub(crate) flags: JsString,
}

#[derive(Debug)]
pub(crate) struct ShapeProperty {
    pub(crate) key: JsPropertyKey,
    pub(crate) spec: LiteralPropertySpec,
}

/// An object literal's property list, in source order. `NewObject` pops one
/// value per entry.
#[derive(Debug)]
pub(crate) struct ObjectShape {
    pub(crate) props: Vec<ShapeProperty>,
    /// Every entry is a plain data property under its own key, so a fresh
    /// object can take them without checking for earlier definitions: no
    /// accessors, methods, `__proto__` setters or repeated keys.
    pub(crate) distinct_data: bool,
}

/// `ExecAst` operand meaning "no break/continue can leave this statement".
pub(crate) const NO_ESCAPE_TABLE: u16 = u16::MAX;

/// `ExecAst` flag bit: the statement runs inside a `try` block (or a `catch`
/// whose `finally` still has to run), so a call in tail position inside it
/// must not become a tail call (sec-static-semantics-hascallintailposition).
pub(crate) const EXEC_NO_TAIL_CALL: u8 = 1;

/// `ExecAst` flag bit (Script goal): the statement's completion value is a
/// candidate for the script's completion value, which the VM keeps in its
/// accumulator. A `Normal` value replaces it, `Empty` leaves it, and a
/// `break`/`continue` carrying a value updates it before the jump
/// (UpdateEmpty, sec-updateempty).
pub(crate) const EXEC_SET_COMPLETION: u8 = 2;

/// A protected range of a chunk: a `Throw` raised by an op in
/// `[start, end)` resumes at `target` with the operand stack cut back to
/// `stack_height` values and the block environments to `scope_depth`, the
/// thrown value pushed on top. Entries are ordered innermost first.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Handler {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) target: u32,
    pub(crate) stack_height: u16,
    pub(crate) scope_depth: u16,
    /// A `try` handler: a latched `__host_exit` makes the exit uncatchable
    /// instead (the tree-walker's `exec_try` does the same).
    pub(crate) checks_exit: bool,
}

/// Where `EndFinally` resumes for a pending break/continue/return the
/// finalizer was entered with.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FinallyExit {
    pub(crate) target: u32,
    /// The pending completion's value (a return value) goes back on the
    /// operand stack at `target`.
    pub(crate) keep_value: bool,
}

/// How `BindHead` stores an iteration value in a for-in/for-of head the
/// native binding ops do not cover (sec-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset).
#[derive(Debug, Clone)]
pub(crate) enum HeadBinding {
    /// `let`/`const`/`var` with a destructuring pattern.
    Declare { pattern: Pattern, kind: BindingKind },
    /// An assignment target pattern: `[a, b]`, `{x: o.p}`, `o.p`.
    Assign { pattern: Pattern },
}

#[derive(Debug, Default)]
pub(crate) struct Chunk {
    pub(crate) code: Vec<u8>,
    pub(crate) constants: Vec<Constant>,
    pub(crate) names: Vec<Atom>,
    /// Source spellings (without the `#`) of the private names the chunk
    /// reads; the VM resolves each to its Private Name once per frame.
    pub(crate) private_names: Vec<String>,
    /// One remembered global-object property slot per `names` entry, consulted
    /// by the name ops (`LoadName`, `ResolveName`, ...). Verified by key on
    /// every use, so it is never an invalidation concern. May be shorter than
    /// `names` (hand-built chunks): a missing entry means no hint.
    pub(crate) global_hints: Vec<GlobalHint>,
    /// One remembered property-map slot per `names` entry, for the named
    /// property ops (`GetProp`, `SetProp`, ...). Verified by key identity on
    /// every use. May be shorter than `names`: a missing entry means no hint.
    pub(crate) prop_hints: Vec<Cell<u32>>,
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
    /// Set when that instantiation is only declaring these top-level
    /// `let`/`const`/class bindings (`names` index, immutable) in TDZ; the
    /// call path then skips the tree-walker's pass (#1052).
    pub(crate) top_level_lexicals: Option<Vec<(u16, bool)>>,
    pub(crate) handlers: Vec<Handler>,
    pub(crate) finally_tables: Vec<Vec<FinallyExit>>,
    pub(crate) head_bindings: Vec<HeadBinding>,
    /// Most operand-stack slots the chunk keeps live across statement
    /// boundaries (an iterator, a pending completion record).
    pub(crate) persistent_slots: u16,
    /// Why each fallback node could not compile natively, one entry per node.
    #[cfg_attr(not(feature = "perf-counters"), allow(dead_code))]
    pub(crate) fallback_reasons: Vec<&'static str>,
    /// The body is exactly `return this.#name;`: the `private_names` index it
    /// reads. A getter with this shape is served without a frame (see
    /// `Interpreter::call_getter`).
    pub(crate) trivial_private_getter: Option<u16>,
}

impl Chunk {
    /// Recognise the `return this.#name;` body: an optional `Safepoint`, then
    /// `LoadThis`, `GetPrivate`, `Return`, with at most a trailing
    /// `ReturnUndefined` after it. Nothing else may be present, so no
    /// handler, hand-off or declaration needs a frame.
    pub(crate) fn detect_trivial_private_getter(&self) -> Option<u16> {
        use super::op::Op;
        if !self.ast.is_empty() || !self.handlers.is_empty() || self.needs_instantiation {
            return None;
        }
        let mut code = self.code.as_slice();
        if let [first, rest @ ..] = code
            && *first == Op::Safepoint as u8
        {
            code = rest;
        }
        match code {
            [load_this, get, lo, hi, ret, tail @ ..]
                if *load_this == Op::LoadThis as u8
                    && *get == Op::GetPrivate as u8
                    && *ret == Op::Return as u8
                    && (tail.is_empty() || tail == [Op::ReturnUndefined as u8]) =>
            {
                let idx = u16::from_le_bytes([*lo, *hi]);
                (usize::from(idx) < self.private_names.len()).then_some(idx)
            }
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn prop_hint(&self, name_idx: u16) -> Option<&Cell<u32>> {
        self.prop_hints.get(usize::from(name_idx))
    }

    #[inline]
    pub(crate) fn global_hint(&self, name_idx: u16) -> Option<&GlobalHint> {
        self.global_hints.get(usize::from(name_idx))
    }
}
