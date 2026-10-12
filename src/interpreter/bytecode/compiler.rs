use super::chunk::{AstNode, Chunk, Constant, EscapeEntry, NO_ESCAPE_TABLE};
use super::op::Op;
use crate::ast::{
    AssignOp, BinaryOp, CallSiteId, DoWhileStatement, Expression, ForInit, ForStatement, Literal,
    LogicalOp, MemberProperty, Pattern, Statement, UnaryOp, UpdateOp, VarKind, VariableDeclaration,
    WhileStatement,
};
use crate::types::{JsBigInt, JsString};

#[derive(Debug)]
pub(crate) enum CompileError {
    /// This node has no native lowering. In Function goal the node itself is
    /// handed to the tree-walker (`EvalAst`/`ExecAst`); in Script goal the
    /// whole body bails. The string carries a construction-site label for
    /// greppable documentation, `{:?}` traces when debugging the eligibility
    /// membrane, and the counters build's BAIL/FALLBACK tables.
    Unsupported(&'static str),
    /// The whole body must stay on the tree-walker: a construct whose
    /// semantics depend on running as one tree-walked frame (direct `eval`,
    /// `with`, `super`, ...), or a compiler resource limit.
    Ineligible(&'static str),
}

impl CompileError {
    pub(crate) fn reason(&self) -> &'static str {
        match self {
            CompileError::Unsupported(reason) | CompileError::Ineligible(reason) => reason,
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
}

impl LoopFrame {
    fn new(labels: Vec<String>, continue_target: Option<usize>) -> Self {
        Self {
            labels,
            continue_target,
            continue_sites: Vec::new(),
            break_sites: Vec::new(),
            escape_breaks: Vec::new(),
            escape_continues: Vec::new(),
        }
    }
}

/// Compiler state saved before compiling a node, so a node that turns out to
/// need the tree-walker can be rolled back cleanly and emitted as one
/// `EvalAst`/`ExecAst` instead.
struct Snapshot {
    code_len: usize,
    current_stack: u16,
    current_refs: u16,
    frames: Vec<(usize, usize, usize, usize)>,
    ast_len: usize,
    tables_len: usize,
    native_ops: u32,
}

struct Compiler {
    goal: CompileGoal,
    code: Vec<u8>,
    constants: Vec<Constant>,
    names: Vec<crate::interpreter::Atom>,
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
    /// end via `resolve_loop_frame`.
    loop_frames: Vec<LoopFrame>,
    /// Frame binding order for a function body whose locals are known: slot
    /// `i` is the expected position of that name in the call's binding
    /// vector. Empty when no local ops are emitted.
    locals: Vec<String>,
    ast: Vec<AstNode>,
    escape_tables: Vec<Vec<EscapeEntry>>,
    /// Ops other than bookkeeping and tree-walker hand-offs. A chunk with none
    /// would pay chunk-entry cost for nothing.
    native_ops: u32,
    fallback_reasons: Vec<&'static str>,
}

impl Compiler {
    fn new(goal: CompileGoal) -> Self {
        Self {
            goal,
            locals: Vec::new(),
            code: Vec::new(),
            constants: Vec::new(),
            names: Vec::new(),
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
        }
    }

    fn add_constant(&mut self, c: Constant) -> Result<u16, CompileError> {
        let idx = self.constants.len();
        if idx > u16::MAX as usize {
            return Err(CompileError::Ineligible("constant pool overflow"));
        }
        self.constants.push(c);
        Ok(idx as u16)
    }

    fn add_name(&mut self, name: &str) -> Result<u16, CompileError> {
        if let Some(i) = self.names.iter().position(|n| n.as_str() == name) {
            return Ok(i as u16);
        }
        let idx = self.names.len();
        if idx > u16::MAX as usize {
            return Err(CompileError::Ineligible("name pool overflow"));
        }
        self.names.push(crate::interpreter::Atom::intern(name));
        Ok(idx as u16)
    }

    fn emit(&mut self, op: Op) {
        if !matches!(
            op,
            Op::Safepoint
                | Op::EvalAst
                | Op::ExecAst
                | Op::Pop
                | Op::Jump
                | Op::Return
                | Op::ReturnUndefined
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
            return Err(CompileError::Ineligible("jump offset overflow"));
        }
        self.emit_u16(delta as i16 as u16);
        Ok(())
    }

    fn patch_jump(&mut self, patch: usize) -> Result<(), CompileError> {
        // Offset is from the byte AFTER the 2-byte operand to the current code length.
        let from = patch + 2;
        let to = self.code.len();
        self.max_jump_target = self.max_jump_target.max(to);
        let delta = to as isize - from as isize;
        if !(i16::MIN as isize..=i16::MAX as isize).contains(&delta) {
            return Err(CompileError::Ineligible("jump offset overflow"));
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

    fn emit_store_resolved_name(&mut self, name_idx: u16) {
        self.emit(Op::StoreResolvedName);
        self.emit_u16(name_idx);
        self.pop_ref();
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

    fn local_slot(&self, name: &str) -> Option<u16> {
        self.locals
            .iter()
            .position(|n| n == name)
            .and_then(|i| u16::try_from(i).ok())
    }

    fn emit_store_local(&mut self, name_idx: u16, slot: u16) {
        self.emit(Op::StoreLocal);
        self.emit_u16(name_idx);
        self.emit_u16(slot);
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
        }
    }

    fn rollback(&mut self, snap: Snapshot) {
        self.code.truncate(snap.code_len);
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
    }

    fn add_ast_node(&mut self, node: AstNode, reason: &'static str) -> Result<u16, CompileError> {
        let idx = self.ast.len();
        if idx >= usize::from(NO_ESCAPE_TABLE) {
            return Err(CompileError::Ineligible("ast node pool overflow"));
        }
        self.ast.push(node);
        self.fallback_reasons.push(reason);
        Ok(idx as u16)
    }

    /// Compiles `expr`; when it has no native lowering (and can be evaluated
    /// standalone), the whole node is handed to the tree-walker instead. Only
    /// Function goal falls back per node — the Script goal still bails whole.
    fn compile_expr(&mut self, expr: &Expression) -> Result<(), CompileError> {
        if self.goal != CompileGoal::Function {
            return self.compile_expr_native(expr);
        }
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
                    self.compile_expr(e)?;
                    if i != last {
                        self.emit(Op::Pop);
                        self.pop_n(1);
                    }
                }
                Ok(())
            }
            // NamedEvaluation (sec-runtime-semantics-namedevaluation) is applied
            // by the assignment, not by the function expression, so an
            // anonymous function/class value needs the tree-walker's logic.
            Expression::Assign(_, target, value)
                if matches!(target.as_ref(), Expression::Identifier(_))
                    && value.is_anonymous_function_definition() =>
            {
                Err(CompileError::Unsupported("anonymous function assignment"))
            }
            Expression::Assign(op, target, value) => match target.as_ref() {
                Expression::Identifier(name) if self.local_slot(name).is_some() => {
                    // A local of this frame: no `with`, closure or direct eval
                    // can change what the name resolves to while the
                    // right-hand side runs, so the reference needs no early
                    // resolution.
                    let idx = self.add_name(name)?;
                    let slot = self.local_slot(name).expect("checked by the guard");
                    if *op != AssignOp::Assign {
                        self.emit(Op::LoadLocal);
                        self.emit_u16(idx);
                        self.emit_u16(slot);
                        self.push_n(1);
                    }
                    self.compile_expr(value)?;
                    if *op != AssignOp::Assign {
                        self.emit(Self::compound_binary_op(*op)?);
                        self.pop_n(2);
                        self.push_n(1);
                    }
                    self.emit_store_local(idx, slot);
                    Ok(())
                }
                Expression::Identifier(name) => {
                    let idx = self.add_name(name)?;
                    self.emit_resolve_name(idx);
                    if *op == AssignOp::Assign {
                        self.compile_expr(value)?;
                    } else {
                        self.emit(Op::LoadResolvedName);
                        self.emit_u16(idx);
                        self.push_n(1);
                        self.compile_expr(value)?;
                        self.emit(Self::compound_binary_op(*op)?);
                        self.pop_n(2);
                        self.push_n(1);
                    }
                    self.emit_store_resolved_name(idx);
                    Ok(())
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
                    MemberProperty::Private(_) => Err(CompileError::Unsupported("private field")),
                },
                _ => Err(CompileError::Unsupported("assign target")),
            },
            Expression::Update(op, prefix, target) => {
                let mode = match (op, prefix) {
                    (UpdateOp::Increment, false) => 0,
                    (UpdateOp::Increment, true) => 1,
                    (UpdateOp::Decrement, false) => 2,
                    (UpdateOp::Decrement, true) => 3,
                };
                match target.as_ref() {
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
                        self.code.push(mode);
                        self.push_n(1);
                        Ok(())
                    }
                    Expression::Member(obj, MemberProperty::Dot(name), _) => {
                        self.compile_expr(obj)?;
                        let idx = self.add_name(name)?;
                        self.emit(Op::UpdateProp);
                        self.emit_u16(idx);
                        self.code.push(mode);
                        // Stack height unchanged: pop base, push result.
                        Ok(())
                    }
                    Expression::Member(obj, MemberProperty::Computed(key), _) => {
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
            Expression::Logical(op, lhs, rhs) => {
                self.compile_expr(lhs)?;
                // lhs stays on stack via *Keep variants
                let short_circuit_op = match op {
                    LogicalOp::And => Op::JumpIfFalsyKeep,
                    LogicalOp::Or => Op::JumpIfTruthyKeep,
                    LogicalOp::NullishCoalescing => Op::JumpIfNotNullishKeep,
                };
                let to_end = self.emit_jump(short_circuit_op);
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
                self.compile_expr(test)?;
                self.pop_n(1);
                let to_else = self.emit_jump(Op::JumpIfFalse);
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
                self.compile_expr(operand)?;
                self.emit(Op::Pop);
                self.pop_n(1);
                self.emit(Op::LoadUndefined);
                self.push_n(1);
                Ok(())
            }
            Expression::Binary(op, lhs, rhs) => {
                self.compile_expr(lhs)?;
                self.compile_expr(rhs)?;
                self.emit(Self::binary_op(*op)?);
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
                    // Stack height unchanged: pop base, push value.
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
                MemberProperty::Private(_) => Err(CompileError::Unsupported("private field")),
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
            _ => Err(CompileError::Unsupported(expression_kind(expr))),
        }
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
            _ => return Err(CompileError::Unsupported("call callee")),
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

    fn compile_var_declaration(&mut self, decl: &VariableDeclaration) -> Result<(), CompileError> {
        if decl.kind != VarKind::Var {
            return Err(CompileError::Unsupported("lexical declaration"));
        }
        for declarator in &decl.declarations {
            let Pattern::Identifier(name) = &declarator.pattern else {
                return Err(CompileError::Unsupported("var binding pattern"));
            };
            let idx = self.add_var_name(name)?;
            if let Some(init) = &declarator.init {
                if init.is_anonymous_function_definition() {
                    return Err(CompileError::Unsupported("anonymous function initializer"));
                }
                if let Some(slot) = self.local_slot(name) {
                    self.compile_expr(init)?;
                    self.emit_store_local(idx, slot);
                } else {
                    self.emit_resolve_name(idx);
                    self.compile_expr(init)?;
                    self.emit_store_resolved_name(idx);
                }
                self.emit(Op::Pop);
                self.pop_n(1);
            }
        }
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
            Literal::BigInt(text) => {
                let value = crate::interpreter::helpers::parse_bigint_literal(text);
                let idx = self.add_constant(Constant::BigInt(JsBigInt::new(value)))?;
                self.emit(Op::LoadConst);
                self.emit_u16(idx);
                self.push_n(1);
                Ok(())
            }
            _ => Err(CompileError::Unsupported("literal")),
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
        self.compile_statements(body, false)
    }

    /// `skip_hoisted` is the function top level: its function declarations
    /// are instantiated before the chunk runs, so they emit nothing.
    fn compile_statements(
        &mut self,
        body: &[Statement],
        skip_hoisted: bool,
    ) -> Result<(), CompileError> {
        for stmt in body {
            if skip_hoisted && matches!(stmt, Statement::FunctionDeclaration(_)) {
                continue;
            }
            debug_assert_eq!(self.current_stack, 0);
            debug_assert_eq!(self.current_refs, 0);
            let safepoint_at = self.code.len();
            self.emit(Op::Safepoint);
            self.compile_statement(stmt)?;
            if self.code.len() == safepoint_at + 1 {
                self.code.truncate(safepoint_at);
            }
        }
        Ok(())
    }

    /// Compiles `stmt`; a statement with no native lowering is handed to the
    /// tree-walker as one unit (`ExecAst`) in Function goal.
    fn compile_statement(&mut self, stmt: &Statement) -> Result<(), CompileError> {
        if self.goal != CompileGoal::Function {
            return self.compile_statement_native(stmt);
        }
        let snap = self.snapshot();
        match self.compile_statement_native(stmt) {
            Err(CompileError::Unsupported(reason)) => {
                self.rollback(snap);
                self.emit_exec_ast(stmt, reason)
            }
            other => other,
        }
    }

    fn emit_exec_ast(
        &mut self,
        stmt: &Statement,
        reason: &'static str,
    ) -> Result<(), CompileError> {
        debug_assert_eq!(self.current_stack, 0);
        debug_assert_eq!(self.current_refs, 0);
        let mut var_names = std::collections::HashSet::new();
        crate::interpreter::Interpreter::collect_var_names_from_stmts(
            std::slice::from_ref(stmt),
            &mut var_names,
        );
        let mut var_names: Vec<String> = var_names.into_iter().collect();
        var_names.sort();
        for name in &var_names {
            self.add_var_name(name)?;
        }
        let table = if stmt_contains(stmt, &|s| {
            matches!(s, Statement::Break(_) | Statement::Continue(_))
        }) {
            self.build_escape_table()?
        } else {
            NO_ESCAPE_TABLE
        };
        let idx = self.add_ast_node(AstNode::Stmt(stmt.clone()), reason)?;
        self.emit(Op::ExecAst);
        self.emit_u16(idx);
        self.emit_u16(table);
        Ok(())
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
        for frame_idx in (0..frame_count).rev() {
            let mut labels: Vec<Option<String>> = Vec::new();
            if frame_idx == frame_count - 1 {
                labels.push(None);
            }
            labels.extend(self.loop_frames[frame_idx].labels.iter().cloned().map(Some));
            let continue_target = self.loop_frames[frame_idx].continue_target;
            for label in labels {
                self.loop_frames[frame_idx]
                    .escape_breaks
                    .push((table_idx, entries.len()));
                entries.push(EscapeEntry {
                    is_continue: false,
                    label: label.clone(),
                    target: u32::MAX,
                });
                match continue_target {
                    Some(target) => entries.push(EscapeEntry {
                        is_continue: true,
                        label,
                        target: target as u32,
                    }),
                    None => {
                        self.loop_frames[frame_idx]
                            .escape_continues
                            .push((table_idx, entries.len()));
                        entries.push(EscapeEntry {
                            is_continue: true,
                            label,
                            target: u32::MAX,
                        });
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
        match stmt {
            Statement::Empty => Ok(()),
            Statement::Expression(expr) => {
                self.compile_expr(expr)?;
                self.emit(if self.goal == CompileGoal::Script {
                    Op::SetCompletion
                } else {
                    Op::Pop
                });
                self.pop_n(1);
                Ok(())
            }
            Statement::Block(body) => {
                // A block is statement-level and net-zero on the stack; each
                // contained statement balances itself. The VM has no block
                // environments, so a block that declares a binding runs as
                // one unit on the tree-walker (BlockDeclarationInstantiation,
                // sec-blockdeclarationinstantiation).
                if declares_lexical(body) {
                    return Err(CompileError::Unsupported("block-scoped declaration"));
                }
                self.compile_statement_list(body)
            }
            Statement::Variable(decl) => self.compile_var_declaration(decl),
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
                self.compile_expr(&if_stmt.test)?;
                self.pop_n(1); // JumpIfFalse consumes the test value
                let else_target = self.emit_jump(Op::JumpIfFalse);
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
                self.emit(Op::ReturnUndefined);
                Ok(())
            }
            Statement::Return(Some(Expression::Call(callee, args, site_id))) => {
                self.compile_call(callee, args, Op::ReturnCall, *site_id)?;
                self.emit(Op::Return);
                self.pop_n(1);
                Ok(())
            }
            Statement::Return(Some(expr)) => {
                if contains_tail_call(expr) {
                    return Err(CompileError::Unsupported("nested tail call"));
                }
                self.compile_expr(expr)?;
                self.emit(Op::Return);
                self.pop_n(1);
                Ok(())
            }
            _ => Err(CompileError::Unsupported(statement_kind(stmt))),
        }
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

    /// Resolves `break`/`continue`'s target loop frame. Unlabeled forms bind
    /// to the innermost (last-pushed) frame; labeled forms search outward so
    /// a label on an outer loop is reachable past an intervening unlabeled
    /// inner loop. The parser's early-error checks
    /// (`sec-continue-statement-static-semantics-containsundefinedcontinuetarget`,
    /// and `break`'s analogous rule) guarantee a labeled form always names an
    /// enclosing loop that compiled successfully — this only returns `None`
    /// for a label that targets a non-loop statement or an unlabeled form
    /// with no enclosing loop at all, neither of which this compiler should
    /// ever actually reach given its existing eligibility checks, but it's
    /// cheap defensive coding against a future change that stops being true.
    fn resolve_loop_frame(&self, label: Option<&str>) -> Option<usize> {
        match label {
            None => self.loop_frames.len().checked_sub(1),
            Some(name) => self
                .loop_frames
                .iter()
                .rposition(|frame| frame.labels.iter().any(|l| l == name)),
        }
    }

    fn compile_break(&mut self, label: Option<&str>) -> Result<(), CompileError> {
        let idx = self
            .resolve_loop_frame(label)
            .ok_or(CompileError::Ineligible("statement:Break"))?;
        debug_assert_eq!(self.current_stack, 0);
        debug_assert_eq!(self.current_refs, 0);
        let site = self.emit_jump(Op::Jump);
        self.loop_frames[idx].break_sites.push(site);
        Ok(())
    }

    fn compile_continue(&mut self, label: Option<&str>) -> Result<(), CompileError> {
        let idx = self
            .resolve_loop_frame(label)
            .ok_or(CompileError::Ineligible("statement:Continue"))?;
        debug_assert_eq!(self.current_stack, 0);
        debug_assert_eq!(self.current_refs, 0);
        match self.loop_frames[idx].continue_target {
            Some(target) => self.emit_jump_to(Op::Jump, target),
            None => {
                let site = self.emit_jump(Op::Jump);
                self.loop_frames[idx].continue_sites.push(site);
                Ok(())
            }
        }
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
        self.loop_frames
            .push(LoopFrame::new(labels, Some(loop_start)));
        let exit = if Self::test_always_true(Some(&while_stmt.test)) {
            None
        } else {
            self.compile_expr(&while_stmt.test)?;
            self.pop_n(1);
            Some(self.emit_jump(Op::JumpIfFalse))
        };
        self.compile_statement(&while_stmt.body)?;
        debug_assert_eq!(self.current_stack, 0);
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
        self.loop_frames.push(LoopFrame::new(labels, None));
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
            self.compile_expr(&do_while_stmt.test)?;
            self.pop_n(1);
            let exit = self.emit_jump(Op::JumpIfFalse);
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
        if let Some(init) = &for_stmt.init {
            match init {
                ForInit::Variable(decl) => self.compile_var_declaration(decl)?,
                ForInit::Expression(expr) => {
                    self.compile_expr(expr)?;
                    self.emit(Op::Pop);
                    self.pop_n(1);
                }
            }
        }
        let loop_start = self.code.len();
        let exit = if Self::test_always_true(for_stmt.test.as_ref()) {
            None
        } else {
            let test = for_stmt
                .test
                .as_ref()
                .expect("test_always_true(None) is true");
            self.compile_expr(test)?;
            self.pop_n(1);
            Some(self.emit_jump(Op::JumpIfFalse))
        };
        self.loop_frames.push(LoopFrame::new(labels, None));
        self.compile_statement(&for_stmt.body)?;
        let frame = self
            .loop_frames
            .pop()
            .expect("pushed above, not re-entrant");
        for site in frame.continue_sites {
            self.patch_jump(site)?;
        }
        self.resolve_escape_targets(&frame.escape_continues);
        if let Some(update) = &for_stmt.update {
            self.compile_expr(update)?;
            self.emit(Op::Pop);
            self.pop_n(1);
        }
        debug_assert_eq!(self.current_stack, 0);
        debug_assert_eq!(self.current_refs, 0);
        self.emit_jump_to(Op::Jump, loop_start)?;
        if let Some(exit) = exit {
            self.patch_jump(exit)?;
        }
        for site in frame.break_sites {
            self.patch_jump(site)?;
        }
        self.resolve_escape_targets(&frame.escape_breaks);
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
            if !ends_with_return(&self.code) || self.max_jump_target >= self.code.len() {
                self.emit(Op::ReturnUndefined);
            }
        }
        Chunk {
            code: self.code,
            constants: self.constants,
            names: self.names,
            var_names: self.var_names,
            max_stack: self.max_stack,
            max_refs: self.max_refs,
            ast: self.ast,
            escape_tables: self.escape_tables,
            needs_instantiation: false,
            fallback_reasons: self.fallback_reasons,
        }
    }
}

/// `Super` and private names only make sense inside their parent expression;
/// `Spread` is an argument/element form, not an expression of its own.
fn is_standalone_expression(expr: &Expression) -> bool {
    !matches!(
        expr,
        Expression::Super | Expression::PrivateIdentifier(_) | Expression::Spread(_)
    )
}

fn is_function_declaration(stmt: &Statement) -> bool {
    matches!(stmt, Statement::FunctionDeclaration(_))
        || matches!(stmt, Statement::Labeled(_, inner) if is_function_declaration(inner))
}

/// Whether a statement list declares a binding a block would scope to itself
/// (`let`/`const`/`using`, class, function declarations).
fn declares_lexical(stmts: &[Statement]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        Statement::Variable(decl) => decl.kind != VarKind::Var,
        Statement::ClassDeclaration(_) => true,
        other => is_function_declaration(other),
    })
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

/// A construct that only has its meaning when the whole function runs as one
/// tree-walked frame, or whose compile-time handling depends on resolving
/// names in the enclosing scope chain. Such a body stays wholly on the
/// tree-walker, exactly as before per-node fallback existed.
fn is_poison_expression(expr: &Expression) -> bool {
    match expr {
        Expression::Super
        | Expression::NewTarget
        | Expression::ImportMeta
        | Expression::Import(..)
        | Expression::ImportDefer(..)
        | Expression::ImportSource(..)
        | Expression::Yield(..)
        | Expression::Await(_) => true,
        Expression::Call(callee, _, _) => {
            matches!(callee.as_ref(), Expression::Identifier(name) if name == "eval")
        }
        Expression::Binary(BinaryOp::In, lhs, _) => {
            matches!(lhs.as_ref(), Expression::PrivateIdentifier(_))
        }
        _ => false,
    }
}

fn pattern_has_poison(pattern: &Pattern) -> bool {
    use crate::ast::{ArrayPatternElement, ObjectPatternProperty, PropertyKey};
    match pattern {
        Pattern::Identifier(_) => false,
        Pattern::Assign(inner, default) => pattern_has_poison(inner) || expr_has_poison(default),
        Pattern::Rest(inner) => pattern_has_poison(inner),
        Pattern::MemberExpression(expr) => expr_has_poison(expr),
        Pattern::Array(elements) => elements.iter().flatten().any(|e| match e {
            ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => pattern_has_poison(p),
        }),
        Pattern::Object(props) => props.iter().any(|p| match p {
            ObjectPatternProperty::KeyValue(key, value) => {
                matches!(key, PropertyKey::Computed(e) if expr_has_poison(e))
                    || pattern_has_poison(value)
            }
            ObjectPatternProperty::Shorthand(_) => false,
            ObjectPatternProperty::Rest(p) => pattern_has_poison(p),
        }),
    }
}

fn expr_has_poison(expr: &Expression) -> bool {
    crate::ast::expr_contains_matching(expr, &is_poison_expression)
}

fn declaration_has_poison(decl: &VariableDeclaration) -> bool {
    decl.declarations
        .iter()
        .any(|d| pattern_has_poison(&d.pattern))
}

fn for_in_of_left_has_poison(left: &crate::ast::ForInOfLeft) -> bool {
    match left {
        crate::ast::ForInOfLeft::Variable(decl) => declaration_has_poison(decl),
        crate::ast::ForInOfLeft::Pattern(p) => pattern_has_poison(p),
        crate::ast::ForInOfLeft::Expression(e) => expr_has_poison(e),
    }
}

/// One-time deep scan, run before anything is emitted: once a parent falls
/// back, its subtree is never visited by the compiler, so poison inside it
/// would otherwise go unnoticed. Does not enter nested function bodies.
fn body_has_poison(body: &[Statement]) -> bool {
    crate::ast::stmts_contain_matching(body, &is_poison_expression)
        || stmts_contain(body, &|stmt| match stmt {
            Statement::With(..) => true,
            Statement::Variable(decl) => declaration_has_poison(decl),
            Statement::ForIn(f) => for_in_of_left_has_poison(&f.left),
            Statement::ForOf(f) => f.is_await || for_in_of_left_has_poison(&f.left),
            Statement::Try(t) => t
                .handler
                .as_ref()
                .and_then(|h| h.param.as_ref())
                .is_some_and(pattern_has_poison),
            Statement::For(f) => matches!(
                &f.init,
                Some(ForInit::Variable(decl)) if declaration_has_poison(decl)
            ),
            _ => false,
        })
}

/// Whether the body needs declaration instantiation before the chunk runs:
/// function declarations (anywhere — Annex B hoists block-level ones) and
/// top-level lexical bindings, which the VM itself never declares.
fn body_needs_instantiation(body: &[Statement]) -> bool {
    declares_lexical(body) || stmts_contain(body, &|s| is_function_declaration(s))
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

fn ends_with_return(code: &[u8]) -> bool {
    matches!(
        code.last().copied().and_then(Op::from_u8),
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
    }
}

/// Function-goal compile of a statement list. Per-node fallback is only
/// worth taking when some native work remains in the chunk; a body that is
/// nothing but tree-walker hand-offs would pay chunk-entry cost for nothing.
fn compile_function_goal(body: &[Statement], locals: Vec<String>) -> Result<Chunk, CompileError> {
    if body_has_poison(body) {
        return Err(CompileError::Ineligible(
            "construct needs tree-walked frame",
        ));
    }
    let mut c = Compiler::new(CompileGoal::Function);
    c.locals = locals;
    c.compile_statements(body, true)?;
    if !c.ast.is_empty() && c.native_ops == 0 {
        return Err(CompileError::Ineligible("fallback-only body"));
    }
    let mut chunk = c.finish();
    chunk.needs_instantiation = body_needs_instantiation(body);
    Ok(chunk)
}

pub(crate) fn compile_body(body: &[Statement]) -> Result<Chunk, CompileError> {
    compile_function_goal(body, Vec::new())
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
    let first = compile_body(body)?;
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
        return Ok(first);
    }
    compile_function_goal(body, locals)
}

pub(crate) fn compile_script_body(body: &[Statement]) -> Result<Chunk, CompileError> {
    let mut c = Compiler::new(CompileGoal::Script);
    c.compile_statement_list(body)?;
    Ok(c.finish())
}
