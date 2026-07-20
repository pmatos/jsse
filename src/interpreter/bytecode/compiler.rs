use super::chunk::{Chunk, Constant};
use super::op::Op;
use crate::ast::{
    AssignOp, BinaryOp, Expression, ForInit, Literal, LogicalOp, Pattern, Statement, UnaryOp,
    UpdateOp, VarKind, VariableDeclaration,
};

#[derive(Debug)]
pub(crate) enum CompileError {
    /// The string carries a construction-site label for greppable
    /// documentation and to surface in `{:?}` traces when debugging the
    /// eligibility membrane. Callers only ever match `Err(_)`, hence the
    /// allow.
    #[allow(dead_code)]
    Unsupported(&'static str),
}

struct Compiler {
    code: Vec<u8>,
    constants: Vec<Constant>,
    names: Vec<std::rc::Rc<str>>,
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
}

impl Compiler {
    fn new() -> Self {
        Self {
            code: Vec::new(),
            constants: Vec::new(),
            names: Vec::new(),
            var_names: Vec::new(),
            current_stack: 0,
            max_stack: 0,
            current_refs: 0,
            max_refs: 0,
            max_jump_target: 0,
        }
    }

    fn add_constant(&mut self, c: Constant) -> Result<u16, CompileError> {
        let idx = self.constants.len();
        if idx > u16::MAX as usize {
            return Err(CompileError::Unsupported("constant pool overflow"));
        }
        self.constants.push(c);
        Ok(idx as u16)
    }

    fn add_name(&mut self, name: &str) -> Result<u16, CompileError> {
        if let Some(i) = self.names.iter().position(|n| n.as_ref() == name) {
            return Ok(i as u16);
        }
        let idx = self.names.len();
        if idx > u16::MAX as usize {
            return Err(CompileError::Unsupported("name pool overflow"));
        }
        self.names.push(std::rc::Rc::from(name));
        Ok(idx as u16)
    }

    fn emit(&mut self, op: Op) {
        self.code.push(op as u8);
    }

    fn emit_u16(&mut self, n: u16) {
        self.code.push((n & 0xff) as u8);
        self.code.push((n >> 8) as u8);
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
            return Err(CompileError::Unsupported("jump offset overflow"));
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
            return Err(CompileError::Unsupported("jump offset overflow"));
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
            _ => Err(CompileError::Unsupported("binary op")),
        }
    }

    fn compound_binary_op(op: AssignOp) -> Result<Op, CompileError> {
        let binary = match op {
            AssignOp::AddAssign => BinaryOp::Add,
            AssignOp::SubAssign => BinaryOp::Sub,
            AssignOp::MulAssign => BinaryOp::Mul,
            AssignOp::DivAssign => BinaryOp::Div,
            AssignOp::ModAssign => BinaryOp::Mod,
            AssignOp::ExpAssign => BinaryOp::Exp,
            AssignOp::LShiftAssign => BinaryOp::LShift,
            AssignOp::RShiftAssign => BinaryOp::RShift,
            AssignOp::URShiftAssign => BinaryOp::URShift,
            AssignOp::BitAndAssign => BinaryOp::BitAnd,
            AssignOp::BitOrAssign => BinaryOp::BitOr,
            AssignOp::BitXorAssign => BinaryOp::BitXor,
            _ => return Err(CompileError::Unsupported("assignment op")),
        };
        Self::binary_op(binary)
    }

    fn compile_expr(&mut self, expr: &Expression) -> Result<(), CompileError> {
        match expr {
            Expression::Literal(lit) => self.compile_literal(lit),
            Expression::Identifier(name) => {
                let idx = self.add_name(name)?;
                self.emit(Op::LoadName);
                self.emit_u16(idx);
                self.push_n(1);
                Ok(())
            }
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
            Expression::Assign(op, target, value) => {
                let Expression::Identifier(name) = target.as_ref() else {
                    return Err(CompileError::Unsupported("assign target"));
                };
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
            Expression::Update(op, prefix, target) => {
                let Expression::Identifier(name) = target.as_ref() else {
                    return Err(CompileError::Unsupported("update target"));
                };
                let idx = self.add_name(name)?;
                self.emit(Op::UpdateName);
                self.emit_u16(idx);
                let mode = match (op, prefix) {
                    (UpdateOp::Increment, false) => 0,
                    (UpdateOp::Increment, true) => 1,
                    (UpdateOp::Decrement, false) => 2,
                    (UpdateOp::Decrement, true) => 3,
                };
                self.code.push(mode);
                self.push_n(1);
                Ok(())
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
            _ => Err(CompileError::Unsupported("expression")),
        }
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
                self.emit_resolve_name(idx);
                self.compile_expr(init)?;
                self.emit_store_resolved_name(idx);
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
                let s = String::from_utf16_lossy(units);
                let idx = self.add_constant(Constant::String(s.into()))?;
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
            _ => Err(CompileError::Unsupported("literal")),
        }
    }

    fn compile_statement(&mut self, stmt: &Statement) -> Result<(), CompileError> {
        match stmt {
            Statement::Empty => Ok(()),
            Statement::Expression(expr) => {
                self.compile_expr(expr)?;
                self.emit(Op::Pop);
                self.pop_n(1);
                Ok(())
            }
            Statement::Block(body) => {
                // A block is statement-level and net-zero on the stack; each
                // contained statement balances itself. Any unsupported nested
                // statement propagates the error so the whole body bails.
                for s in body {
                    self.compile_statement(s)?;
                }
                Ok(())
            }
            Statement::Variable(decl) => self.compile_var_declaration(decl),
            Statement::If(if_stmt) => {
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
            Statement::While(while_stmt) => {
                let loop_start = self.code.len();
                self.compile_expr(&while_stmt.test)?;
                self.pop_n(1);
                let exit = self.emit_jump(Op::JumpIfFalse);
                self.compile_statement(&while_stmt.body)?;
                debug_assert_eq!(self.current_stack, 0);
                debug_assert_eq!(self.current_refs, 0);
                self.emit_jump_to(Op::Jump, loop_start)?;
                self.patch_jump(exit)?;
                Ok(())
            }
            Statement::For(for_stmt) => {
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
                let exit = if let Some(test) = &for_stmt.test {
                    self.compile_expr(test)?;
                    self.pop_n(1);
                    Some(self.emit_jump(Op::JumpIfFalse))
                } else {
                    None
                };
                self.compile_statement(&for_stmt.body)?;
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
                Ok(())
            }
            Statement::Return(None) => {
                self.emit(Op::ReturnUndefined);
                Ok(())
            }
            Statement::Return(Some(expr)) => {
                self.compile_expr(expr)?;
                self.emit(Op::Return);
                self.pop_n(1);
                Ok(())
            }
            _ => Err(CompileError::Unsupported("statement")),
        }
    }

    fn finish(mut self) -> Chunk {
        // Emit a trailing `ReturnUndefined` unless the chunk already ends in a
        // return AND no branch falls through to the end. A forward jump whose
        // target is the end of the chunk (e.g. the false arm of a one-armed
        // `if` whose consequent ends in `return`) would otherwise run `pc` past
        // the last byte of `code` and panic in the VM dispatch loop.
        if !ends_with_return(&self.code) || self.max_jump_target >= self.code.len() {
            self.emit(Op::ReturnUndefined);
        }
        Chunk {
            code: self.code,
            constants: self.constants,
            names: self.names,
            var_names: self.var_names,
            max_stack: self.max_stack,
            max_refs: self.max_refs,
        }
    }
}

fn ends_with_return(code: &[u8]) -> bool {
    matches!(
        code.last().copied().and_then(Op::from_u8),
        Some(Op::Return) | Some(Op::ReturnUndefined)
    )
}

pub(crate) fn compile_body(body: &[Statement]) -> Result<Chunk, CompileError> {
    let mut c = Compiler::new();
    for stmt in body {
        c.compile_statement(stmt)?;
    }
    Ok(c.finish())
}
