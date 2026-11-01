//! A binary operator expression, with the `--node` error position of the
//! operator (#1511): `Chunk::binary_pos` maps the offset after the op's
//! instruction (a fused one too) to the operator's source position, which the
//! VM reads when the op throws.

use super::{CompileError, Compiler};
use crate::ast::{BinaryOp, Expression, Literal};
use crate::interpreter::bytecode::op::Op;

impl Compiler {
    /// `pos` is the operator's source offset plus one (0 outside `--node`).
    pub(super) fn compile_binary(
        &mut self,
        op: BinaryOp,
        lhs: &Expression,
        rhs: &Expression,
        pos: u32,
    ) -> Result<(), CompileError> {
        let bop = Self::binary_op(op)?;
        if !matches!(bop, Op::Instanceof | Op::In)
            && let Expression::Literal(Literal::Number(k)) = rhs
        {
            // The literal folds into the op: no `LoadConst` dispatch.
            // The left operand is still evaluated first and the
            // constant has no effects, so evaluation order holds.
            if let Expression::Identifier(name) = lhs
                && let Some(slot) = self.local_slot(name)
            {
                self.emit_local_bin_const(name, slot, bop, *k)?;
            } else {
                self.compile_expr(lhs)?;
                self.emit_bin_const(bop, *k)?;
            }
            self.note_binary_pos(pos);
            return Ok(());
        }
        if !matches!(bop, Op::Instanceof | Op::In)
            && let Some((a, b)) = self.local_pair(lhs, rhs)
        {
            self.emit_local_local_bin(a, b, bop);
            self.note_binary_pos(pos);
            return Ok(());
        }
        self.compile_expr(lhs)?;
        self.compile_expr(rhs)?;
        self.emit(bop);
        self.note_binary_pos(pos);
        self.pop_n(2);
        self.push_n(1);
        Ok(())
    }

    pub(super) fn note_binary_pos(&mut self, pos: u32) {
        if pos != 0 {
            let end = u32::try_from(self.code.len()).unwrap_or(u32::MAX);
            while self.binary_pos.last().is_some_and(|entry| entry.0 >= end) {
                self.binary_pos.pop();
            }
            self.binary_pos.push((end, pos));
        }
    }
}
