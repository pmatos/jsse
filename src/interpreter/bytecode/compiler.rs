use super::chunk::{Chunk, Constant};
use super::op::Op;
use crate::ast::{BinaryOp, Expression, Literal, Statement};

#[derive(Debug)]
pub(crate) enum CompileError {
    Unsupported(&'static str),
}

struct Compiler {
    code: Vec<u8>,
    constants: Vec<Constant>,
    current_stack: u16,
    max_stack: u16,
}

impl Compiler {
    fn new() -> Self {
        Self {
            code: Vec::new(),
            constants: Vec::new(),
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

    fn emit(&mut self, op: Op) {
        self.code.push(op as u8);
    }

    fn emit_u16(&mut self, n: u16) {
        self.code.push((n & 0xff) as u8);
        self.code.push((n >> 8) as u8);
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
            Expression::Binary(op, lhs, rhs) => {
                self.compile_expr(lhs)?;
                self.compile_expr(rhs)?;
                let bytecode_op = match op {
                    BinaryOp::Add => Op::Add,
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
            _ => Err(CompileError::Unsupported("literal")),
        }
    }

    fn compile_statement(&mut self, stmt: &Statement) -> Result<(), CompileError> {
        match stmt {
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
