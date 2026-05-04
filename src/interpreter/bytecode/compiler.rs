use super::chunk::{Chunk, Constant};
use super::op::Op;
use crate::ast::{AssignOp, BinaryOp, Expression, Literal, LogicalOp, Statement, UnaryOp};

#[derive(Debug)]
pub(crate) enum CompileError {
    Unsupported(&'static str),
}

struct Compiler {
    code: Vec<u8>,
    constants: Vec<Constant>,
    names: Vec<std::rc::Rc<str>>,
    current_stack: u16,
    max_stack: u16,
}

impl Compiler {
    fn new() -> Self {
        Self {
            code: Vec::new(),
            constants: Vec::new(),
            names: Vec::new(),
            current_stack: 0,
            max_stack: 0,
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

    fn patch_jump(&mut self, patch: usize) -> Result<(), CompileError> {
        // Offset is from the byte AFTER the 2-byte operand to the current code length.
        let from = patch + 2;
        let to = self.code.len();
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
            Expression::Assign(AssignOp::Assign, target, value) => {
                let Expression::Identifier(name) = target.as_ref() else {
                    return Err(CompileError::Unsupported("assign target"));
                };
                let idx = self.add_name(name)?;
                self.compile_expr(value)?;
                self.emit(Op::StoreName);
                self.emit_u16(idx);
                // Stack height unchanged: value remains on stack.
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
                let bytecode_op = match op {
                    BinaryOp::Add => Op::Add,
                    BinaryOp::Sub => Op::Sub,
                    BinaryOp::Mul => Op::Mul,
                    BinaryOp::Div => Op::Div,
                    BinaryOp::Mod => Op::Mod,
                    BinaryOp::Exp => Op::Pow,
                    BinaryOp::Eq => Op::Eq,
                    BinaryOp::NotEq => Op::NotEq,
                    BinaryOp::StrictEq => Op::StrictEq,
                    BinaryOp::StrictNotEq => Op::StrictNotEq,
                    BinaryOp::Lt => Op::Lt,
                    BinaryOp::Gt => Op::Gt,
                    BinaryOp::LtEq => Op::LtEq,
                    BinaryOp::GtEq => Op::GtEq,
                    BinaryOp::BitAnd => Op::BitAnd,
                    BinaryOp::BitOr => Op::BitOr,
                    BinaryOp::BitXor => Op::BitXor,
                    BinaryOp::LShift => Op::Shl,
                    BinaryOp::RShift => Op::Shr,
                    BinaryOp::URShift => Op::UShr,
                    _ => return Err(CompileError::Unsupported("binary op")),
                };
                self.emit(bytecode_op);
                self.pop_n(2);
                self.push_n(1);
                Ok(())
            }
            _ => Err(CompileError::Unsupported("expression")),
        }
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
        if !ends_with_return(&self.code) {
            self.emit(Op::ReturnUndefined);
        }
        Chunk {
            code: self.code,
            constants: self.constants,
            names: self.names,
            max_stack: self.max_stack,
            num_params: 0,
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
