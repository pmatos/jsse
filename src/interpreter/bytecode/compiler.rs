use super::chunk::{
    ArrayTemplate, AstNode, BLOCK_SLOT_FLAG, BLOCK_SLOT_MAX_INDEX, BLOCK_SLOT_MAX_SCOPES,
    BlockScope, Chunk, Constant, EXEC_NO_TAIL_CALL, EXEC_SET_COMPLETION, EscapeEntry, FinallyExit,
    FunctionTemplate, Handler, HeadBinding, NO_ESCAPE_TABLE, NO_INIT_SLOT, ObjectShape,
    RegExpTemplate, ShapeProperty,
};
use super::op::{Op, UPDATE_DISCARD};
use crate::ast::{
    AssignOp, BinaryOp, CallSiteId, CatchClause, DeclPhase, DoWhileStatement, Expression,
    ForInOfLeft, ForInit, ForStatement, Literal, LogicalOp, MemberProperty, Pattern, Property,
    PropertyKey, PropertyKind, Statement, SwitchStatement, TryStatement, UnaryOp, UpdateOp,
    VarKind, VariableDeclaration, WhileStatement,
};
use crate::interpreter::GlobalHint;
use crate::interpreter::eval::LiteralPropertySpec;
use crate::interpreter::hoisting::block_declares_lexical;
use crate::interpreter::types::BindingKind;
use crate::types::{JsString, JsValue};

/// Operand values one `NewArray`/`ArrayAppend`/`NewObject`/`ObjectAppend`
/// keeps on the operand stack at once: larger literals are built in chunks.
const LITERAL_VALUE_CHUNK: usize = 256;

#[derive(Debug)]
pub(crate) enum CompileError {
    /// This node has no native lowering. In either goal the node itself is
    /// handed to the tree-walker (`EvalAst`/`ExecAst`). The string carries a
    /// construction-site label for greppable documentation, `{:?}` traces
    /// when debugging the eligibility membrane, and the counters build's
    /// BAIL/FALLBACK tables.
    Unsupported(&'static str),
    /// The whole body must stay on the tree-walker: a construct whose
    /// semantics depend on running as one tree-walked frame (direct `eval`,
    /// `with`, `super`, ...), or a compiler resource limit.
    Ineligible(&'static str),
    /// An `i16` jump offset cannot reach its target. The statement that owns
    /// the jump is handed to the tree-walker instead of failing the whole body.
    JumpOverflow,
}

impl CompileError {
    pub(crate) fn reason(&self) -> &'static str {
        match self {
            CompileError::Unsupported(reason) | CompileError::Ineligible(reason) => reason,
            CompileError::JumpOverflow => "jump offset overflow",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CompileGoal {
    Function,
    Script,
}

/// Tracks one in-progress loop so `break`/`continue` can
/// resolve their target without re-walking the AST. Stacked labels on one
/// loop (`a: b: while (...) {}`) collect into a single frame's `labels`,
/// mirroring LabelledEvaluation's label-set semantics (`sec-runtime-semantics-labelledevaluation`)
/// rather than nesting one frame per label.
struct LoopFrame {
    labels: Vec<String>,
    /// `while`'s continue target (`loop_start`) is known before the body
    /// compiles, so it starts `Some` and `continue` lowers to a direct
    /// backward jump. `for` and `do-while` continue targets follow their
    /// bodies, so they start `None` and defer through `continue_sites`.
    continue_target: Option<usize>,
    continue_sites: Vec<usize>,
    /// The position after the loop is never known while compiling the body,
    /// in either loop kind, so `break` always defers through here.
    break_sites: Vec<usize>,
    /// Escape-table entries (table, entry) whose targets are this loop's
    /// break position / continue position, resolved when the loop ends.
    escape_breaks: Vec<(usize, usize)>,
    escape_continues: Vec<(usize, usize)>,
    /// A `switch` is a `break` target but not a `continue` target.
    is_switch: bool,
    /// Block environments open at this frame's break/continue targets (the
    /// loop head's own environment included). A jump from deeper inside
    /// leaves the difference first.
    scope_depth: usize,
    /// Cleanup entries open at this frame's targets; a jump from deeper
    /// inside runs the entries above this depth on its way out.
    cleanup_depth: usize,
    /// Persistent operand-stack slots live at the targets.
    stack_height: u16,
}

impl LoopFrame {
    fn new(
        labels: Vec<String>,
        continue_target: Option<usize>,
        scope_depth: usize,
        cleanup_depth: usize,
        stack_height: u16,
    ) -> Self {
        Self {
            labels,
            continue_target,
            scope_depth,
            cleanup_depth,
            stack_height,
            continue_sites: Vec::new(),
            break_sites: Vec::new(),
            escape_breaks: Vec::new(),
            escape_continues: Vec::new(),
            is_switch: false,
        }
    }
}

/// Compiler state saved before compiling a node, so a node that turns out to
/// need the tree-walker can be rolled back cleanly and emitted as one
/// `EvalAst`/`ExecAst` instead.
struct Snapshot {
    code_len: usize,
    last_op_at: usize,
    current_stack: u16,
    current_refs: u16,
    frames: Vec<(usize, usize, usize, usize)>,
    ast_len: usize,
    tables_len: usize,
    native_ops: u32,
    scopes_len: usize,
    block_scopes_len: usize,
    functions_len: usize,
    array_templates_len: usize,
    object_shapes_len: usize,
    regexps_len: usize,
    cleanups_len: usize,
    finally_ctxs: Vec<(usize, usize)>,
    handlers_len: usize,
    finally_tables_len: usize,
    head_bindings_len: usize,
    stack_base: u16,
    no_tail: u32,
    unwind_segments_len: usize,
}

#[derive(Default)]
struct ScopeDecls {
    functions: Vec<Statement>,
    function_names: Vec<String>,
    lexicals: Vec<(String, bool)>,
    simple_catch: bool,
}

/// Something a `break`, `continue` or `return` leaving a construct has to do
/// on its way out, innermost last.
#[derive(Clone, Copy)]
enum Cleanup {
    /// A for-in/for-of loop's iterator, one persistent operand slot. A for-of
    /// closes it (`close`); for-in enumerators are simply dropped.
    Iter { close: bool, scope_depth: usize },
    /// The pending completion record (value, kind) of a running `finally`
    /// block: two persistent slots to drop.
    Record { scope_depth: usize },
    /// The `try` block or `catch` of a `try`/`finally`: leaving it for
    /// anywhere else runs the finalizer first.
    Finally { ctx: usize, scope_depth: usize },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Terminal {
    Return,
    /// Index into `loop_frames`.
    Break(usize),
    Continue(usize),
}

/// The exits routed through one `try`/`finally`'s finalizer: entry `i` is the
/// completion kind `i + 2` (0 is normal, 1 a throw).
#[derive(Default)]
struct FinallyCtx {
    routes: Vec<(Terminal, bool)>,
    /// Jumps into the finalizer, patched to its start.
    jump_sites: Vec<usize>,
}

/// A `break`/`continue`/`return` an `ExecAst` statement can complete with.
enum EscapingJump {
    Return,
    Break(Option<String>),
    Continue(Option<String>),
}

/// Name pools up to this size are searched linearly; a larger one gets an index.
const NAME_SCAN_LIMIT: usize = 16;

type NameIndex = std::collections::HashMap<
    crate::interpreter::Atom,
    u16,
    crate::interpreter::atom::IdentityBuild,
>;

struct Compiler {
    goal: CompileGoal,
    code: Vec<u8>,
    constants: Vec<Constant>,
    names: Vec<crate::interpreter::Atom>,
    name_index: Option<NameIndex>,
    private_names: Vec<String>,
    var_names: Vec<u16>,
    current_stack: u16,
    max_stack: u16,
    current_refs: u16,
    max_refs: u16,
    /// Highest byte offset targeted by any patched forward jump. When this
    /// equals the final code length, some branch falls through to the very end
    /// of the chunk, so `finish` must append a trailing `ReturnUndefined` even
    /// when the last *emitted* opcode is a `Return` — otherwise that branch
    /// runs `pc` off the end of `code` and panics in the VM dispatch loop. The
    /// motivating case is a one-armed `if` whose consequent ends in `return`.
    max_jump_target: usize,
    /// Innermost loop last. `break`/`continue` resolution for the unlabeled
    /// form reads the last entry; the labeled form searches outward from the
    /// end via `resolve_break_frame`/`resolve_continue_frame`.
    loop_frames: Vec<LoopFrame>,
    /// Frame binding order for a function body whose locals are known: slot
    /// `i` is the expected position of that name in the call's binding
    /// vector. Empty when no local ops are emitted.
    locals: Vec<String>,
    /// Offset of the most recently emitted opcode byte. The last byte of
    /// `code` may be an operand (a slot or name index whose high byte equals a
    /// return opcode), so `finish` must not decode `code.last()`.
    last_op_at: usize,
    ast: Vec<AstNode>,
    escape_tables: Vec<Vec<EscapeEntry>>,
    /// Ops other than bookkeeping and tree-walker hand-offs. A chunk with none
    /// would pay chunk-entry cost for nothing.
    native_ops: u32,
    fallback_reasons: Vec<&'static str>,
    /// Names declared by each open block environment, outermost first. A name
    /// found here shadows the frame's `locals`, and its position is the slot
    /// hint `PushEnv`'s declaration order produces.
    scopes: Vec<Vec<String>>,
    block_scopes: Vec<BlockScope>,
    functions: Vec<FunctionTemplate>,
    array_templates: Vec<ArrayTemplate>,
    object_shapes: Vec<ObjectShape>,
    regexps: Vec<RegExpTemplate>,
    /// Set by the function top-level statement list for the statement about
    /// to be compiled; consumed on entry so nested statements see `false`.
    /// Only there may a `let`/`const` initialise its hoisted binding directly.
    at_function_top_level: bool,
    cleanups: Vec<Cleanup>,
    finally_ctxs: Vec<FinallyCtx>,
    handlers: Vec<Handler>,
    finally_tables: Vec<Vec<FinallyExit>>,
    head_bindings: Vec<HeadBinding>,
    /// Operand-stack slots live across statement boundaries here (iterators,
    /// completion records); `current_stack` equals this between statements.
    stack_base: u16,
    max_persistent: u16,
    /// Open `try` blocks (and `catch` blocks with a `finally`): a call here is
    /// never in tail position.
    no_tail: u32,
    /// Constant-pool index of each finalizer completion kind used so far.
    kind_constants: Vec<(u16, u16)>,
    /// Code ranges `[start, end)` of the `IterClose` an exit emits for the
    /// cleanup at the given index. An exit is logically a completion leaving
    /// the construct, so a `return()` error it raises belongs to the code
    /// *around* that construct, and handlers of the constructs it just left
    /// must not see it (see `push_handler`).
    unwind_segments: Vec<(usize, usize, usize)>,
    /// Compiling a generator body: `yield` lowers to `Op::Yield`, and a node
    /// handed to the tree-walker must not contain one (it could not suspend).
    generator: bool,
}

impl Compiler {
    fn new(goal: CompileGoal) -> Self {
        Self {
            goal,
            generator: false,
            locals: Vec::new(),
            last_op_at: 0,
            code: Vec::new(),
            constants: Vec::new(),
            names: Vec::new(),
            name_index: None,
            private_names: Vec::new(),
            var_names: Vec::new(),
            current_stack: 0,
            max_stack: 0,
            current_refs: 0,
            max_refs: 0,
            max_jump_target: 0,
            loop_frames: Vec::new(),
            ast: Vec::new(),
            escape_tables: Vec::new(),
            native_ops: 0,
            fallback_reasons: Vec::new(),
            scopes: Vec::new(),
            block_scopes: Vec::new(),
            functions: Vec::new(),
            array_templates: Vec::new(),
            object_shapes: Vec::new(),
            regexps: Vec::new(),
            at_function_top_level: false,
            cleanups: Vec::new(),
            finally_ctxs: Vec::new(),
            handlers: Vec::new(),
            finally_tables: Vec::new(),
            head_bindings: Vec::new(),
            stack_base: 0,
            max_persistent: 0,
            no_tail: 0,
            kind_constants: Vec::new(),
            unwind_segments: Vec::new(),
        }
    }

    fn new_generator() -> Self {
        let mut c = Self::new(CompileGoal::Function);
        c.generator = true;
        // A generator body has no tail position (sec-isintailposition).
        c.no_tail = 1;
        c
    }

    fn add_constant(&mut self, c: Constant) -> Result<u16, CompileError> {
        let idx = self.constants.len();
        if idx > u16::MAX as usize {
            return Err(CompileError::Ineligible("constant pool overflow"));
        }
        self.constants.push(c);
        Ok(idx as u16)
    }

    // `Atom` hashes and compares by identity, never through its `OnceCell`.
    #[allow(clippy::mutable_key_type)]
    fn add_name(&mut self, name: &str) -> Result<u16, CompileError> {
        if let Some(index) = &self.name_index {
            let atom = crate::interpreter::Atom::intern(name);
            if let Some(&i) = index.get(&atom) {
                return Ok(i);
            }
            return self.push_name(atom);
        }
        if let Some(i) = self.names.iter().position(|n| n.as_str() == name) {
            return Ok(i as u16);
        }
        self.push_name(crate::interpreter::Atom::intern(name))
    }

    #[allow(clippy::mutable_key_type)]
    fn push_name(&mut self, atom: crate::interpreter::Atom) -> Result<u16, CompileError> {
        let idx = self.names.len();
        if idx > u16::MAX as usize {
            return Err(CompileError::Ineligible("name pool overflow"));
        }
        if self.name_index.is_none() && idx >= NAME_SCAN_LIMIT {
            self.name_index = Some(
                self.names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (n.clone(), i as u16))
                    .collect(),
            );
        }
        if let Some(index) = &mut self.name_index {
            index.insert(atom.clone(), idx as u16);
        }
        self.names.push(atom);
        Ok(idx as u16)
    }

    /// Index of a private name's source spelling in the chunk's private-name
    /// pool. The VM resolves each entry to its Private Name once per frame.
    fn add_private_name(&mut self, name: &str) -> Result<u16, CompileError> {
        if let Some(i) = self.private_names.iter().position(|n| n == name) {
            return Ok(i as u16);
        }
        let idx = self.private_names.len();
        if idx > u16::MAX as usize {
            return Err(CompileError::Ineligible("private name pool overflow"));
        }
        self.private_names.push(name.to_string());
        Ok(idx as u16)
    }

    fn emit(&mut self, op: Op) {
        self.last_op_at = self.code.len();
        if !matches!(
            op,
            Op::Safepoint
                | Op::EvalAst
                | Op::ExecAst
                | Op::Pop
                | Op::Jump
                | Op::Return
                | Op::ReturnUndefined
                | Op::PushEnv
                | Op::PopEnv
                | Op::CopyIterEnv
                | Op::CheckExit
                | Op::DropBelow
        ) {
            self.native_ops += 1;
        }
        self.code.push(op as u8);
    }

    fn emit_u16(&mut self, n: u16) {
        self.code.push((n & 0xff) as u8);
        self.code.push((n >> 8) as u8);
    }

    fn emit_u32(&mut self, n: u32) {
        self.code.extend_from_slice(&n.to_le_bytes());
    }

    /// Emit a forward jump with a placeholder offset; returns the patch site
    /// (offset of the first operand byte). Caller must invoke `patch_jump`
    /// once the target instruction has been emitted.
    fn emit_jump(&mut self, op: Op) -> usize {
        self.emit(op);
        let patch = self.code.len();
        self.code.push(0);
        self.code.push(0);
        patch
    }

    fn emit_jump_to(&mut self, op: Op, target: usize) -> Result<(), CompileError> {
        self.emit(op);
        let from = self.code.len() + 2;
        let delta = target as isize - from as isize;
        if !(i16::MIN as isize..=i16::MAX as isize).contains(&delta) {
            return Err(CompileError::JumpOverflow);
        }
        self.emit_u16(delta as i16 as u16);
        Ok(())
    }

    fn patch_jump(&mut self, patch: usize) -> Result<(), CompileError> {
        self.patch_jump_to(patch, self.code.len())
    }

    fn patch_jump_to(&mut self, patch: usize, to: usize) -> Result<(), CompileError> {
        // Offset is from the byte AFTER the 2-byte operand to the target.
        let from = patch + 2;
        self.max_jump_target = self.max_jump_target.max(to);
        let delta = to as isize - from as isize;
        if !(i16::MIN as isize..=i16::MAX as isize).contains(&delta) {
            return Err(CompileError::JumpOverflow);
        }
        let off = delta as i16 as u16;
        self.code[patch] = (off & 0xff) as u8;
        self.code[patch + 1] = (off >> 8) as u8;
        Ok(())
    }

    fn push_n(&mut self, n: u16) {
        self.current_stack += n;
        if self.current_stack > self.max_stack {
            self.max_stack = self.current_stack;
        }
    }

    fn pop_n(&mut self, n: u16) {
        debug_assert!(self.current_stack >= n, "stack underflow during compile");
        self.current_stack -= n;
    }

    fn push_ref(&mut self) {
        self.current_refs += 1;
        self.max_refs = self.max_refs.max(self.current_refs);
    }

    fn pop_ref(&mut self) {
        debug_assert!(
            self.current_refs > 0,
            "reference stack underflow during compile"
        );
        self.current_refs -= 1;
    }

    fn emit_resolve_name(&mut self, name_idx: u16) {
        self.emit(Op::ResolveName);
        self.emit_u16(name_idx);
        self.push_ref();
    }

    /// Stores the top of stack through the reference `ResolveName` pushed;
    /// `discard` also pops the value.
    fn emit_store_resolved_name(&mut self, name_idx: u16, discard: bool) {
        self.emit(if discard {
            Op::StoreResolvedNamePop
        } else {
            Op::StoreResolvedName
        });
        self.emit_u16(name_idx);
        self.pop_ref();
        if discard {
            self.pop_n(1);
        }
    }

    fn emit_dup_n(&mut self, count: u8) {
        self.emit(Op::DupN);
        self.code.push(count);
        self.push_n(u16::from(count));
    }

    fn emit_load_name(&mut self, name_idx: u16) {
        self.emit(Op::LoadName);
        self.emit_u16(name_idx);
        self.push_n(1);
    }

    fn compile_load_name(&mut self, name: &str) -> Result<(), CompileError> {
        let idx = self.add_name(name)?;
        if let Some(slot) = self.local_slot(name) {
            self.emit(Op::LoadLocal);
            self.emit_u16(idx);
            self.emit_u16(slot);
            self.push_n(1);
        } else {
            self.emit_load_name(idx);
        }
        Ok(())
    }

    /// The slot operand for a frame or block-environment binding of `name`
    /// the local ops can address, nearest scope first. A name an open block
    /// environment declares never resolves to the frame's, and one that does
    /// not fit the operand encoding gets no hint (the by-name path finds it).
    fn local_slot(&self, name: &str) -> Option<u16> {
        for (scope, names) in self.scopes.iter().enumerate().rev() {
            if let Some(index) = names.iter().position(|n| n == name) {
                return (scope < BLOCK_SLOT_MAX_SCOPES && index < BLOCK_SLOT_MAX_INDEX)
                    .then_some(BLOCK_SLOT_FLAG | ((scope as u16) << 8) | index as u16);
            }
        }
        self.locals
            .iter()
            .position(|n| n == name)
            .and_then(|i| u16::try_from(i).ok())
            .filter(|slot| slot & BLOCK_SLOT_FLAG == 0)
    }

    /// Stores the top of stack into a frame local; `discard` also pops it.
    fn emit_store_local(&mut self, name_idx: u16, slot: u16, discard: bool) {
        self.emit(if discard {
            Op::StoreLocalPop
        } else {
            Op::StoreLocal
        });
        self.emit_u16(name_idx);
        self.emit_u16(slot);
        if discard {
            self.pop_n(1);
        }
    }

    /// `[..., l]` → `[..., l op k]`.
    fn emit_bin_const(&mut self, bop: Op, k: f64) -> Result<(), CompileError> {
        let k_idx = self.add_constant(Constant::Number(k))?;
        self.emit(Op::BinConst);
        self.code.push(bop as u8);
        self.emit_u16(k_idx);
        Ok(())
    }

    /// `LoadLocal name` then `BinConst`, in one op: pushes `name op k`.
    fn emit_local_bin_const(
        &mut self,
        name: &str,
        slot: u16,
        bop: Op,
        k: f64,
    ) -> Result<(), CompileError> {
        let idx = self.add_name(name)?;
        let k_idx = self.add_constant(Constant::Number(k))?;
        self.emit(Op::LocalBinConst);
        self.emit_u16(idx);
        self.emit_u16(slot);
        self.code.push(bop as u8);
        self.emit_u16(k_idx);
        self.push_n(1);
        Ok(())
    }

    fn reset_script_completion(&mut self) {
        if self.goal == CompileGoal::Script {
            self.emit(Op::LoadUndefined);
            self.push_n(1);
            self.emit(Op::SetCompletion);
            self.pop_n(1);
        }
    }

    fn add_var_name(&mut self, name: &str) -> Result<u16, CompileError> {
        let idx = self.add_name(name)?;
        if !self.var_names.contains(&idx) {
            self.var_names.push(idx);
        }
        Ok(idx)
    }

    fn binary_op(op: BinaryOp) -> Result<Op, CompileError> {
        match op {
            BinaryOp::Add => Ok(Op::Add),
            BinaryOp::Sub => Ok(Op::Sub),
            BinaryOp::Mul => Ok(Op::Mul),
            BinaryOp::Div => Ok(Op::Div),
            BinaryOp::Mod => Ok(Op::Mod),
            BinaryOp::Exp => Ok(Op::Pow),
            BinaryOp::Eq => Ok(Op::Eq),
            BinaryOp::NotEq => Ok(Op::NotEq),
            BinaryOp::StrictEq => Ok(Op::StrictEq),
            BinaryOp::StrictNotEq => Ok(Op::StrictNotEq),
            BinaryOp::Lt => Ok(Op::Lt),
            BinaryOp::Gt => Ok(Op::Gt),
            BinaryOp::LtEq => Ok(Op::LtEq),
            BinaryOp::GtEq => Ok(Op::GtEq),
            BinaryOp::BitAnd => Ok(Op::BitAnd),
            BinaryOp::BitOr => Ok(Op::BitOr),
            BinaryOp::BitXor => Ok(Op::BitXor),
            BinaryOp::LShift => Ok(Op::Shl),
            BinaryOp::RShift => Ok(Op::Shr),
            BinaryOp::URShift => Ok(Op::UShr),
            BinaryOp::Instanceof => Ok(Op::Instanceof),
            BinaryOp::In => Ok(Op::In),
        }
    }

    fn compound_binary_op(op: AssignOp) -> Result<Op, CompileError> {
        let binary = op
            .binary_op()
            .ok_or(CompileError::Unsupported("assignment op"))?;
        Self::binary_op(binary)
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            code_len: self.code.len(),
            last_op_at: self.last_op_at,
            current_stack: self.current_stack,
            current_refs: self.current_refs,
            frames: self
                .loop_frames
                .iter()
                .map(|f| {
                    (
                        f.break_sites.len(),
                        f.continue_sites.len(),
                        f.escape_breaks.len(),
                        f.escape_continues.len(),
                    )
                })
                .collect(),
            ast_len: self.ast.len(),
            tables_len: self.escape_tables.len(),
            native_ops: self.native_ops,
            scopes_len: self.scopes.len(),
            block_scopes_len: self.block_scopes.len(),
            functions_len: self.functions.len(),
            array_templates_len: self.array_templates.len(),
            object_shapes_len: self.object_shapes.len(),
            regexps_len: self.regexps.len(),
            cleanups_len: self.cleanups.len(),
            finally_ctxs: self
                .finally_ctxs
                .iter()
                .map(|c| (c.routes.len(), c.jump_sites.len()))
                .collect(),
            handlers_len: self.handlers.len(),
            finally_tables_len: self.finally_tables.len(),
            head_bindings_len: self.head_bindings.len(),
            stack_base: self.stack_base,
            no_tail: self.no_tail,
            unwind_segments_len: self.unwind_segments.len(),
        }
    }

    fn rollback(&mut self, snap: Snapshot) {
        self.code.truncate(snap.code_len);
        self.last_op_at = snap.last_op_at;
        self.current_stack = snap.current_stack;
        self.current_refs = snap.current_refs;
        self.loop_frames.truncate(snap.frames.len());
        for (frame, &(breaks, continues, esc_breaks, esc_continues)) in
            self.loop_frames.iter_mut().zip(snap.frames.iter())
        {
            frame.break_sites.truncate(breaks);
            frame.continue_sites.truncate(continues);
            frame.escape_breaks.truncate(esc_breaks);
            frame.escape_continues.truncate(esc_continues);
        }
        self.ast.truncate(snap.ast_len);
        self.escape_tables.truncate(snap.tables_len);
        self.native_ops = snap.native_ops;
        self.fallback_reasons.truncate(snap.ast_len);
        self.scopes.truncate(snap.scopes_len);
        self.block_scopes.truncate(snap.block_scopes_len);
        self.functions.truncate(snap.functions_len);
        self.array_templates.truncate(snap.array_templates_len);
        self.object_shapes.truncate(snap.object_shapes_len);
        self.regexps.truncate(snap.regexps_len);
        self.cleanups.truncate(snap.cleanups_len);
        self.finally_ctxs.truncate(snap.finally_ctxs.len());
        for (ctx, &(routes, sites)) in self.finally_ctxs.iter_mut().zip(&snap.finally_ctxs) {
            ctx.routes.truncate(routes);
            ctx.jump_sites.truncate(sites);
        }
        self.handlers.truncate(snap.handlers_len);
        self.finally_tables.truncate(snap.finally_tables_len);
        self.head_bindings.truncate(snap.head_bindings_len);
        self.stack_base = snap.stack_base;
        self.no_tail = snap.no_tail;
        self.unwind_segments.truncate(snap.unwind_segments_len);
    }

    fn add_ast_node(&mut self, node: AstNode, reason: &'static str) -> Result<u16, CompileError> {
        if self.generator && node_contains_yield(&node) {
            return Err(CompileError::Ineligible("yield in tree-walked node"));
        }
        let idx = self.ast.len();
        if idx >= usize::from(NO_ESCAPE_TABLE) {
            return Err(CompileError::Ineligible("ast node pool overflow"));
        }
        self.ast.push(node);
        self.fallback_reasons.push(reason);
        Ok(idx as u16)
    }

    /// Compiles `expr`; when it has no native lowering (and can be evaluated
    /// standalone), the whole node is handed to the tree-walker instead.
    fn compile_expr(&mut self, expr: &Expression) -> Result<(), CompileError> {
        let snap = self.snapshot();
        match self.compile_expr_native(expr) {
            Err(CompileError::Unsupported(reason)) if is_standalone_expression(expr) => {
                self.rollback(snap);
                let idx = self.add_ast_node(AstNode::Expr(expr.clone()), reason)?;
                self.emit(Op::EvalAst);
                self.emit_u16(idx);
                self.push_n(1);
                Ok(())
            }
            other => other,
        }
    }

    fn compile_expr_native(&mut self, expr: &Expression) -> Result<(), CompileError> {
        match expr {
            Expression::Literal(lit) => self.compile_literal(lit),
            Expression::This => {
                self.emit(Op::LoadThis);
                self.push_n(1);
                Ok(())
            }
            Expression::Identifier(name) => self.compile_load_name(name),
            Expression::Unary(op, operand) => {
                self.compile_expr(operand)?;
                let bop = match op {
                    UnaryOp::Minus => Op::Neg,
                    UnaryOp::Plus => Op::Plus,
                    UnaryOp::Not => Op::Not,
                    UnaryOp::BitNot => Op::BitNot,
                };
                self.emit(bop);
                // Stack height unchanged: pop one, push one.
                Ok(())
            }
            Expression::Typeof(operand) => {
                match operand.as_ref() {
                    Expression::Identifier(name) if self.local_slot(name).is_none() => {
                        let idx = self.add_name(name)?;
                        self.emit(Op::TypeofName);
                        self.emit_u16(idx);
                        self.push_n(1);
                    }
                    // A frame-local name always exists, so reading it cannot
                    // be the unresolvable-reference case `TypeofName` covers.
                    other => {
                        self.compile_expr(other)?;
                        self.emit(Op::Typeof);
                    }
                }
                Ok(())
            }
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                if exprs.is_empty() {
                    return Err(CompileError::Unsupported("empty sequence"));
                }
                let last = exprs.len() - 1;
                for (i, e) in exprs.iter().enumerate() {
                    if i == last {
                        self.compile_expr(e)?;
                    } else {
                        self.compile_effect(e)?;
                    }
                }
                Ok(())
            }
            // NamedEvaluation (sec-runtime-semantics-namedevaluation) is applied
            // by the assignment, not by the function expression. A function
            // or arrow literal is named as its closure is created; an
            // anonymous class needs the tree-walker's logic.
            Expression::Assign(_, target, value)
                if matches!(target.as_ref(), Expression::Identifier(_))
                    && is_anonymous_class(value) =>
            {
                Err(CompileError::Unsupported("anonymous class assignment"))
            }
            Expression::Assign(op, target, value) => match target.as_ref() {
                Expression::Identifier(name) => {
                    self.compile_identifier_assign(*op, name, value, false)
                }
                Expression::Member(obj, prop, _) => match prop {
                    MemberProperty::Dot(name) => {
                        let idx = self.add_name(name)?;
                        if *op == AssignOp::Assign {
                            self.compile_expr(obj)?;
                            self.compile_expr(value)?;
                            self.emit(Op::SetProp);
                            self.emit_u16(idx);
                            self.pop_n(2);
                            self.push_n(1);
                        } else {
                            // lVal ← base.name; rVal ← value; SetProp(base, name, op(lVal, rVal)).
                            // The base is duplicated so GetProp's pop/SetProp's pop each
                            // consume their own copy — the base is evaluated exactly once.
                            self.compile_expr(obj)?;
                            self.emit_dup_n(1);
                            self.emit(Op::GetProp);
                            self.emit_u16(idx);
                            self.compile_expr(value)?;
                            self.emit(Self::compound_binary_op(*op)?);
                            self.pop_n(2);
                            self.push_n(1);
                            self.emit(Op::SetProp);
                            self.emit_u16(idx);
                            self.pop_n(2);
                            self.push_n(1);
                        }
                        Ok(())
                    }
                    MemberProperty::Computed(key) => {
                        self.compile_expr(obj)?;
                        self.compile_expr(key)?;
                        if *op == AssignOp::Assign {
                            self.compile_expr(value)?;
                            self.emit(Op::SetElement);
                            self.pop_n(3);
                            self.push_n(1);
                        } else {
                            // ToPrimitiveKey performs ToPropertyKey's only side-effecting
                            // step exactly once, in the spec's required order (base-nullish
                            // check before key coercion); base and key are then duplicated
                            // so GetElement's pop and SetElement's pop each consume their
                            // own copy.
                            self.emit(Op::ToPrimitiveKey);
                            self.emit_dup_n(2);
                            self.emit(Op::GetElement);
                            self.pop_n(2);
                            self.push_n(1);
                            self.compile_expr(value)?;
                            self.emit(Self::compound_binary_op(*op)?);
                            self.pop_n(2);
                            self.push_n(1);
                            self.emit(Op::SetElement);
                            self.pop_n(3);
                            self.push_n(1);
                        }
                        Ok(())
                    }
                    MemberProperty::Private(name) => {
                        let idx = self.add_private_name(name)?;
                        if *op == AssignOp::Assign {
                            self.compile_expr(obj)?;
                            self.compile_expr(value)?;
                            self.emit(Op::SetPrivate);
                            self.emit_u16(idx);
                            self.pop_n(2);
                            self.push_n(1);
                        } else {
                            // PrivateGet -> rVal -> op -> PrivateSet, the base
                            // evaluated once (as for `base.name op= value`).
                            self.compile_expr(obj)?;
                            self.emit_dup_n(1);
                            self.emit(Op::GetPrivate);
                            self.emit_u16(idx);
                            self.compile_expr(value)?;
                            self.emit(Self::compound_binary_op(*op)?);
                            self.pop_n(2);
                            self.push_n(1);
                            self.emit(Op::SetPrivate);
                            self.emit_u16(idx);
                            self.pop_n(2);
                            self.push_n(1);
                        }
                        Ok(())
                    }
                },
                _ => Err(CompileError::Unsupported("assign target")),
            },
            Expression::Update(op, prefix, target) => {
                self.compile_update(*op, *prefix, target, false)
            }
            Expression::Logical(op, lhs, rhs) => {
                self.compile_expr(lhs)?;
                // lhs stays on stack via *Keep variants
                let to_end = self.emit_jump(Self::logical_keep_jump(*op));
                // Short-circuit not taken: drop lhs and use rhs
                self.emit(Op::Pop);
                self.pop_n(1);
                self.compile_expr(rhs)?;
                self.pop_n(1);
                self.patch_jump(to_end)?;
                self.push_n(1);
                Ok(())
            }
            Expression::Conditional(test, then_e, else_e) => {
                let to_else = self.compile_cond_jump(test)?;
                self.compile_expr(then_e)?;
                self.pop_n(1); // value will be re-pushed at the merge point
                let to_end = self.emit_jump(Op::Jump);
                self.patch_jump(to_else)?;
                self.compile_expr(else_e)?;
                self.pop_n(1); // same — re-pushed at merge
                self.patch_jump(to_end)?;
                self.push_n(1);
                Ok(())
            }
            Expression::Void(operand) => {
                // Evaluate for side effects, discard, push undefined.
                self.compile_effect(operand)?;
                self.emit(Op::LoadUndefined);
                self.push_n(1);
                Ok(())
            }
            Expression::Binary(BinaryOp::In, lhs, rhs)
                if let Expression::PrivateIdentifier(name) = lhs.as_ref() =>
            {
                self.compile_expr(rhs)?;
                let idx = self.add_private_name(name)?;
                self.emit(Op::PrivateIn);
                self.emit_u16(idx);
                Ok(())
            }
            Expression::Binary(op, lhs, rhs) => {
                let bop = Self::binary_op(*op)?;
                if !matches!(bop, Op::Instanceof | Op::In)
                    && let Expression::Literal(Literal::Number(k)) = rhs.as_ref()
                {
                    // The literal folds into the op: no `LoadConst` dispatch.
                    // The left operand is still evaluated first and the
                    // constant has no effects, so evaluation order holds.
                    if let Expression::Identifier(name) = lhs.as_ref()
                        && let Some(slot) = self.local_slot(name)
                    {
                        self.emit_local_bin_const(name, slot, bop, *k)?;
                    } else {
                        self.compile_expr(lhs)?;
                        self.emit_bin_const(bop, *k)?;
                    }
                    return Ok(());
                }
                self.compile_expr(lhs)?;
                self.compile_expr(rhs)?;
                self.emit(bop);
                self.pop_n(2);
                self.push_n(1);
                Ok(())
            }
            Expression::Member(obj, prop, _site_id) => match prop {
                MemberProperty::Dot(name) => {
                    self.compile_expr(obj)?;
                    let idx = self.add_name(name)?;
                    self.emit(Op::GetProp);
                    self.emit_u16(idx);
                    Ok(())
                }
                MemberProperty::Computed(key) => {
                    self.compile_expr(obj)?;
                    self.compile_expr(key)?;
                    self.emit(Op::GetElement);
                    self.pop_n(2);
                    self.push_n(1);
                    Ok(())
                }
                MemberProperty::Private(name) => {
                    self.compile_expr(obj)?;
                    let idx = self.add_private_name(name)?;
                    self.emit(Op::GetPrivate);
                    self.emit_u16(idx);
                    Ok(())
                }
            },
            Expression::Call(callee, args, site_id) => {
                self.compile_call(callee, args, Op::Call, *site_id)
            }
            Expression::New(callee, args, site_id) => {
                if args.iter().any(|arg| matches!(arg, Expression::Spread(_))) {
                    return Err(CompileError::Unsupported("spread constructor argument"));
                }
                let argc = u16::try_from(args.len())
                    .map_err(|_| CompileError::Ineligible("construct argument count overflow"))?;
                if usize::from(self.current_stack) + args.len() + 1 > usize::from(u16::MAX) {
                    return Err(CompileError::Ineligible("operand stack overflow"));
                }
                // Construct never reads a `this` value from the callee reference
                // (unlike Call), so no LoadCalleeName-style receiver capture is
                // needed — any expression compiles as the callee directly.
                self.compile_expr(callee)?;
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Op::Construct);
                self.emit_u16(argc);
                self.emit_u32(site_id.0);
                self.pop_n(argc + 1);
                self.push_n(1);
                Ok(())
            }
            Expression::Array(elements, _) => self.compile_array_literal(elements),
            Expression::Object(props, _) => self.compile_object_literal(props),
            Expression::Function(_) | Expression::ArrowFunction(_) => {
                self.compile_closure(expr, None, false)
            }
            Expression::Yield(argument, false) if self.generator => {
                self.compile_yield(argument.as_deref(), false)
            }
            _ => Err(CompileError::Unsupported(expression_kind(expr))),
        }
    }

    /// `yield argument`: `[..., value]` → `[..., sent]`, or `[...]` when the
    /// sent value is `discard`ed (an expression statement). A `next(v)`
    /// resumption pushes `v` and jumps over the stub that follows; `return(v)`
    /// resumes at the stub with the operand stack cut back to the persistent
    /// height, so the stub is the same exit a `return v` statement emits
    /// (running `finally` blocks and closing iterators on the way out);
    /// `throw(e)` resumes at the enclosing handler like a throw raised by the
    /// op itself.
    fn compile_yield(
        &mut self,
        argument: Option<&Expression>,
        discard: bool,
    ) -> Result<(), CompileError> {
        match argument {
            Some(argument) => self.compile_expr(argument)?,
            None => {
                self.emit(Op::LoadUndefined);
                self.push_n(1);
            }
        }
        self.emit(Op::Yield);
        self.emit_u16(self.stack_base);
        self.code.push(u8::from(discard));
        let skip = self.code.len();
        self.code.extend_from_slice(&[0, 0]);
        let (stack, refs) = (self.current_stack, self.current_refs);
        self.current_stack = self.stack_base + 1;
        self.current_refs = 0;
        self.max_stack = self.max_stack.max(self.current_stack);
        self.compile_jump(Terminal::Return, true)?;
        self.current_stack = if discard { stack - 1 } else { stack };
        self.current_refs = refs;
        self.patch_jump(skip)
    }

    /// `MakeClosure` for a function or arrow literal. `inferred_name` is the
    /// name NamedEvaluation gives it; `is_method` marks an object-literal
    /// method (not constructable, has a home object).
    fn compile_closure(
        &mut self,
        expr: &Expression,
        inferred_name: Option<JsString>,
        is_method: bool,
    ) -> Result<(), CompileError> {
        let template = match expr {
            Expression::Function(f) => FunctionTemplate {
                name: f.name.clone(),
                info: f.info(),
                body: f.body.clone(),
                is_arrow: false,
                is_async: f.is_async,
                is_generator: f.is_generator,
                body_is_strict: f.body_is_strict,
                source_text: f.source_text.clone(),
                inferred_name,
            },
            Expression::ArrowFunction(af) => FunctionTemplate {
                name: None,
                info: af.info(),
                body: af.body.body().clone(),
                is_arrow: true,
                is_async: af.is_async,
                is_generator: false,
                body_is_strict: af.body_is_strict,
                source_text: af.source_text.clone(),
                inferred_name,
            },
            _ => return Err(CompileError::Unsupported("closure literal")),
        };
        let idx = u16::try_from(self.functions.len())
            .map_err(|_| CompileError::Ineligible("function template pool overflow"))?;
        self.functions.push(template);
        self.emit(Op::MakeClosure);
        self.emit_u16(idx);
        self.code.push(u8::from(is_method));
        self.push_n(1);
        Ok(())
    }

    /// Compiles the value of a NamedEvaluation position (`name = value`,
    /// `var name = value`): a function or arrow literal is created with
    /// `name`; anything else compiles as usual.
    fn compile_named_value(&mut self, value: &Expression, name: &str) -> Result<(), CompileError> {
        if is_anonymous_closure(value) {
            self.compile_closure(value, Some(JsString::from_str(name)), false)
        } else {
            self.compile_expr(value)
        }
    }

    /// A literal keeps up to one chunk of pending values on the operand
    /// stack; one too deep to take another chunk stays on the tree-walker.
    fn check_literal_stack(&self) -> Result<(), CompileError> {
        if usize::from(self.current_stack) + LITERAL_VALUE_CHUNK + 2 > usize::from(u16::MAX) {
            return Err(CompileError::Unsupported("literal operand stack"));
        }
        Ok(())
    }

    fn compile_array_literal(
        &mut self,
        elements: &[Option<Expression>],
    ) -> Result<(), CompileError> {
        self.check_literal_stack()?;
        if let Some(template) = constant_array_template(elements) {
            let idx = u16::try_from(self.array_templates.len())
                .map_err(|_| CompileError::Ineligible("array template pool overflow"))?;
            self.array_templates.push(template);
            self.emit(Op::NewArrayConst);
            self.emit_u16(idx);
            self.push_n(1);
            return Ok(());
        }
        if elements.iter().any(Option::is_none) {
            return Err(CompileError::Unsupported("array literal: elision"));
        }
        if elements
            .iter()
            .flatten()
            .any(|e| matches!(e, Expression::Spread(_)))
        {
            return Err(CompileError::Unsupported("array literal: spread"));
        }
        // The first chunk of values becomes the array; later chunks are
        // appended to it, so the operand stack never holds more than one
        // chunk of pending elements above the array.
        let mut created = false;
        let mut pending: u16 = 0;
        for element in elements.iter().flatten() {
            self.compile_expr(element)?;
            pending += 1;
            if usize::from(pending) == LITERAL_VALUE_CHUNK {
                self.flush_array_values(created, pending);
                created = true;
                pending = 0;
            }
        }
        if pending > 0 || !created {
            self.flush_array_values(created, pending);
        }
        Ok(())
    }

    /// `NewArray`/`ArrayAppend` for the `count` values on top of the stack.
    fn flush_array_values(&mut self, created: bool, count: u16) {
        self.emit(if created {
            Op::ArrayAppend
        } else {
            Op::NewArray
        });
        self.emit_u16(count);
        self.pop_n(count);
        if !created {
            self.push_n(1);
        }
    }

    fn compile_object_literal(&mut self, props: &[Property]) -> Result<(), CompileError> {
        self.check_literal_stack()?;
        // Everything that cannot be a static key with a value on the stack is
        // rejected before anything is emitted.
        for prop in props {
            if matches!(prop.key, PropertyKey::Computed(_)) {
                return Err(CompileError::Unsupported("object literal: computed key"));
            }
            if matches!(prop.key, PropertyKey::Private(_)) {
                return Err(CompileError::Unsupported("object literal: private name"));
            }
            if matches!(prop.value, Expression::Spread(_)) {
                return Err(CompileError::Unsupported("object literal: spread"));
            }
            let function_value = matches!(prop.value, Expression::Function(_));
            if (prop.method || prop.kind != PropertyKind::Init) && !function_value {
                return Err(CompileError::Unsupported("object literal: method value"));
            }
        }
        // The first chunk of properties allocates the object; later chunks
        // define theirs on it, so the operand stack holds at most one chunk
        // of pending values above the object.
        let mut created = false;
        let mut chunks = props.chunks(LITERAL_VALUE_CHUNK);
        loop {
            let chunk = chunks.next().unwrap_or(&[]);
            let mut shape = Vec::with_capacity(chunk.len());
            for prop in chunk {
                shape.push(self.compile_shape_property(prop)?);
            }
            let idx = u16::try_from(self.object_shapes.len())
                .map_err(|_| CompileError::Ineligible("object shape pool overflow"))?;
            let count = shape.len() as u16;
            let distinct_data = shape.iter().enumerate().all(|(i, prop)| {
                prop.spec.kind == PropertyKind::Init
                    && !prop.spec.is_method
                    && !prop.spec.sets_prototype(&prop.key)
                    && !shape[..i].iter().any(|earlier| earlier.key == prop.key)
            });
            self.object_shapes.push(ObjectShape {
                props: shape,
                distinct_data,
            });
            self.emit(if created {
                Op::ObjectAppend
            } else {
                Op::NewObject
            });
            self.emit_u16(idx);
            self.pop_n(count);
            if !created {
                self.push_n(1);
                created = true;
            }
            if chunks.len() == 0 {
                return Ok(());
            }
        }
    }

    /// Compiles one statically keyed property's value onto the operand stack
    /// and returns its shape entry.
    fn compile_shape_property(&mut self, prop: &Property) -> Result<ShapeProperty, CompileError> {
        let key = crate::interpreter::eval::static_property_key(&prop.key)
            .expect("compile_object_literal rejects computed and private keys");
        let mut spec = LiteralPropertySpec {
            kind: prop.kind,
            plain_init: !prop.computed && !prop.shorthand && !prop.method,
            is_method: prop.method,
            name_value: false,
        };
        if is_anonymous_closure(&prop.value) {
            let fn_name = match prop.kind {
                _ if spec.sets_prototype(&key) => None,
                PropertyKind::Init => Some(key.to_js_string()),
                PropertyKind::Get => Some(
                    crate::interpreter::eval::prefixed_function_name("get ", &key).to_js_string(),
                ),
                PropertyKind::Set => Some(
                    crate::interpreter::eval::prefixed_function_name("set ", &key).to_js_string(),
                ),
            };
            self.compile_closure(&prop.value, fn_name, prop.method)?;
        } else {
            self.compile_expr(&prop.value)?;
            // An anonymous class is named by the literal after the fact.
            spec.name_value =
                prop.kind == PropertyKind::Init && prop.value.is_anonymous_function_definition();
        }
        Ok(ShapeProperty { key, spec })
    }

    /// Compiles `expr` for its effects only: the operand stack is unchanged
    /// afterwards. Assignments and updates of identifiers fold the discard
    /// into the store/update op instead of a trailing `Pop`.
    fn compile_effect(&mut self, expr: &Expression) -> Result<(), CompileError> {
        match expr {
            Expression::Assign(op, target, value) => {
                // An anonymous class value needs NamedEvaluation, which
                // `compile_expr` hands to the tree-walker.
                if let Expression::Identifier(name) = target.as_ref()
                    && !is_anonymous_class(value)
                {
                    return self.compile_identifier_assign(*op, name, value, true);
                }
            }
            Expression::Update(op, prefix, target)
                if matches!(target.as_ref(), Expression::Identifier(_)) =>
            {
                return self.compile_update(*op, *prefix, target, true);
            }
            Expression::Sequence(exprs) | Expression::Comma(exprs) if !exprs.is_empty() => {
                for e in exprs {
                    self.compile_effect(e)?;
                }
                return Ok(());
            }
            Expression::Yield(argument, false) if self.generator => {
                return self.compile_yield(argument.as_deref(), true);
            }
            _ => {}
        }
        self.compile_expr(expr)?;
        self.emit(Op::Pop);
        self.pop_n(1);
        Ok(())
    }

    /// `name = value` / `name op= value`. Leaves the assigned value on the
    /// operand stack, or nothing when `discard`.
    fn compile_identifier_assign(
        &mut self,
        op: AssignOp,
        name: &str,
        value: &Expression,
        discard: bool,
    ) -> Result<(), CompileError> {
        let idx = self.add_name(name)?;
        if let Some(slot) = self.local_slot(name) {
            // A local of this frame: no `with`, closure or direct eval can
            // change what the name resolves to while the right-hand side
            // runs, so the reference needs no early resolution.
            if op == AssignOp::Assign {
                self.compile_named_value(value, name)?;
            } else if let Expression::Literal(Literal::Number(k)) = value {
                self.emit_local_bin_const(name, slot, Self::compound_binary_op(op)?, *k)?;
            } else {
                self.compile_load_name(name)?;
                self.compile_expr(value)?;
                self.emit(Self::compound_binary_op(op)?);
                self.pop_n(2);
                self.push_n(1);
            }
            self.emit_store_local(idx, slot, discard);
        } else {
            self.emit_resolve_name(idx);
            if op == AssignOp::Assign {
                self.compile_named_value(value, name)?;
            } else {
                self.emit(Op::LoadResolvedName);
                self.emit_u16(idx);
                self.push_n(1);
                if let Expression::Literal(Literal::Number(k)) = value {
                    self.emit_bin_const(Self::compound_binary_op(op)?, *k)?;
                } else {
                    self.compile_expr(value)?;
                    self.emit(Self::compound_binary_op(op)?);
                    self.pop_n(2);
                    self.push_n(1);
                }
            }
            self.emit_store_resolved_name(idx, discard);
        }
        Ok(())
    }

    /// `x++`, `o.p++`, `o[k]--` and friends. The mode byte's bit 2 means "do
    /// not push the result", which effect context sets for identifiers: the
    /// discarded value is unobservable.
    fn compile_update(
        &mut self,
        op: UpdateOp,
        prefix: bool,
        target: &Expression,
        discard: bool,
    ) -> Result<(), CompileError> {
        let mode = match (op, prefix) {
            (UpdateOp::Increment, false) => 0,
            (UpdateOp::Increment, true) => 1,
            (UpdateOp::Decrement, false) => 2,
            (UpdateOp::Decrement, true) => 3,
        };
        match target {
            Expression::Identifier(name) => {
                let idx = self.add_name(name)?;
                if let Some(slot) = self.local_slot(name) {
                    self.emit(Op::UpdateLocal);
                    self.emit_u16(idx);
                    self.emit_u16(slot);
                } else {
                    self.emit(Op::UpdateName);
                    self.emit_u16(idx);
                }
                self.code
                    .push(if discard { mode | UPDATE_DISCARD } else { mode });
                if !discard {
                    self.push_n(1);
                }
                Ok(())
            }
            Expression::Member(obj, MemberProperty::Dot(name), _) if !discard => {
                self.compile_expr(obj)?;
                let idx = self.add_name(name)?;
                self.emit(Op::UpdateProp);
                self.emit_u16(idx);
                self.code.push(mode);
                Ok(())
            }
            Expression::Member(obj, MemberProperty::Private(name), _) if !discard => {
                self.compile_expr(obj)?;
                let idx = self.add_private_name(name)?;
                self.emit(Op::UpdatePrivate);
                self.emit_u16(idx);
                self.code.push(mode);
                Ok(())
            }
            Expression::Member(obj, MemberProperty::Computed(key), _) if !discard => {
                self.compile_expr(obj)?;
                self.compile_expr(key)?;
                self.emit(Op::UpdateElement);
                self.code.push(mode);
                self.pop_n(2);
                self.push_n(1);
                Ok(())
            }
            _ => Err(CompileError::Unsupported("update target")),
        }
    }

    fn is_comparison(op: BinaryOp) -> bool {
        matches!(
            op,
            BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::LtEq
                | BinaryOp::GtEq
                | BinaryOp::Eq
                | BinaryOp::NotEq
                | BinaryOp::StrictEq
                | BinaryOp::StrictNotEq
        )
    }

    /// Compiles `test` and a forward jump taken when it is falsy; returns the
    /// patch site. A comparison fuses into `CmpJumpIfFalse` (its result is
    /// always a Boolean, so ToBoolean is the identity). `local < literal`
    /// keeps `LocalBinConst` + `JumpIfFalse`, which is already two dispatches.
    fn compile_cond_jump(&mut self, test: &Expression) -> Result<usize, CompileError> {
        if let Expression::Binary(op, lhs, rhs) = test
            && Self::is_comparison(*op)
        {
            let local_lhs =
                matches!(lhs.as_ref(), Expression::Identifier(n) if self.local_slot(n).is_some());
            let literal_rhs = matches!(rhs.as_ref(), Expression::Literal(Literal::Number(_)));
            if !(local_lhs && literal_rhs) {
                let cmp = Self::binary_op(*op)?;
                self.compile_expr(lhs)?;
                self.compile_expr(rhs)?;
                self.pop_n(2);
                self.emit(Op::CmpJumpIfFalse);
                self.code.push(cmp as u8);
                let patch = self.code.len();
                self.emit_u16(0);
                return Ok(patch);
            }
        }
        self.compile_expr(test)?;
        self.pop_n(1);
        Ok(self.emit_jump(Op::JumpIfFalse))
    }

    fn compile_call(
        &mut self,
        callee: &Expression,
        args: &[Expression],
        op: Op,
        site_id: CallSiteId,
    ) -> Result<(), CompileError> {
        if args.iter().any(|arg| matches!(arg, Expression::Spread(_))) {
            return Err(CompileError::Unsupported("spread call argument"));
        }
        let argc = u16::try_from(args.len())
            .map_err(|_| CompileError::Ineligible("call argument count overflow"))?;
        if usize::from(self.current_stack) + args.len() + 2 > usize::from(u16::MAX) {
            return Err(CompileError::Ineligible("operand stack overflow"));
        }

        // Stack layout consumed by Call/ReturnCall:
        //   [..., callee, this, arg0, ..., argN]
        // `super.m()` needs no check here: `compile_expr` rejects `Super`.
        match callee {
            // A bare `eval` may resolve to the realm's intrinsic eval at
            // runtime, in which case it needs the caller's lexical
            // environment. Keep the whole Body on the tree-walker rather than
            // changing direct-eval semantics.
            Expression::Identifier(name) if name == "eval" => {
                return Err(CompileError::Ineligible("direct eval call"));
            }
            Expression::Identifier(name) => {
                let name_idx = self.add_name(name)?;
                self.emit(Op::LoadCalleeName);
                self.emit_u16(name_idx);
                self.push_n(2);
            }
            Expression::Member(obj, MemberProperty::Dot(name), _) => {
                self.compile_expr(obj)?;
                let name_idx = self.add_name(name)?;
                self.emit(Op::LoadMethod);
                self.emit_u16(name_idx);
                self.push_n(1);
            }
            Expression::Member(obj, MemberProperty::Computed(key), _) => {
                self.compile_expr(obj)?;
                self.compile_expr(key)?;
                // base + key on the stack become callee + this in place.
                self.emit(Op::LoadMethodElement);
            }
            Expression::Member(obj, MemberProperty::Private(name), _) => {
                self.compile_expr(obj)?;
                let idx = self.add_private_name(name)?;
                self.emit(Op::LoadMethodPrivate);
                self.emit_u16(idx);
                self.push_n(1);
            }
            Expression::BoundCallee(func, this) => {
                self.compile_expr(func)?;
                self.compile_expr(this)?;
            }
            // `super(...)` and a parenthesised optional chain pick their
            // `this` from the callee reference.
            Expression::Super | Expression::OptionalChain(..) => {
                return Err(CompileError::Unsupported("call callee"));
            }
            // Any other callee (an immediately invoked function, `f()()`,
            // `(0, o.m)()`, ...) is not a reference: `this` is undefined
            // (EvaluateCall, sec-evaluatecall).
            other => {
                self.compile_expr(other)?;
                self.emit(Op::LoadUndefined);
                self.push_n(1);
            }
        }
        for arg in args {
            self.compile_expr(arg)?;
        }
        self.emit(op);
        self.emit_u16(argc);
        self.emit_u32(site_id.0);
        self.pop_n(argc + 2);
        if matches!(op, Op::Call | Op::ReturnCall) {
            self.push_n(1);
        }
        Ok(())
    }

    /// `owns_lexicals` lets a `let`/`const` initialise the binding its
    /// enclosing environment already holds in its TDZ: declaration
    /// instantiation hoisted it into the function environment, or `PushEnv`
    /// into the block's. Anywhere else (a Script's top level, whose lexicals
    /// are global declarative bindings) the tree-walker runs the whole
    /// statement.
    fn compile_var_declaration(
        &mut self,
        decl: &VariableDeclaration,
        owns_lexicals: bool,
    ) -> Result<(), CompileError> {
        if decl.kind != VarKind::Var {
            if !owns_lexicals || !matches!(decl.kind, VarKind::Let | VarKind::Const) {
                return Err(CompileError::Unsupported("lexical declaration"));
            }
            return self.compile_lexical_declaration(decl);
        }
        for declarator in &decl.declarations {
            let Pattern::Identifier(name) = &declarator.pattern else {
                return Err(CompileError::Unsupported("var binding pattern"));
            };
            let idx = self.add_var_name(name)?;
            if let Some(init) = &declarator.init {
                if is_anonymous_class(init) {
                    return Err(CompileError::Unsupported("anonymous class initializer"));
                }
                if let Some(slot) = self.local_slot(name) {
                    self.compile_named_value(init, name)?;
                    self.emit_store_local(idx, slot, true);
                } else {
                    self.emit_resolve_name(idx);
                    self.compile_named_value(init, name)?;
                    self.emit_store_resolved_name(idx, true);
                }
            }
        }
        Ok(())
    }

    fn compile_lexical_declaration(
        &mut self,
        decl: &VariableDeclaration,
    ) -> Result<(), CompileError> {
        let kind_byte = u8::from(decl.kind == VarKind::Const);
        for declarator in &decl.declarations {
            let Pattern::Identifier(name) = &declarator.pattern else {
                return Err(CompileError::Unsupported("lexical binding pattern"));
            };
            if declarator.init.as_ref().is_some_and(is_anonymous_class) {
                return Err(CompileError::Unsupported("anonymous class initializer"));
            }
            let idx = self.add_name(name)?;
            match &declarator.init {
                Some(init) => self.compile_named_value(init, name)?,
                None => {
                    self.emit(Op::LoadUndefined);
                    self.push_n(1);
                }
            }
            // The slot `PushEnv` declared this name at, when the innermost
            // open block environment owns it.
            let slot = self
                .scopes
                .last()
                .and_then(|names| names.iter().position(|n| n == name))
                .and_then(|i| u16::try_from(i).ok())
                .filter(|&i| i != NO_INIT_SLOT)
                .unwrap_or(NO_INIT_SLOT);
            self.emit(Op::InitLexical);
            self.emit_u16(idx);
            self.code.push(kind_byte);
            self.emit_u16(slot);
            self.pop_n(1);
        }
        Ok(())
    }

    /// What BlockDeclarationInstantiation declares for the statement lists
    /// of one block (or all `case` clauses of a `switch`): block-level
    /// function declarations first, then `let`/`const`/class bindings in TDZ,
    /// the order `exec_statements` instantiates them in. `None` when a list
    /// holds a `using` declaration, whose environment owns a disposal stack
    /// the VM does not model.
    fn scope_declarations(&self, lists: &[&[Statement]]) -> Option<ScopeDecls> {
        let mut decls = ScopeDecls::default();
        for stmts in lists {
            for stmt in *stmts {
                if let Some(f) = crate::interpreter::hoisting::unwrap_labeled_function(stmt) {
                    decls.functions.push(stmt.clone());
                    decls.function_names.push(f.name.clone());
                }
            }
        }
        for stmts in lists {
            let has_using = stmts.iter().any(|stmt| {
                matches!(
                    stmt,
                    Statement::Variable(decl)
                        if matches!(decl.kind, VarKind::Using | VarKind::AwaitUsing)
                )
            });
            if has_using {
                return None;
            }
            decls
                .lexicals
                .extend(crate::interpreter::hoisting::collect_lexical_names(stmts));
        }
        Some(decls)
    }

    /// Emits `PushEnv` for `decls` and makes its names shadow the enclosing
    /// scopes' for everything compiled until the matching `close_scope`.
    fn open_scope(&mut self, decls: ScopeDecls) -> Result<(), CompileError> {
        let index = self.block_scopes.len();
        let operand = u16::try_from(index)
            .map_err(|_| CompileError::Ineligible("block scope pool overflow"))?;
        let mut names: Vec<String> = Vec::new();
        for name in &decls.function_names {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        let mut lexicals = Vec::with_capacity(decls.lexicals.len());
        for (name, is_const) in &decls.lexicals {
            if !names.contains(name) {
                names.push(name.clone());
                lexicals.push((self.add_name(name)?, *is_const));
            }
        }
        self.block_scopes.push(BlockScope {
            functions: decls.functions,
            lexicals,
            simple_catch: decls.simple_catch,
        });
        self.emit(Op::PushEnv);
        self.emit_u16(operand);
        self.scopes.push(names);
        Ok(())
    }

    fn close_scope(&mut self) {
        self.emit_pop_env(1);
        self.scopes.pop();
    }

    /// `PopEnv` for `count` environments, split to fit its u8 operand. The
    /// compile-time scope stack is the caller's to maintain: a `break` or
    /// `continue` leaves environments without closing them.
    fn emit_pop_env(&mut self, mut count: usize) {
        while count > 0 {
            let n = count.min(usize::from(u8::MAX));
            self.emit(Op::PopEnv);
            self.code.push(n as u8);
            count -= n;
        }
    }

    /// A statement list that runs in its own environment when it declares a
    /// binding (a block, a `try`/`catch`/`finally` block).
    fn compile_scoped_block(&mut self, body: &[Statement]) -> Result<(), CompileError> {
        if !block_declares_lexical(body) {
            return self.compile_statement_list(body);
        }
        let decls = self
            .scope_declarations(&[body])
            .ok_or(CompileError::Unsupported("block-scoped declaration"))?;
        self.open_scope(decls)?;
        self.compile_statements(body, false, true)?;
        self.close_scope();
        Ok(())
    }

    fn compile_literal(&mut self, lit: &Literal) -> Result<(), CompileError> {
        match lit {
            Literal::Number(n) => {
                let idx = self.add_constant(Constant::Number(*n))?;
                self.emit(Op::LoadConst);
                self.emit_u16(idx);
                self.push_n(1);
                Ok(())
            }
            Literal::String(units) => {
                let idx = self.add_constant(Constant::String(JsString::from_vec(units.clone())))?;
                self.emit(Op::LoadConst);
                self.emit_u16(idx);
                self.push_n(1);
                Ok(())
            }
            Literal::Boolean(true) => {
                self.emit(Op::LoadTrue);
                self.push_n(1);
                Ok(())
            }
            Literal::Boolean(false) => {
                self.emit(Op::LoadFalse);
                self.push_n(1);
                Ok(())
            }
            Literal::Null => {
                self.emit(Op::LoadNull);
                self.push_n(1);
                Ok(())
            }
            Literal::BigInt(value) => {
                let idx = self.add_constant(Constant::BigInt(value.clone()))?;
                self.emit(Op::LoadConst);
                self.emit_u16(idx);
                self.push_n(1);
                Ok(())
            }
            Literal::RegExp(pattern, flags) => {
                let idx = u16::try_from(self.regexps.len())
                    .map_err(|_| CompileError::Ineligible("regexp pool overflow"))?;
                self.regexps.push(RegExpTemplate {
                    source: crate::interpreter::eval::regexp_literal_source(pattern),
                    flags: JsString::from_str(flags),
                });
                self.emit(Op::NewRegExp);
                self.emit_u16(idx);
                self.push_n(1);
                Ok(())
            }
        }
    }

    /// Compiles a statement list, emitting an `Op::Safepoint` before each
    /// entry so the VM can reach a GC safepoint at every statement-list
    /// position, matching the tree-walker's per-statement safepoint (issue
    /// #808). Every statement list a compiled chunk can hold runs through
    /// here: the function/script top level and `Statement::Block`'s body.
    /// A statement that compiles to no bytecode (e.g. `;` or `var a;`) drops
    /// its safepoint too — nothing could have been allocated since the prior
    /// one, so it would just be a wasted dispatch.
    fn compile_statement_list(&mut self, body: &[Statement]) -> Result<(), CompileError> {
        self.compile_statements(body, false, false)
    }

    /// `skip_hoisted` is the function top level: its function declarations
    /// are instantiated before the chunk runs, so they emit nothing.
    /// `owns_lexicals` says the list's `let`/`const` initialise bindings the
    /// current environment already holds in TDZ: the function top level, or
    /// a block/`switch` body running in its own `PushEnv` environment.
    fn compile_statements(
        &mut self,
        body: &[Statement],
        skip_hoisted: bool,
        owns_lexicals: bool,
    ) -> Result<(), CompileError> {
        for stmt in body {
            if skip_hoisted
                && matches!(stmt, Statement::FunctionDeclaration(f) if f.phase != DeclPhase::CopyOnly)
            {
                continue;
            }
            debug_assert_eq!(self.current_stack, self.stack_base);
            debug_assert_eq!(self.current_refs, 0);
            let safepoint_at = self.code.len();
            let previous_op_at = self.last_op_at;
            self.emit(Op::Safepoint);
            self.at_function_top_level = owns_lexicals;
            self.compile_statement(stmt)?;
            if self.code.len() == safepoint_at + 1 {
                self.code.truncate(safepoint_at);
                self.last_op_at = previous_op_at;
            }
        }
        Ok(())
    }

    /// Compiles `stmt`; a statement with no native lowering is handed to the
    /// tree-walker as one unit (`ExecAst`).
    fn compile_statement(&mut self, stmt: &Statement) -> Result<(), CompileError> {
        let snap = self.snapshot();
        match self.compile_statement_native(stmt) {
            Err(err @ (CompileError::Unsupported(_) | CompileError::JumpOverflow)) => {
                self.rollback(snap);
                self.emit_exec_ast(stmt, err.reason())
            }
            other => other,
        }
    }

    fn emit_exec_ast(
        &mut self,
        stmt: &Statement,
        reason: &'static str,
    ) -> Result<(), CompileError> {
        debug_assert_eq!(self.current_stack, self.stack_base);
        debug_assert_eq!(self.current_refs, 0);
        // A `return`, `break` or `continue` leaving the statement would skip
        // the code a `finally` block or an iterator close has to run: the
        // construct that owns it stays on the tree-walker as a whole instead.
        if self.exec_ast_escape_blocked(stmt) {
            return Err(CompileError::Unsupported(
                "completion leaves a cleanup region",
            ));
        }
        for name in crate::interpreter::hoisting::collect_var_names(std::slice::from_ref(stmt)) {
            self.add_var_name(&name)?;
        }
        let table = if stmt_contains(stmt, &|s| {
            matches!(s, Statement::Break(_) | Statement::Continue(_))
        }) {
            self.build_escape_table()?
        } else {
            NO_ESCAPE_TABLE
        };
        let idx = self.add_ast_node(AstNode::Stmt(Box::new(stmt.clone())), reason)?;
        self.emit(Op::ExecAst);
        self.emit_u16(idx);
        self.emit_u16(table);
        let mut flags = if self.no_tail > 0 {
            EXEC_NO_TAIL_CALL
        } else {
            0
        };
        if self.goal == CompileGoal::Script {
            flags |= EXEC_SET_COMPLETION;
        }
        self.code.push(flags);
        Ok(())
    }

    /// Whether a completion of `stmt` leaving it would cross a cleanup that
    /// has code to run (a finalizer, an iterator `return()`).
    fn exec_ast_escape_blocked(&self, stmt: &Statement) -> bool {
        if !self.cleanups.iter().any(runs_code) {
            return false;
        }
        let mut jumps = Vec::new();
        collect_escaping_jumps(stmt, &mut Vec::new(), 0, 0, &mut jumps);
        jumps.iter().any(|jump| match jump {
            EscapingJump::Return => true,
            EscapingJump::Break(label) => self
                .resolve_break_frame(label.as_deref())
                .is_none_or(|f| self.crosses_cleanup_code(self.loop_frames[f].cleanup_depth)),
            EscapingJump::Continue(label) => self
                .resolve_continue_frame(label.as_deref())
                .is_none_or(|f| self.crosses_cleanup_code(self.loop_frames[f].cleanup_depth)),
        })
    }

    fn crosses_cleanup_code(&self, from_depth: usize) -> bool {
        self.cleanups[from_depth.min(self.cleanups.len())..]
            .iter()
            .any(runs_code)
    }

    /// A call in tail position may leave the chunk as a `TailCall` only when
    /// nothing has to run after it.
    fn tail_call_allowed(&self) -> bool {
        self.no_tail == 0 && !self.crosses_cleanup_code(0)
    }

    fn bump_persistent(&mut self, n: u16) {
        self.stack_base += n;
        self.max_persistent = self.max_persistent.max(self.stack_base);
    }

    /// One entry per way a `break`/`continue` completion can leave the
    /// statement and reach an enclosing compiled loop: the innermost loop for
    /// the unlabeled forms, and every compiled loop for its own labels.
    fn build_escape_table(&mut self) -> Result<u16, CompileError> {
        if self.loop_frames.is_empty() {
            return Ok(NO_ESCAPE_TABLE);
        }
        let table_idx = self.escape_tables.len();
        if table_idx >= usize::from(NO_ESCAPE_TABLE) {
            return Err(CompileError::Ineligible("escape table overflow"));
        }
        let mut entries: Vec<EscapeEntry> = Vec::new();
        let frame_count = self.loop_frames.len();
        let innermost_loop = self.loop_frames.iter().rposition(|f| !f.is_switch);
        for frame_idx in (0..frame_count).rev() {
            // (label, includes the unlabeled break, includes the unlabeled
            // continue): unlabeled break binds to the innermost frame of any
            // kind, unlabeled continue to the innermost loop.
            let mut targets: Vec<(Option<String>, bool, bool)> = Vec::new();
            let is_switch = self.loop_frames[frame_idx].is_switch;
            if frame_idx == frame_count - 1 {
                targets.push((None, true, false));
            }
            if innermost_loop == Some(frame_idx) {
                targets.push((None, false, true));
            }
            for label in &self.loop_frames[frame_idx].labels {
                targets.push((Some(label.clone()), true, !is_switch));
            }
            let continue_target = self.loop_frames[frame_idx].continue_target;
            let scope_depth = u16::try_from(self.loop_frames[frame_idx].scope_depth)
                .map_err(|_| CompileError::Ineligible("block scope depth overflow"))?;
            let stack_height = self.loop_frames[frame_idx].stack_height;
            for (label, breaks, continues) in targets {
                if breaks {
                    self.loop_frames[frame_idx]
                        .escape_breaks
                        .push((table_idx, entries.len()));
                    entries.push(EscapeEntry {
                        is_continue: false,
                        label: label.clone(),
                        target: u32::MAX,
                        scope_depth,
                        stack_height,
                    });
                }
                if continues {
                    match continue_target {
                        Some(target) => entries.push(EscapeEntry {
                            is_continue: true,
                            label,
                            target: target as u32,
                            scope_depth,
                            stack_height,
                        }),
                        None => {
                            self.loop_frames[frame_idx]
                                .escape_continues
                                .push((table_idx, entries.len()));
                            entries.push(EscapeEntry {
                                is_continue: true,
                                label,
                                target: u32::MAX,
                                scope_depth,
                                stack_height,
                            });
                        }
                    }
                }
            }
        }
        self.escape_tables.push(entries);
        Ok(table_idx as u16)
    }

    fn resolve_escape_targets(&mut self, entries: &[(usize, usize)]) {
        let target = self.code.len();
        self.max_jump_target = self.max_jump_target.max(target);
        for &(table, entry) in entries {
            self.escape_tables[table][entry].target = target as u32;
        }
    }

    fn compile_statement_native(&mut self, stmt: &Statement) -> Result<(), CompileError> {
        let at_top_level = std::mem::take(&mut self.at_function_top_level);
        match stmt {
            Statement::Empty => Ok(()),
            Statement::Expression(expr) => {
                if self.goal == CompileGoal::Script {
                    // The value is the statement's completion value.
                    self.compile_expr(expr)?;
                    self.emit(Op::SetCompletion);
                    self.pop_n(1);
                    Ok(())
                } else {
                    self.compile_effect(expr)
                }
            }
            Statement::Block(body) => {
                // A block is statement-level and net-zero on the stack; each
                // contained statement balances itself. A block that declares
                // a binding runs in its own environment, instantiated by
                // `PushEnv` (BlockDeclarationInstantiation,
                // sec-blockdeclarationinstantiation).
                self.compile_scoped_block(body)
            }
            Statement::Variable(decl) => self.compile_var_declaration(decl, at_top_level),
            Statement::If(if_stmt) => {
                // Annex B.3.3 treats `if (x) function f() {}` as if the
                // function sat in a block; leave that to the tree-walker.
                if is_function_declaration(&if_stmt.consequent)
                    || if_stmt
                        .alternate
                        .as_deref()
                        .is_some_and(is_function_declaration)
                {
                    return Err(CompileError::Unsupported("function declaration in if"));
                }
                // IfStatement always produces a value: an empty selected arm
                // is updated to undefined, and a missing else on the false
                // path returns undefined. A value-producing arm replaces it.
                self.reset_script_completion();
                // Lowering (mirrors `Conditional`'s JumpIfFalse discipline):
                //   <test>
                //   JumpIfFalse else_target   ; JumpIfFalse POPS the test value
                //   <consequent>
                //   [Jump end_target]         ; only when an alternate exists
                // else_target:
                //   [<alternate>]
                // end_target:
                // Branches are statements (net-zero on the stack), so unlike
                // the ternary expression nothing is re-pushed at the merge.
                let else_target = self.compile_cond_jump(&if_stmt.test)?;
                self.compile_statement(&if_stmt.consequent)?;
                if let Some(alternate) = &if_stmt.alternate {
                    let end_target = self.emit_jump(Op::Jump);
                    self.patch_jump(else_target)?;
                    self.compile_statement(alternate)?;
                    self.patch_jump(end_target)?;
                } else {
                    self.patch_jump(else_target)?;
                }
                Ok(())
            }
            Statement::While(while_stmt) => self.compile_while(while_stmt, Vec::new()),
            Statement::DoWhile(do_while_stmt) => self.compile_do_while(do_while_stmt, Vec::new()),
            Statement::For(for_stmt) => self.compile_for(for_stmt, Vec::new()),
            Statement::ForIn(for_in) => self.compile_for_in(for_in, Vec::new()),
            Statement::ForOf(for_of) => self.compile_for_of(for_of, Vec::new()),
            Statement::Try(try_stmt) => self.compile_try(try_stmt),
            Statement::Switch(switch) => self.compile_switch(switch, Vec::new()),
            Statement::Labeled(label, inner) => {
                let mut labels = vec![label.clone()];
                let mut current: &Statement = inner;
                while let Statement::Labeled(next_label, next_inner) = current {
                    labels.push(next_label.clone());
                    current = next_inner;
                }
                match current {
                    Statement::While(while_stmt) => self.compile_while(while_stmt, labels),
                    Statement::DoWhile(do_while_stmt) => {
                        self.compile_do_while(do_while_stmt, labels)
                    }
                    Statement::For(for_stmt) => self.compile_for(for_stmt, labels),
                    Statement::ForIn(for_in) => self.compile_for_in(for_in, labels),
                    Statement::ForOf(for_of) => self.compile_for_of(for_of, labels),
                    Statement::Switch(switch) => self.compile_switch(switch, labels),
                    _ => Err(CompileError::Unsupported("statement:Labeled")),
                }
            }
            Statement::Throw(expr) => {
                self.compile_expr(expr)?;
                self.emit(Op::Throw);
                self.pop_n(1);
                Ok(())
            }
            Statement::Break(label) => self.compile_break(label.as_deref()),
            Statement::Continue(label) => self.compile_continue(label.as_deref()),
            Statement::Return(_) if self.goal == CompileGoal::Script => {
                Err(CompileError::Unsupported("return in script"))
            }
            Statement::Return(None) => {
                if self.cleanups.is_empty() {
                    self.emit(Op::ReturnUndefined);
                } else {
                    self.emit(Op::LoadUndefined);
                    self.push_n(1);
                    self.compile_jump(Terminal::Return, true)?;
                    self.pop_n(1);
                }
                Ok(())
            }
            Statement::Return(Some(expr)) => {
                if self.tail_call_allowed() && contains_tail_call(expr) {
                    return self.compile_tail_return(expr);
                }
                self.compile_return_value(expr)
            }
            _ => Err(CompileError::Unsupported(statement_kind(stmt))),
        }
    }

    /// The jump taken when `lhs` already decides the result; it leaves `lhs`
    /// on the operand stack.
    fn logical_keep_jump(op: LogicalOp) -> Op {
        match op {
            LogicalOp::And => Op::JumpIfFalsyKeep,
            LogicalOp::Or => Op::JumpIfTruthyKeep,
            LogicalOp::NullishCoalescing => Op::JumpIfNotNullishKeep,
        }
    }

    /// `return expr` where a call sits in a nested tail position (`?:`
    /// branch, right operand of `&&`/`||`/`??`, last element of a comma
    /// sequence). Each tail leaf ends in its own return, so a call leaf can
    /// leave the chunk as a `TailCall` exactly like a direct `return f(x)`.
    /// The operand stack is unchanged on exit and control never falls
    /// through. Only entered when `tail_call_allowed()`.
    fn compile_tail_return(&mut self, expr: &Expression) -> Result<(), CompileError> {
        match expr {
            Expression::Call(callee, args, site_id) => {
                self.compile_call(callee, args, Op::ReturnCall, *site_id)?;
                self.compile_jump(Terminal::Return, true)?;
                self.pop_n(1);
                Ok(())
            }
            Expression::TaggedTemplate(..) => Err(CompileError::Unsupported("nested tail call")),
            Expression::Conditional(test, then_e, else_e) => {
                let to_else = self.compile_cond_jump(test)?;
                self.compile_tail_return(then_e)?;
                self.patch_jump(to_else)?;
                self.compile_tail_return(else_e)
            }
            Expression::Logical(op, lhs, rhs) => {
                self.compile_expr(lhs)?;
                let to_return = self.emit_jump(Self::logical_keep_jump(*op));
                self.emit(Op::Pop);
                self.pop_n(1);
                self.compile_tail_return(rhs)?;
                self.patch_jump(to_return)?;
                self.push_n(1);
                self.compile_jump(Terminal::Return, true)?;
                self.pop_n(1);
                Ok(())
            }
            Expression::Sequence(exprs) | Expression::Comma(exprs)
                if let Some((last, prefix)) = exprs.split_last() =>
            {
                for e in prefix {
                    self.compile_effect(e)?;
                }
                self.compile_tail_return(last)
            }
            _ => self.compile_return_value(expr),
        }
    }

    /// Evaluates `expr` and returns it through any open cleanups.
    fn compile_return_value(&mut self, expr: &Expression) -> Result<(), CompileError> {
        self.compile_expr(expr)?;
        self.compile_jump(Terminal::Return, true)?;
        self.pop_n(1);
        Ok(())
    }

    /// A literal `true` has no side effects and cannot end the loop. In
    /// Mandreel's generated labeled loops this saves LoadTrue and
    /// JumpIfFalse (including ToBoolean) on every iteration. A missing test
    /// (`for(;;)`) is equally unconditional.
    fn test_always_true(test: Option<&Expression>) -> bool {
        matches!(
            test,
            None | Some(Expression::Literal(Literal::Boolean(true)))
        )
    }

    /// Resolves a `break`'s target frame. The unlabeled form binds to the
    /// innermost frame of either kind (a `switch` is a break target); the
    /// labeled form searches outward so a label on an outer loop is
    /// reachable past an intervening unlabeled inner loop. The parser's
    /// early-error checks guarantee a labeled form always names an enclosing
    /// frame that compiled successfully — `None` is defensive coding against
    /// a future change that stops being true.
    fn resolve_break_frame(&self, label: Option<&str>) -> Option<usize> {
        match label {
            None => self.loop_frames.len().checked_sub(1),
            Some(name) => self
                .loop_frames
                .iter()
                .rposition(|frame| frame.labels.iter().any(|l| l == name)),
        }
    }

    /// A `continue` targets the innermost loop; `switch` frames are skipped
    /// (sec-continue-statement-runtime-semantics-evaluation).
    fn resolve_continue_frame(&self, label: Option<&str>) -> Option<usize> {
        match label {
            None => self.loop_frames.iter().rposition(|frame| !frame.is_switch),
            Some(name) => self
                .loop_frames
                .iter()
                .rposition(|frame| !frame.is_switch && frame.labels.iter().any(|l| l == name)),
        }
    }

    fn compile_break(&mut self, label: Option<&str>) -> Result<(), CompileError> {
        let idx = self
            .resolve_break_frame(label)
            .ok_or(CompileError::Ineligible("statement:Break"))?;
        debug_assert_eq!(self.current_stack, self.stack_base);
        debug_assert_eq!(self.current_refs, 0);
        self.compile_jump(Terminal::Break(idx), false)
    }

    fn compile_continue(&mut self, label: Option<&str>) -> Result<(), CompileError> {
        let idx = self
            .resolve_continue_frame(label)
            .ok_or(CompileError::Ineligible("statement:Continue"))?;
        debug_assert_eq!(self.current_stack, self.stack_base);
        debug_assert_eq!(self.current_refs, 0);
        self.compile_jump(Terminal::Continue(idx), false)
    }

    /// A completion value pending on the operand stack (a return value)
    /// is `has_value`; it stays on top while the exit runs the cleanups.
    fn compile_jump(&mut self, terminal: Terminal, has_value: bool) -> Result<(), CompileError> {
        // The exit sequence pushes at most a placeholder value and a kind.
        self.push_n(2);
        self.pop_n(2);
        self.emit_unwind(terminal, has_value, self.cleanups.len(), self.scopes.len())
    }

    fn emit_pop_env_to(&mut self, cur_scope: &mut usize, target: usize) {
        debug_assert!(*cur_scope >= target, "unwinding to a deeper scope");
        self.emit_pop_env(cur_scope.saturating_sub(target));
        *cur_scope = target.min(*cur_scope);
    }

    fn emit_drop_below(&mut self, has_value: bool, count: u8) {
        self.emit(Op::DropBelow);
        self.code.push(u8::from(has_value));
        self.code.push(count);
    }

    /// Emits the exit from the innermost of the `from_entry` open cleanups
    /// out to `terminal`'s target: leaves block environments, drops
    /// persistent slots, closes iterators, and where a finalizer is in the
    /// way jumps into it with a completion kind (the rest of the exit is
    /// emitted behind that finalizer by `compile_try`).
    fn emit_unwind(
        &mut self,
        terminal: Terminal,
        has_value: bool,
        from_entry: usize,
        mut cur_scope: usize,
    ) -> Result<(), CompileError> {
        let target_depth = match terminal {
            Terminal::Return => 0,
            Terminal::Break(f) | Terminal::Continue(f) => self.loop_frames[f].cleanup_depth,
        };
        for i in (target_depth..from_entry).rev() {
            match self.cleanups[i] {
                Cleanup::Iter { close, scope_depth } => {
                    self.emit_pop_env_to(&mut cur_scope, scope_depth);
                    if close {
                        let start = self.code.len();
                        self.emit(Op::IterClose);
                        self.code.push(u8::from(has_value));
                        self.unwind_segments.push((start, self.code.len(), i));
                    } else {
                        self.emit_drop_below(has_value, 1);
                    }
                }
                Cleanup::Record { scope_depth } => {
                    self.emit_pop_env_to(&mut cur_scope, scope_depth);
                    self.emit_drop_below(has_value, 2);
                }
                Cleanup::Finally { ctx, scope_depth } => {
                    self.emit_pop_env_to(&mut cur_scope, scope_depth);
                    let kind = self.finally_route(ctx, terminal, has_value)?;
                    if !has_value {
                        self.emit(Op::LoadUndefined);
                    }
                    self.emit_load_kind(kind)?;
                    let site = self.emit_jump(Op::Jump);
                    self.finally_ctxs[ctx].jump_sites.push(site);
                    return Ok(());
                }
            }
        }
        match terminal {
            Terminal::Return => {
                debug_assert!(has_value);
                self.emit(Op::Return);
            }
            Terminal::Break(f) => {
                let depth = self.loop_frames[f].scope_depth;
                self.emit_pop_env_to(&mut cur_scope, depth);
                let site = self.emit_jump(Op::Jump);
                self.loop_frames[f].break_sites.push(site);
            }
            Terminal::Continue(f) => {
                let depth = self.loop_frames[f].scope_depth;
                self.emit_pop_env_to(&mut cur_scope, depth);
                match self.loop_frames[f].continue_target {
                    Some(target) => self.emit_jump_to(Op::Jump, target)?,
                    None => {
                        let site = self.emit_jump(Op::Jump);
                        self.loop_frames[f].continue_sites.push(site);
                    }
                }
            }
        }
        Ok(())
    }

    fn finally_route(
        &mut self,
        ctx: usize,
        terminal: Terminal,
        has_value: bool,
    ) -> Result<u16, CompileError> {
        let routes = &mut self.finally_ctxs[ctx].routes;
        let index = match routes.iter().position(|r| *r == (terminal, has_value)) {
            Some(i) => i,
            None => {
                routes.push((terminal, has_value));
                routes.len() - 1
            }
        };
        u16::try_from(index + 2).map_err(|_| CompileError::Ineligible("finalizer exit overflow"))
    }

    /// Pushes a finalizer completion kind (a Number constant).
    fn emit_load_kind(&mut self, kind: u16) -> Result<(), CompileError> {
        let idx = match self.kind_constants.iter().find(|(k, _)| *k == kind) {
            Some(&(_, idx)) => idx,
            None => {
                let idx = self.add_constant(Constant::Number(f64::from(kind)))?;
                self.kind_constants.push((kind, idx));
                idx
            }
        };
        self.emit(Op::LoadConst);
        self.emit_u16(idx);
        Ok(())
    }

    /// `switch` (sec-switch-statement-runtime-semantics-evaluation,
    /// sec-runtime-semantics-caseblockevaluation): the discriminant is
    /// evaluated once; each case selector is evaluated lazily, in source
    /// order, and compared with `===`; execution starts at the first match
    /// (else the `default` clause) and falls through the later clauses.
    ///
    ///   <discriminant>
    ///   per case:  <selector>; CaseStrictEq match_i
    ///   Pop; Jump default_body | end
    ///   match_i:   Pop; Jump body_i            (one trampoline per case)
    ///   body_0 ... body_n                      (source order)
    ///   end:
    fn compile_switch(
        &mut self,
        switch: &SwitchStatement,
        labels: Vec<String>,
    ) -> Result<(), CompileError> {
        if self.goal != CompileGoal::Function {
            return Err(CompileError::Unsupported("statement:Switch"));
        }
        // Case bodies share one block scope, entered after the discriminant
        // is evaluated; the selectors run inside it
        // (sec-switch-statement-runtime-semantics-evaluation). The tree-walker
        // hoists a case clause's function declarations as `var`-kind
        // bindings with its own rules, so those keep the whole statement
        // there.
        let scoped = switch
            .cases
            .iter()
            .any(|case| block_declares_lexical(&case.consequent));
        let decls = if scoped {
            let lists: Vec<&[Statement]> = switch
                .cases
                .iter()
                .map(|case| case.consequent.as_slice())
                .collect();
            Some(
                self.scope_declarations(&lists)
                    .filter(|decls| decls.functions.is_empty())
                    .ok_or(CompileError::Unsupported("block-scoped declaration"))?,
            )
        } else {
            None
        };
        self.compile_expr(&switch.discriminant)?;
        if let Some(decls) = decls {
            self.open_scope(decls)?;
        }
        let mut match_sites: Vec<Option<usize>> = Vec::with_capacity(switch.cases.len());
        for case in &switch.cases {
            match &case.test {
                Some(test) => {
                    self.compile_expr(test)?;
                    self.pop_n(1);
                    match_sites.push(Some(self.emit_jump(Op::CaseStrictEq)));
                }
                None => match_sites.push(None),
            }
        }
        // No selector matched: drop the discriminant and go to `default`,
        // or past the switch when there is none.
        self.emit(Op::Pop);
        self.pop_n(1);
        let no_match_site = self.emit_jump(Op::Jump);
        let mut trampoline_sites: Vec<Option<usize>> = Vec::with_capacity(switch.cases.len());
        for match_site in &match_sites {
            match match_site {
                Some(site) => {
                    self.patch_jump(*site)?;
                    self.emit(Op::Pop);
                    trampoline_sites.push(Some(self.emit_jump(Op::Jump)));
                }
                None => trampoline_sites.push(None),
            }
        }
        let mut frame = LoopFrame::new(
            labels,
            None,
            self.scopes.len(),
            self.cleanups.len(),
            self.stack_base,
        );
        frame.is_switch = true;
        self.loop_frames.push(frame);
        let mut body_starts: Vec<usize> = Vec::with_capacity(switch.cases.len());
        for case in &switch.cases {
            body_starts.push(self.code.len());
            self.compile_statements(&case.consequent, false, scoped)?;
        }
        let frame = self
            .loop_frames
            .pop()
            .expect("pushed above, not re-entrant");
        for (site, start) in trampoline_sites.iter().zip(&body_starts) {
            if let Some(site) = site {
                self.patch_jump_to(*site, *start)?;
            }
        }
        match switch.cases.iter().position(|case| case.test.is_none()) {
            Some(default_index) => {
                self.patch_jump_to(no_match_site, body_starts[default_index])?;
            }
            None => self.patch_jump(no_match_site)?,
        }
        for site in frame.break_sites {
            self.patch_jump(site)?;
        }
        self.resolve_escape_targets(&frame.escape_breaks);
        if scoped {
            self.close_scope();
        }
        Ok(())
    }

    /// `try`/`catch`/`finally` (sec-try-statement-runtime-semantics-evaluation).
    ///
    ///   <block>                        protected by `Handler` -> catch_pad
    ///   CheckExit
    ///   [Jump past_catch]
    /// catch_pad:                       [thrown]
    ///   [PushEnv; InitLexical e | BindHead pattern | Pop]
    ///   <catch block>
    ///   [PopEnv]
    /// past_catch:
    ///   LoadUndefined; LoadConst 0     completion record (value, kind)
    ///   [Jump finalizer]
    /// throw_pad:                       [thrown]; Handler over block+catch
    ///   LoadConst 1
    /// finalizer:                       also entered by every break/continue/
    ///   <finally block>                return crossing it, with a kind >= 2
    ///   EndFinally table
    ///   [Jump end] exit pads...        one per kind: the rest of the exit
    ///
    /// A `Throw` is dispatched through the chunk's handler table, so the
    /// block's and the catch's ranges are the only places it can resume from.
    fn compile_try(&mut self, t: &TryStatement) -> Result<(), CompileError> {
        if self.goal != CompileGoal::Function {
            return Err(CompileError::Unsupported("statement:Try"));
        }
        debug_assert_eq!(self.current_stack, self.stack_base);
        let base = self.stack_base;
        let scope_depth = self.scopes.len();
        let has_finally = t.finalizer.is_some();
        let inner_from = self.cleanups.len() + usize::from(has_finally);
        let ctx = if has_finally {
            let ctx = self.finally_ctxs.len();
            self.finally_ctxs.push(FinallyCtx::default());
            self.cleanups.push(Cleanup::Finally { ctx, scope_depth });
            Some(ctx)
        } else {
            None
        };
        let height = u16_state(base)?;
        let depth = u16_state(scope_depth)?;

        let try_start = self.code.len();
        self.no_tail += 1;
        self.compile_scoped_block(&t.block)?;
        self.no_tail -= 1;
        self.emit(Op::CheckExit);
        let try_end = self.code.len();

        if let Some(handler) = &t.handler {
            let past_catch = self.emit_jump(Op::Jump);
            let catch_start = self.code.len();
            self.push_handler(
                (try_start, try_end),
                catch_start,
                (height, depth),
                true,
                inner_from,
            );
            self.push_n(1);
            self.compile_catch(handler, has_finally)?;
            self.patch_jump(past_catch)?;
        }
        let (Some(ctx), Some(finalizer)) = (ctx, &t.finalizer) else {
            return Ok(());
        };
        let protected_end = self.code.len();
        self.cleanups.pop();

        // The block and the catch ran to completion.
        self.emit(Op::LoadUndefined);
        self.push_n(1);
        self.emit_load_kind(0)?;
        self.push_n(1);
        let to_finalizer = self.emit_jump(Op::Jump);
        // A throw out of either arrives with the thrown value on the stack.
        let throw_pad = self.code.len();
        self.push_handler(
            (try_start, protected_end),
            throw_pad,
            (height, depth),
            true,
            inner_from,
        );
        self.current_stack = base + 1;
        self.emit_load_kind(1)?;
        self.push_n(1);
        let finalizer_start = self.code.len();
        self.patch_jump_to(to_finalizer, finalizer_start)?;
        let sites = std::mem::take(&mut self.finally_ctxs[ctx].jump_sites);
        for site in sites {
            self.patch_jump_to(site, finalizer_start)?;
        }

        self.cleanups.push(Cleanup::Record { scope_depth });
        self.bump_persistent(2);
        self.compile_scoped_block(finalizer)?;
        self.cleanups.pop();
        self.stack_base -= 2;

        let routes = std::mem::take(&mut self.finally_ctxs[ctx].routes);
        let table = u16::try_from(self.finally_tables.len())
            .map_err(|_| CompileError::Ineligible("finalizer table overflow"))?;
        self.finally_tables.push(
            routes
                .iter()
                .map(|&(_, has_value)| FinallyExit {
                    target: 0,
                    keep_value: has_value,
                })
                .collect(),
        );
        self.emit(Op::EndFinally);
        self.emit_u16(table);
        self.pop_n(2);
        // Each exit's remainder lives behind the finalizer; the normal
        // completion jumps over them.
        let past_exits = if routes.is_empty() {
            None
        } else {
            Some(self.emit_jump(Op::Jump))
        };
        for (i, &(terminal, has_value)) in routes.iter().enumerate() {
            self.finally_tables[usize::from(table)][i].target = self.code.len() as u32;
            self.max_jump_target = self.max_jump_target.max(self.code.len());
            self.emit_unwind(terminal, has_value, self.cleanups.len(), scope_depth)?;
        }
        if let Some(site) = past_exits {
            self.patch_jump(site)?;
        }
        self.finally_ctxs.pop();
        debug_assert_eq!(self.current_stack, self.stack_base);
        Ok(())
    }

    /// Registers a handler over `[start, end)` except for the `IterClose`
    /// exits emitted inside it for cleanups below `inner_from` (the first
    /// cleanup index lying inside the construct the handler belongs to):
    /// those close iterators of the construct itself or of enclosing ones,
    /// after the completion has left the protected code, so an error from
    /// their `return()` must reach the enclosing handlers instead.
    fn push_handler(
        &mut self,
        range: (usize, usize),
        target: usize,
        state: (u16, u16),
        checks_exit: bool,
        inner_from: usize,
    ) {
        let (start, end) = range;
        let mut holes: Vec<(usize, usize)> = self
            .unwind_segments
            .iter()
            .filter(|&&(s, e, cleanup)| s >= start && e <= end && cleanup < inner_from)
            .map(|&(s, e, _)| (s, e))
            .collect();
        holes.sort_unstable();
        let mut from = start;
        for (hole_start, hole_end) in holes {
            if hole_start > from {
                self.handlers.push(Handler {
                    start: from as u32,
                    end: hole_start as u32,
                    target: target as u32,
                    stack_height: state.0,
                    scope_depth: state.1,
                    checks_exit,
                });
            }
            from = hole_end;
        }
        if end > from {
            self.handlers.push(Handler {
                start: from as u32,
                end: end as u32,
                target: target as u32,
                stack_height: state.0,
                scope_depth: state.1,
                checks_exit,
            });
        }
    }

    /// The `catch` clause, entered with the thrown value on the operand stack.
    fn compile_catch(
        &mut self,
        clause: &CatchClause,
        has_finally: bool,
    ) -> Result<(), CompileError> {
        if has_finally {
            self.no_tail += 1;
        }
        let scoped = clause.param.is_some();
        if let Some(param) = &clause.param {
            let mut names = Vec::new();
            param.bound_names(&mut names);
            self.open_scope(ScopeDecls {
                lexicals: names.into_iter().map(|name| (name, false)).collect(),
                simple_catch: matches!(param, Pattern::Identifier(_)),
                ..ScopeDecls::default()
            })?;
        }
        match &clause.param {
            Some(Pattern::Identifier(name)) => {
                self.emit_init_lexical(name, false)?;
            }
            Some(pattern) => {
                self.emit_bind_head(HeadBinding::Declare {
                    pattern: pattern.clone(),
                    kind: BindingKind::Let,
                })?;
            }
            None => {
                self.emit(Op::Pop);
                self.pop_n(1);
            }
        }
        self.compile_scoped_block(&clause.body)?;
        if scoped {
            self.close_scope();
        }
        if has_finally {
            self.no_tail -= 1;
        }
        Ok(())
    }

    /// `InitLexical` of `name` in the innermost block environment, consuming
    /// the value on top of the stack.
    fn emit_init_lexical(&mut self, name: &str, is_const: bool) -> Result<(), CompileError> {
        let idx = self.add_name(name)?;
        let slot = self
            .scopes
            .last()
            .and_then(|names| names.iter().position(|n| n == name))
            .and_then(|i| u16::try_from(i).ok())
            .filter(|&i| i != NO_INIT_SLOT)
            .unwrap_or(NO_INIT_SLOT);
        self.emit(Op::InitLexical);
        self.emit_u16(idx);
        self.code.push(u8::from(is_const));
        self.emit_u16(slot);
        self.pop_n(1);
        Ok(())
    }

    fn emit_bind_head(&mut self, head: HeadBinding) -> Result<(), CompileError> {
        if self.generator && head_binding_contains_yield(&head) {
            return Err(CompileError::Ineligible(
                "yield in tree-walked head binding",
            ));
        }
        let idx = u16::try_from(self.head_bindings.len())
            .map_err(|_| CompileError::Ineligible("head binding pool overflow"))?;
        self.head_bindings.push(head);
        self.emit(Op::BindHead);
        self.emit_u16(idx);
        self.pop_n(1);
        Ok(())
    }

    /// Stores the value on top of the stack into the existing binding `name`
    /// (the `var` of a for-in/of head, or an assignment target).
    fn emit_store_name(&mut self, name: &str) -> Result<(), CompileError> {
        let idx = self.add_name(name)?;
        if let Some(slot) = self.local_slot(name) {
            self.emit_store_local(idx, slot, true);
        } else {
            self.emit_resolve_name(idx);
            self.emit_store_resolved_name(idx, true);
        }
        Ok(())
    }

    fn compile_for_in(
        &mut self,
        for_in: &crate::ast::ForInStatement,
        labels: Vec<String>,
    ) -> Result<(), CompileError> {
        self.compile_for_in_of(
            &for_in.left,
            &for_in.right,
            &for_in.body,
            false,
            for_in.may_capture_iteration_scope(),
            labels,
        )
    }

    fn compile_for_of(
        &mut self,
        for_of: &crate::ast::ForOfStatement,
        labels: Vec<String>,
    ) -> Result<(), CompileError> {
        if for_of.is_await {
            return Err(CompileError::Unsupported("for await"));
        }
        self.compile_for_in_of(
            &for_of.left,
            &for_of.right,
            &for_of.body,
            true,
            for_of.may_capture_iteration_scope(),
            labels,
        )
    }

    /// `for (head in/of expr) body` (sec-forin-div-ofheadevaluation,
    /// sec-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset).
    /// The iterator (a for-of's iterator, a for-in's enumerator) is one
    /// persistent operand slot for the whole loop:
    ///
    ///   [PushEnv (TDZ)] <expr> [PopEnv]; GetIterator | ForInHead
    ///   [PushEnv head scope]            only when no closure can capture it
    /// top:
    ///   IterStep done                   [it] -> [it, value] | jump, it dropped
    ///   [PushEnv head scope]            a fresh environment per iteration
    ///   <bind head>                     consumes the value
    ///   <body>
    ///   [PopEnv]; Jump top
    /// throw_pad: IterCloseThrow         handler over bind + body (for-of)
    /// break_pad: IterClose | Pop        `break` and exits from nested loops
    /// done:      [PopEnv]
    fn compile_for_in_of(
        &mut self,
        left: &ForInOfLeft,
        right: &Expression,
        body: &Statement,
        is_of: bool,
        may_capture: bool,
        labels: Vec<String>,
    ) -> Result<(), CompileError> {
        if self.goal != CompileGoal::Function {
            return Err(CompileError::Unsupported(if is_of {
                "statement:ForOf"
            } else {
                "statement:ForIn"
            }));
        }
        enum Head<'a> {
            Lexical(String, bool),
            LexicalPattern(&'a Pattern, bool, Vec<String>),
            VarName(String),
            VarPattern(&'a Pattern, Vec<String>),
            AssignName(String),
            AssignPattern(&'a Pattern),
        }
        let head = match left {
            ForInOfLeft::Variable(decl) => {
                let [declarator] = decl.declarations.as_slice() else {
                    return Err(CompileError::Unsupported("for-in/of head declarations"));
                };
                match (decl.kind, &declarator.pattern) {
                    (VarKind::Var, _) if declarator.init.is_some() => {
                        return Err(CompileError::Unsupported("for-in initializer"));
                    }
                    (VarKind::Let | VarKind::Const, Pattern::Identifier(name)) => {
                        Head::Lexical(name.clone(), decl.kind == VarKind::Const)
                    }
                    (VarKind::Let | VarKind::Const, pattern) => {
                        let mut names = Vec::new();
                        pattern.bound_names(&mut names);
                        Head::LexicalPattern(pattern, decl.kind == VarKind::Const, names)
                    }
                    (VarKind::Var, Pattern::Identifier(name)) => Head::VarName(name.clone()),
                    (VarKind::Var, pattern) => {
                        let mut names = Vec::new();
                        pattern.bound_names(&mut names);
                        Head::VarPattern(pattern, names)
                    }
                    _ => return Err(CompileError::Unsupported("for-in/of using head")),
                }
            }
            ForInOfLeft::Pattern(Pattern::Identifier(name)) => Head::AssignName(name.clone()),
            ForInOfLeft::Pattern(pattern) => Head::AssignPattern(pattern),
            ForInOfLeft::Expression(Expression::Identifier(name)) => Head::AssignName(name.clone()),
            ForInOfLeft::Expression(_) => {
                return Err(CompileError::Unsupported("for-in/of expression target"));
            }
        };

        // The head's names are in their TDZ while the expression runs.
        let lexical_names: Vec<(String, bool)> = match &head {
            Head::Lexical(name, is_const) => vec![(name.clone(), *is_const)],
            Head::LexicalPattern(_, is_const, names) => {
                names.iter().map(|n| (n.clone(), *is_const)).collect()
            }
            _ => Vec::new(),
        };
        if lexical_names.is_empty() {
            self.compile_expr(right)?;
        } else {
            self.open_scope(ScopeDecls {
                lexicals: lexical_names.clone(),
                ..ScopeDecls::default()
            })?;
            self.compile_expr(right)?;
            self.close_scope();
        }
        self.emit(if is_of {
            Op::GetIterator
        } else {
            Op::ForInHead
        });
        match &head {
            Head::VarName(name) => {
                self.add_var_name(name)?;
            }
            Head::VarPattern(_, names) => {
                for name in names {
                    self.add_var_name(name)?;
                }
            }
            _ => {}
        }

        // The iterator stays below everything the loop runs.
        self.cleanups.push(Cleanup::Iter {
            close: is_of,
            scope_depth: self.scopes.len(),
        });
        self.bump_persistent(1);

        let hoisted = matches!(head, Head::Lexical(..)) && !may_capture;
        if hoisted {
            self.open_scope(ScopeDecls {
                lexicals: lexical_names.clone(),
                ..ScopeDecls::default()
            })?;
        }
        let loop_top = self.code.len();
        self.loop_frames.push(LoopFrame::new(
            labels,
            Some(loop_top),
            self.scopes.len(),
            self.cleanups.len(),
            self.stack_base,
        ));
        let step = self.emit_jump(if is_of { Op::IterStep } else { Op::ForInStep });
        // The iteration value is on top, below the loop body's statements.
        self.push_n(1);
        let range_start = self.code.len();
        let per_iteration = !lexical_names.is_empty() && !hoisted;
        if per_iteration {
            self.open_scope(ScopeDecls {
                lexicals: lexical_names.clone(),
                ..ScopeDecls::default()
            })?;
        }
        match &head {
            Head::Lexical(name, is_const) => self.emit_init_lexical(name, *is_const)?,
            Head::LexicalPattern(pattern, is_const, _) => {
                self.emit_bind_head(HeadBinding::Declare {
                    pattern: (*pattern).clone(),
                    kind: if *is_const {
                        BindingKind::Const
                    } else {
                        BindingKind::Let
                    },
                })?;
            }
            Head::VarName(name) | Head::AssignName(name) => self.emit_store_name(name)?,
            Head::VarPattern(pattern, _) => {
                self.emit_bind_head(HeadBinding::Declare {
                    pattern: (*pattern).clone(),
                    kind: BindingKind::Var,
                })?;
            }
            Head::AssignPattern(pattern) => {
                self.emit_bind_head(HeadBinding::Assign {
                    pattern: (*pattern).clone(),
                })?;
            }
        }
        self.compile_statement(body)?;
        if per_iteration {
            self.close_scope();
        }
        let range_end = self.code.len();
        debug_assert_eq!(self.current_stack, self.stack_base);
        self.emit_jump_to(Op::Jump, loop_top)?;
        let frame = self
            .loop_frames
            .pop()
            .expect("pushed above, not re-entrant");
        debug_assert!(
            frame.continue_sites.is_empty(),
            "a for-in/of's continue target is known up front"
        );
        if is_of {
            let pad = self.code.len();
            self.push_handler(
                (range_start, range_end),
                pad,
                (u16_state(self.stack_base)?, u16_state(frame.scope_depth)?),
                false,
                self.cleanups.len(),
            );
            self.emit(Op::IterCloseThrow);
        }
        for site in frame.break_sites {
            self.patch_jump(site)?;
        }
        self.resolve_escape_targets(&frame.escape_breaks);
        if is_of {
            self.emit(Op::IterClose);
            self.code.push(0);
        } else {
            self.emit(Op::Pop);
        }
        self.patch_jump(step)?;
        if hoisted {
            self.close_scope();
        }
        self.cleanups.pop();
        self.stack_base -= 1;
        self.pop_n(1);
        debug_assert_eq!(self.current_stack, self.stack_base);
        Ok(())
    }

    fn compile_while(
        &mut self,
        while_stmt: &WhileStatement,
        labels: Vec<String>,
    ) -> Result<(), CompileError> {
        // WhileLoopEvaluation starts its local V at undefined and updates it
        // only when the body produces a non-empty value.
        self.reset_script_completion();
        let loop_start = self.code.len();
        self.loop_frames.push(LoopFrame::new(
            labels,
            Some(loop_start),
            self.scopes.len(),
            self.cleanups.len(),
            self.stack_base,
        ));
        let exit = if Self::test_always_true(Some(&while_stmt.test)) {
            None
        } else {
            Some(self.compile_cond_jump(&while_stmt.test)?)
        };
        self.compile_statement(&while_stmt.body)?;
        debug_assert_eq!(self.current_stack, self.stack_base);
        debug_assert_eq!(self.current_refs, 0);
        self.emit_jump_to(Op::Jump, loop_start)?;
        if let Some(exit) = exit {
            self.patch_jump(exit)?;
        }
        let frame = self
            .loop_frames
            .pop()
            .expect("pushed above, not re-entrant");
        debug_assert!(
            frame.continue_sites.is_empty(),
            "while's continue resolves immediately, never deferred"
        );
        for site in frame.break_sites {
            self.patch_jump(site)?;
        }
        self.resolve_escape_targets(&frame.escape_breaks);
        Ok(())
    }

    fn compile_do_while(
        &mut self,
        do_while_stmt: &DoWhileStatement,
        labels: Vec<String>,
    ) -> Result<(), CompileError> {
        // DoWhileLoopEvaluation starts V at undefined and evaluates its test
        // only after the body, including after a matching continue.
        self.reset_script_completion();
        let loop_start = self.code.len();
        self.loop_frames.push(LoopFrame::new(
            labels,
            None,
            self.scopes.len(),
            self.cleanups.len(),
            self.stack_base,
        ));
        self.compile_statement(&do_while_stmt.body)?;
        let frame = self
            .loop_frames
            .pop()
            .expect("pushed above, not re-entrant");
        for site in frame.continue_sites {
            self.patch_jump(site)?;
        }
        self.resolve_escape_targets(&frame.escape_continues);
        if Self::test_always_true(Some(&do_while_stmt.test)) {
            self.emit_jump_to(Op::Jump, loop_start)?;
        } else {
            let exit = self.compile_cond_jump(&do_while_stmt.test)?;
            self.emit_jump_to(Op::Jump, loop_start)?;
            self.patch_jump(exit)?;
        }
        for site in frame.break_sites {
            self.patch_jump(site)?;
        }
        self.resolve_escape_targets(&frame.escape_breaks);
        Ok(())
    }

    fn compile_for(
        &mut self,
        for_stmt: &ForStatement,
        labels: Vec<String>,
    ) -> Result<(), CompileError> {
        // ForBodyEvaluation has the same V accumulator semantics as while.
        // Reset once before the initializer, not at the loop backedge, so
        // later empty iterations retain the prior V.
        self.reset_script_completion();
        // A `let`/`const` head owns an environment around the whole loop
        // (sec-for-statement-runtime-semantics-labelledevaluation). Its
        // bindings are copied per iteration only when the loop can observe
        // the identity of that environment (CreatePerIterationEnvironment,
        // sec-createperiterationenvironment), the same test `exec_for` uses.
        let mut scoped = false;
        if let Some(ForInit::Variable(decl)) = &for_stmt.init
            && decl.kind != VarKind::Var
        {
            if !matches!(decl.kind, VarKind::Let | VarKind::Const) {
                return Err(CompileError::Unsupported("lexical declaration"));
            }
            let lexicals = crate::interpreter::hoisting::declaration_bound_names(decl)
                .into_iter()
                .map(|name| (name, decl.kind == VarKind::Const))
                .collect();
            self.open_scope(ScopeDecls {
                lexicals,
                ..ScopeDecls::default()
            })?;
            scoped = true;
        }
        let per_iteration = scoped && for_stmt.may_capture_iteration_scope();
        if let Some(init) = &for_stmt.init {
            match init {
                ForInit::Variable(decl) => self.compile_var_declaration(decl, scoped)?,
                ForInit::Expression(expr) => self.compile_effect(expr)?,
            }
        }
        if per_iteration {
            self.emit(Op::CopyIterEnv);
        }
        let loop_start = self.code.len();
        let exit = if Self::test_always_true(for_stmt.test.as_ref()) {
            None
        } else {
            let test = for_stmt
                .test
                .as_ref()
                .expect("test_always_true(None) is true");
            Some(self.compile_cond_jump(test)?)
        };
        self.loop_frames.push(LoopFrame::new(
            labels,
            None,
            self.scopes.len(),
            self.cleanups.len(),
            self.stack_base,
        ));
        self.compile_statement(&for_stmt.body)?;
        let frame = self
            .loop_frames
            .pop()
            .expect("pushed above, not re-entrant");
        for site in frame.continue_sites {
            self.patch_jump(site)?;
        }
        self.resolve_escape_targets(&frame.escape_continues);
        if per_iteration {
            self.emit(Op::CopyIterEnv);
        }
        if let Some(update) = &for_stmt.update {
            self.compile_effect(update)?;
        }
        debug_assert_eq!(self.current_stack, self.stack_base);
        debug_assert_eq!(self.current_refs, 0);
        self.emit_jump_to(Op::Jump, loop_start)?;
        if let Some(exit) = exit {
            self.patch_jump(exit)?;
        }
        for site in frame.break_sites {
            self.patch_jump(site)?;
        }
        self.resolve_escape_targets(&frame.escape_breaks);
        if scoped {
            self.close_scope();
        }
        Ok(())
    }

    fn finish(mut self) -> Chunk {
        if self.goal == CompileGoal::Script {
            // ScriptEvaluation exposes the StatementList's last non-empty
            // completion. Expression statements and the supported structured
            // statements maintain that accumulator via SetCompletion.
            self.emit(Op::ReturnCompletion);
        } else {
            // Emit a trailing `ReturnUndefined` unless the chunk already ends in a
            // return AND no branch falls through to the end. A forward jump whose
            // target is the end of the chunk (e.g. the false arm of a one-armed
            // `if` whose consequent ends in `return`) would otherwise run `pc` past
            // the last byte of `code` and panic in the VM dispatch loop.
            if !ends_with_return(&self.code, self.last_op_at)
                || self.max_jump_target >= self.code.len()
            {
                self.emit(Op::ReturnUndefined);
            }
        }
        let global_hints = self
            .names
            .iter()
            .map(|name| GlobalHint::new(name.as_str()))
            .collect();
        Chunk {
            code: self.code,
            constants: self.constants,
            names: self.names,
            private_names: self.private_names,
            global_hints,
            var_names: self.var_names,
            max_stack: self.max_stack,
            max_refs: self.max_refs,
            ast: self.ast,
            escape_tables: self.escape_tables,
            block_scopes: self.block_scopes,
            functions: self.functions,
            array_templates: self.array_templates,
            object_shapes: self.object_shapes,
            regexps: self.regexps,
            needs_instantiation: false,
            top_level_lexicals: None,
            handlers: self.handlers,
            finally_tables: self.finally_tables,
            head_bindings: self.head_bindings,
            persistent_slots: self.max_persistent,
            fallback_reasons: self.fallback_reasons,
            trivial_private_getter: None,
        }
    }
}

fn u16_state(n: impl TryInto<u16>) -> Result<u16, CompileError> {
    n.try_into()
        .map_err(|_| CompileError::Ineligible("handler state overflow"))
}

/// Whether leaving through this cleanup runs code (a finalizer, an iterator's
/// `return()`), as opposed to only dropping operand slots.
fn runs_code(cleanup: &Cleanup) -> bool {
    matches!(
        cleanup,
        Cleanup::Finally { .. } | Cleanup::Iter { close: true, .. }
    )
}

/// The `break`/`continue`/`return` statements of `stmt` (not entering nested
/// function bodies) whose completion leaves `stmt`: a `break` or `continue`
/// not bound by a loop, `switch` or label inside it.
fn collect_escaping_jumps(
    stmt: &Statement,
    labels: &mut Vec<String>,
    loops: usize,
    switches: usize,
    out: &mut Vec<EscapingJump>,
) {
    match stmt {
        Statement::Return(_) => out.push(EscapingJump::Return),
        Statement::Break(None) if loops == 0 && switches == 0 => {
            out.push(EscapingJump::Break(None));
        }
        Statement::Break(Some(label)) if !labels.contains(label) => {
            out.push(EscapingJump::Break(Some(label.clone())));
        }
        Statement::Continue(None) if loops == 0 => out.push(EscapingJump::Continue(None)),
        Statement::Continue(Some(label)) if !labels.contains(label) => {
            out.push(EscapingJump::Continue(Some(label.clone())));
        }
        Statement::Block(body) => {
            for inner in body {
                collect_escaping_jumps(inner, labels, loops, switches, out);
            }
        }
        Statement::If(i) => {
            collect_escaping_jumps(&i.consequent, labels, loops, switches, out);
            if let Some(alternate) = &i.alternate {
                collect_escaping_jumps(alternate, labels, loops, switches, out);
            }
        }
        Statement::While(w) => collect_escaping_jumps(&w.body, labels, loops + 1, switches, out),
        Statement::DoWhile(d) => collect_escaping_jumps(&d.body, labels, loops + 1, switches, out),
        Statement::For(f) => collect_escaping_jumps(&f.body, labels, loops + 1, switches, out),
        Statement::ForIn(f) => collect_escaping_jumps(&f.body, labels, loops + 1, switches, out),
        Statement::ForOf(f) => collect_escaping_jumps(&f.body, labels, loops + 1, switches, out),
        Statement::Labeled(label, inner) => {
            labels.push(label.clone());
            collect_escaping_jumps(inner, labels, loops, switches, out);
            labels.pop();
        }
        Statement::With(_, inner) => collect_escaping_jumps(inner, labels, loops, switches, out),
        Statement::Try(t) => {
            for inner in &t.block {
                collect_escaping_jumps(inner, labels, loops, switches, out);
            }
            if let Some(handler) = &t.handler {
                for inner in &handler.body {
                    collect_escaping_jumps(inner, labels, loops, switches, out);
                }
            }
            if let Some(finalizer) = &t.finalizer {
                for inner in finalizer {
                    collect_escaping_jumps(inner, labels, loops, switches, out);
                }
            }
        }
        Statement::Switch(s) => {
            for case in &s.cases {
                for inner in &case.consequent {
                    collect_escaping_jumps(inner, labels, loops, switches + 1, out);
                }
            }
        }
        _ => {}
    }
}

/// A function or arrow literal without a name of its own: the only
/// NamedEvaluation values the VM names as it creates them.
fn is_anonymous_closure(expr: &Expression) -> bool {
    matches!(expr, Expression::Function(_) | Expression::ArrowFunction(_))
        && expr.is_anonymous_function_definition()
}

/// An anonymous class expression, whose NamedEvaluation stays on the
/// tree-walker.
fn is_anonymous_class(expr: &Expression) -> bool {
    matches!(expr, Expression::Class(_)) && expr.is_anonymous_function_definition()
}

/// The primitive a literal element stands for, if it is one: numbers
/// (including a negated number), strings, booleans, `null` and BigInts.
fn constant_element(expr: &Expression) -> Option<JsValue> {
    match expr {
        Expression::Literal(Literal::Number(n)) => Some(JsValue::number(*n)),
        Expression::Literal(Literal::String(units)) => {
            Some(JsValue::string(JsString::from_vec(units.clone())))
        }
        Expression::Literal(Literal::Boolean(b)) => Some(JsValue::boolean(*b)),
        Expression::Literal(Literal::Null) => Some(JsValue::NULL),
        Expression::Literal(Literal::BigInt(value)) => Some(JsValue::bigint(value.clone())),
        Expression::Unary(UnaryOp::Minus, operand) => match operand.as_ref() {
            Expression::Literal(Literal::Number(n)) => Some(JsValue::number(-*n)),
            _ => None,
        },
        _ => None,
    }
}

/// An array literal made only of primitive constants and holes can be built
/// from a template, one fresh array per evaluation.
fn constant_array_template(elements: &[Option<Expression>]) -> Option<ArrayTemplate> {
    let mut values: Vec<Option<JsValue>> = Vec::with_capacity(elements.len());
    for element in elements {
        match element {
            Some(expr) => values.push(Some(constant_element(expr)?)),
            None => values.push(None),
        }
    }
    if values.iter().all(Option::is_some) {
        Some(ArrayTemplate::Dense(values.into_iter().flatten().collect()))
    } else {
        Some(ArrayTemplate::Holey(values))
    }
}

/// `Super` and private names only make sense inside their parent expression;
/// `Spread` is an argument/element form, not an expression of its own, and a
/// `BoundCallee` or `PreIterated` is meaningful only to the call or spread that
/// contains it: `compile_call` lowers a `BoundCallee` callee itself, and a
/// spread (which the compiler declines) hands its whole parent to the
/// tree-walker.
fn is_standalone_expression(expr: &Expression) -> bool {
    !matches!(
        expr,
        Expression::Super
            | Expression::PrivateIdentifier(_)
            | Expression::Spread(_)
            | Expression::BoundCallee(..)
            | Expression::PreIterated(_)
    )
}

fn is_function_declaration(stmt: &Statement) -> bool {
    crate::interpreter::hoisting::unwrap_labeled_function(stmt).is_some()
}

/// Pre-order walk over `stmt` and the statements nested in it (not into
/// function bodies, which are separate Bodies); true when `pred` matches any.
fn stmt_contains(stmt: &Statement, pred: &dyn Fn(&Statement) -> bool) -> bool {
    stmts_contain(std::slice::from_ref(stmt), pred)
}

fn stmts_contain(stmts: &[Statement], pred: &dyn Fn(&Statement) -> bool) -> bool {
    stmts.iter().any(|stmt| {
        pred(stmt)
            || match stmt {
                Statement::Block(body) => stmts_contain(body, pred),
                Statement::If(i) => {
                    stmt_contains(&i.consequent, pred)
                        || i.alternate
                            .as_deref()
                            .is_some_and(|a| stmt_contains(a, pred))
                }
                Statement::While(w) => stmt_contains(&w.body, pred),
                Statement::DoWhile(d) => stmt_contains(&d.body, pred),
                Statement::For(f) => stmt_contains(&f.body, pred),
                Statement::ForIn(f) => stmt_contains(&f.body, pred),
                Statement::ForOf(f) => stmt_contains(&f.body, pred),
                Statement::Labeled(_, inner) => stmt_contains(inner, pred),
                Statement::With(_, inner) => stmt_contains(inner, pred),
                Statement::Try(t) => {
                    stmts_contain(&t.block, pred)
                        || t.handler
                            .as_ref()
                            .is_some_and(|h| stmts_contain(&h.body, pred))
                        || t.finalizer.as_ref().is_some_and(|f| stmts_contain(f, pred))
                }
                Statement::Switch(s) => s.cases.iter().any(|c| stmts_contain(&c.consequent, pred)),
                _ => false,
            }
    })
}

type PoisonFn = fn(&Expression) -> bool;

fn is_yield_expression(expr: &Expression) -> bool {
    matches!(expr, Expression::Yield(..))
}

/// A `using`/`await using` declaration anywhere in `stmt` (not nested
/// functions): the resources are disposed by the generator driver, which the
/// state machine implements.
fn declares_using(stmt: &Statement) -> bool {
    use crate::ast::{ForInOfLeft, VarKind};
    let is_using = |kind: VarKind| matches!(kind, VarKind::Using | VarKind::AwaitUsing);
    match stmt {
        Statement::Variable(decl) => is_using(decl.kind),
        Statement::For(f) => {
            matches!(&f.init, Some(ForInit::Variable(decl)) if is_using(decl.kind))
        }
        Statement::ForIn(f) => {
            matches!(&f.left, ForInOfLeft::Variable(decl) if is_using(decl.kind))
        }
        Statement::ForOf(f) => {
            matches!(&f.left, ForInOfLeft::Variable(decl) if is_using(decl.kind))
        }
        _ => false,
    }
}

/// Whether handing `node` to the tree-walker would strand a `yield`.
fn node_contains_yield(node: &AstNode) -> bool {
    match node {
        AstNode::Expr(expr) => expr_has_poison(expr, is_yield_expression),
        AstNode::Stmt(stmt) => {
            body_has_poison(std::slice::from_ref(stmt.as_ref()), is_yield_expression)
        }
    }
}

fn head_binding_contains_yield(head: &HeadBinding) -> bool {
    match head {
        HeadBinding::Declare { pattern, .. } | HeadBinding::Assign { pattern } => {
            pattern_has_poison(pattern, is_yield_expression)
        }
    }
}

/// The Script goal's poison: direct `eval`, dynamic `import()` and
/// `import.meta`. `yield`, `await`, `new.target` and `super`
/// cannot occur in a Script's own code: the traversal only meets them inside
/// nested arrows, which own them, so they must not keep the script
/// whole-body (the JetStream async harness is one such arrow).
fn is_script_poison_expression(expr: &Expression) -> bool {
    match expr {
        Expression::ImportMeta
        | Expression::Import(..)
        | Expression::ImportDefer(..)
        | Expression::ImportSource(..) => true,
        Expression::Call(callee, _, _) => {
            matches!(callee.as_ref(), Expression::Identifier(name) if name == "eval")
        }
        _ => false,
    }
}

/// A construct that only has its meaning when the whole function runs as one
/// tree-walked frame, or whose compile-time handling depends on resolving
/// names in the enclosing scope chain. Such a body stays wholly on the
/// tree-walker, exactly as before per-node fallback existed.
///
/// `yield` and `await` are not poison. Only plain (non-async, non-generator)
/// function bodies reach `compile_function_goal`, so a `yield`/`await`
/// expression the traversal meets can only belong to a nested arrow, which
/// owns it. A nested arrow's `new.target`, `super` and direct `eval` are
/// inherited from the frame, so those stay poison.
fn is_poison_expression(expr: &Expression) -> bool {
    is_script_poison_expression(expr) || matches!(expr, Expression::Super | Expression::NewTarget)
}

fn pattern_has_poison(pattern: &Pattern, poison: PoisonFn) -> bool {
    use crate::ast::{ArrayPatternElement, ObjectPatternProperty, PropertyKey};
    match pattern {
        Pattern::Identifier(_) => false,
        Pattern::Assign(inner, default) => {
            pattern_has_poison(inner, poison) || expr_has_poison(default, poison)
        }
        Pattern::Rest(inner) => pattern_has_poison(inner, poison),
        Pattern::MemberExpression(expr) => expr_has_poison(expr, poison),
        Pattern::Array(elements) => elements.iter().flatten().any(|e| match e {
            ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                pattern_has_poison(p, poison)
            }
        }),
        Pattern::Object(props) => props.iter().any(|p| match p {
            ObjectPatternProperty::KeyValue(key, value) => {
                matches!(key, PropertyKey::Computed(e) if expr_has_poison(e, poison))
                    || pattern_has_poison(value, poison)
            }
            ObjectPatternProperty::Shorthand(_) => false,
            ObjectPatternProperty::Rest(p) => pattern_has_poison(p, poison),
        }),
    }
}

fn expr_has_poison(expr: &Expression, poison: PoisonFn) -> bool {
    crate::ast::expr_contains_matching(expr, &poison)
}

fn declaration_has_poison(decl: &VariableDeclaration, poison: PoisonFn) -> bool {
    decl.declarations
        .iter()
        .any(|d| pattern_has_poison(&d.pattern, poison))
}

fn for_in_of_left_has_poison(left: &crate::ast::ForInOfLeft, poison: PoisonFn) -> bool {
    match left {
        crate::ast::ForInOfLeft::Variable(decl) => declaration_has_poison(decl, poison),
        crate::ast::ForInOfLeft::Pattern(p) => pattern_has_poison(p, poison),
        crate::ast::ForInOfLeft::Expression(e) => expr_has_poison(e, poison),
    }
}

/// One-time deep scan, run before anything is emitted: once a parent falls
/// back, its subtree is never visited by the compiler, so poison inside it
/// would otherwise go unnoticed. Does not enter nested function bodies.
fn body_has_poison(body: &[Statement], poison: PoisonFn) -> bool {
    crate::ast::stmts_contain_matching(body, &poison)
        || stmts_contain(body, &|stmt| match stmt {
            Statement::With(..) => true,
            Statement::Variable(decl) => declaration_has_poison(decl, poison),
            Statement::ForIn(f) => for_in_of_left_has_poison(&f.left, poison),
            Statement::ForOf(f) => f.is_await || for_in_of_left_has_poison(&f.left, poison),
            Statement::Try(t) => t
                .handler
                .as_ref()
                .and_then(|h| h.param.as_ref())
                .is_some_and(|p| pattern_has_poison(p, poison)),
            Statement::For(f) => matches!(
                &f.init,
                Some(ForInit::Variable(decl)) if declaration_has_poison(decl, poison)
            ),
            _ => false,
        })
}

fn contains_tail_call(expr: &Expression) -> bool {
    match expr {
        Expression::Call(..) | Expression::TaggedTemplate(..) => true,
        Expression::Conditional(_, consequent, alternate) => {
            contains_tail_call(consequent) || contains_tail_call(alternate)
        }
        Expression::Logical(_, _, rhs) => contains_tail_call(rhs),
        Expression::Sequence(exprs) | Expression::Comma(exprs) => {
            exprs.last().is_some_and(contains_tail_call)
        }
        _ => false,
    }
}

fn ends_with_return(code: &[u8], last_op_at: usize) -> bool {
    matches!(
        code.get(last_op_at).copied().and_then(Op::from_u8),
        Some(Op::Return) | Some(Op::ReturnUndefined)
    )
}

/// Names the statement kind a bail was blamed on. A label reading only
/// "statement" cannot tell an eligibility expansion which construct to reach
/// for next (issue #524), so the catch-all arm names the variant. Exhaustive on
/// purpose: a new AST variant fails to compile here until it is classified.
fn statement_kind(node: &Statement) -> &'static str {
    match node {
        Statement::Empty => "statement:Empty",
        Statement::Expression { .. } => "statement:Expression",
        Statement::Block { .. } => "statement:Block",
        Statement::Variable { .. } => "statement:Variable",
        Statement::If { .. } => "statement:If",
        Statement::While { .. } => "statement:While",
        Statement::DoWhile { .. } => "statement:DoWhile",
        Statement::For { .. } => "statement:For",
        Statement::ForIn { .. } => "statement:ForIn",
        Statement::ForOf { .. } => "statement:ForOf",
        Statement::Return { .. } => "statement:Return",
        Statement::Break { .. } => "statement:Break",
        Statement::Continue { .. } => "statement:Continue",
        Statement::Throw { .. } => "statement:Throw",
        Statement::Try { .. } => "statement:Try",
        Statement::Switch { .. } => "statement:Switch",
        Statement::Labeled { .. } => "statement:Labeled",
        Statement::With { .. } => "statement:With",
        Statement::Debugger => "statement:Debugger",
        Statement::FunctionDeclaration { .. } => "statement:FunctionDeclaration",
        Statement::ClassDeclaration { .. } => "statement:ClassDeclaration",
    }
}

/// The expression counterpart of [`statement_kind`], and exhaustive for the
/// same reason.
fn expression_kind(node: &Expression) -> &'static str {
    match node {
        Expression::Literal { .. } => "expression:Literal",
        Expression::Identifier { .. } => "expression:Identifier",
        Expression::This => "expression:This",
        Expression::Super => "expression:Super",
        Expression::Array { .. } => "expression:Array",
        Expression::Object { .. } => "expression:Object",
        Expression::Function { .. } => "expression:Function",
        Expression::ArrowFunction { .. } => "expression:ArrowFunction",
        Expression::Class { .. } => "expression:Class",
        Expression::Unary { .. } => "expression:Unary",
        Expression::Binary { .. } => "expression:Binary",
        Expression::Logical { .. } => "expression:Logical",
        Expression::Update { .. } => "expression:Update",
        Expression::Assign { .. } => "expression:Assign",
        Expression::Conditional { .. } => "expression:Conditional",
        Expression::Call { .. } => "expression:Call",
        Expression::New { .. } => "expression:New",
        Expression::Member { .. } => "expression:Member",
        Expression::OptionalChain { .. } => "expression:OptionalChain",
        Expression::Comma { .. } => "expression:Comma",
        Expression::Spread { .. } => "expression:Spread",
        Expression::Yield { .. } => "expression:Yield",
        Expression::Await { .. } => "expression:Await",
        Expression::TaggedTemplate { .. } => "expression:TaggedTemplate",
        Expression::Template { .. } => "expression:Template",
        Expression::Typeof { .. } => "expression:Typeof",
        Expression::Void { .. } => "expression:Void",
        Expression::Delete { .. } => "expression:Delete",
        Expression::Sequence { .. } => "expression:Sequence",
        Expression::Import { .. } => "expression:Import",
        Expression::ImportDefer { .. } => "expression:ImportDefer",
        Expression::ImportSource { .. } => "expression:ImportSource",
        Expression::ImportMeta => "expression:ImportMeta",
        Expression::NewTarget => "expression:NewTarget",
        Expression::PrivateIdentifier { .. } => "expression:PrivateIdentifier",
        Expression::BoundCallee { .. } => "expression:BoundCallee",
        Expression::PreIterated { .. } => "expression:PreIterated",
    }
}

/// Function-goal compile of a statement list. Per-node fallback is only
/// worth taking when some native work remains in the chunk; a body that is
/// nothing but tree-walker hand-offs would pay chunk-entry cost for nothing.
/// Callers pass plain function bodies only (see `is_poison_expression`).
fn compile_function_goal(
    body: &[Statement],
    locals: Vec<String>,
    generator: bool,
) -> Result<Chunk, CompileError> {
    if body_has_poison(body, is_poison_expression) {
        return Err(CompileError::Ineligible(
            "construct needs tree-walked frame",
        ));
    }
    let mut c = if generator {
        Compiler::new_generator()
    } else {
        Compiler::new(CompileGoal::Function)
    };
    c.locals = locals;
    c.compile_statements(body, true, true)?;
    if !c.ast.is_empty() && c.native_ops == 0 {
        return Err(CompileError::Ineligible("fallback-only body"));
    }
    let has_function_declaration = stmts_contain(body, &|s| is_function_declaration(s));
    let top_level_lexicals = if has_function_declaration {
        None
    } else {
        lexical_only_instantiation(&mut c, body)
    };
    let mut chunk = c.finish();
    chunk.needs_instantiation = block_declares_lexical(body) || has_function_declaration;
    chunk.top_level_lexicals = top_level_lexicals;
    Ok(chunk)
}

/// The body's top-level lexical names as `names` indices, when declaration
/// instantiation would do nothing else: the caller has ruled out function
/// declarations (so no Annex B name either), and every `var` name the tree-walker would hoist is
/// one the chunk's own var prologue declares. `None` keeps the generic path.
fn lexical_only_instantiation(c: &mut Compiler, body: &[Statement]) -> Option<Vec<(u16, bool)>> {
    let lexicals = crate::interpreter::hoisting::collect_lexical_names(body);
    if lexicals.is_empty() {
        return None;
    }
    let mut vars = std::collections::HashSet::new();
    crate::interpreter::Interpreter::collect_var_names_from_stmts(body, &mut vars);
    let prologue: std::collections::HashSet<&str> = c
        .var_names
        .iter()
        .map(|&idx| c.names[usize::from(idx)].as_str())
        .collect();
    if !vars.iter().all(|name| prologue.contains(name.as_str())) {
        return None;
    }
    lexicals
        .iter()
        .map(|(name, is_const)| c.add_name(name).map(|idx| (idx, *is_const)))
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

#[cfg(test)]
pub(crate) fn compile_body(body: &[Statement]) -> Result<Chunk, CompileError> {
    compile_function_goal(body, Vec::new(), false)
}

/// What the call path will have put in the frame before the chunk runs: the
/// `arguments` binding when the call declares it, then the simple parameters
/// in order. Names the body `var`-declares follow in first-use order.
pub(crate) struct FrameShape<'a> {
    pub(crate) arguments_slot: bool,
    pub(crate) simple_params: &'a [String],
}

/// Compiles a function body, resolving the frame's own names through local
/// slot ops. The first pass finds the names the chunk's var prologue will
/// declare; the second emits `LoadLocal`/`StoreLocal`/`UpdateLocal` for the
/// frame's names. Slots are hints verified at run time (see `Op::LoadLocal`),
/// so a frame laid out differently only costs the by-name fallback.
pub(crate) fn compile_function_body(
    body: &[Statement],
    shape: &FrameShape<'_>,
) -> Result<Chunk, CompileError> {
    compile_function_body_as(body, shape, false)
}

/// [`compile_function_body`] for a sync generator body, whose `yield`s
/// suspend the chunk (`Op::Yield`). Anything that would have to suspend from
/// inside the tree-walker, or that the generator driver cannot host (`yield*`,
/// `using` declarations), is `Ineligible` and keeps the state machine.
pub(crate) fn compile_generator_body(
    body: &[Statement],
    shape: &FrameShape<'_>,
) -> Result<Chunk, CompileError> {
    if stmts_contain(body, &declares_using) {
        return Err(CompileError::Ineligible(
            "construct needs the generator state machine",
        ));
    }
    compile_function_body_as(body, shape, true)
}

fn compile_function_body_as(
    body: &[Statement],
    shape: &FrameShape<'_>,
    generator: bool,
) -> Result<Chunk, CompileError> {
    let mut first = compile_function_goal(body, Vec::new(), generator)?;
    let mut locals: Vec<String> = Vec::new();
    let mut push = |name: &str| {
        if !locals.iter().any(|n| n == name) {
            locals.push(name.to_string());
        }
    };
    if shape.arguments_slot {
        push("arguments");
    }
    for param in shape.simple_params {
        push(param);
    }
    for &idx in &first.var_names {
        push(first.names[idx as usize].as_str());
    }
    if locals.is_empty() {
        first.trivial_private_getter = first.detect_trivial_private_getter();
        return Ok(first);
    }
    let mut chunk = compile_function_goal(body, locals, generator)?;
    chunk.trivial_private_getter = chunk.detect_trivial_private_getter();
    Ok(chunk)
}

/// Script-goal compile. Like a function body, a Script falls back per node
/// (and stays whole on the tree-walker when nothing in it is native); its
/// function declarations are instantiated by GlobalDeclarationInstantiation
/// before the chunk runs.
pub(crate) fn compile_script_body(body: &[Statement]) -> Result<Chunk, CompileError> {
    if body_has_poison(body, is_script_poison_expression) {
        return Err(CompileError::Ineligible(
            "construct needs tree-walked frame",
        ));
    }
    let mut c = Compiler::new(CompileGoal::Script);
    c.compile_statements(body, true, false)?;
    if !c.ast.is_empty() && c.native_ops == 0 {
        return Err(CompileError::Ineligible("fallback-only body"));
    }
    Ok(c.finish())
}
