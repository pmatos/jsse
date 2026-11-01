//! Binary operator expressions and the comparison jumps, with the `--node`
//! error position of the operator (#1511): `Chunk::binary_pos` maps the offset
//! after the op's instruction (a fused one too) to the operator's source
//! position, which the VM reads when the op throws.

use super::{CompileError, Compiler, Constant};
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
            if let Expression::Identifier(name, _) = lhs
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

    /// The comparison byte, optional constant and jump offset (last, so
    /// `patch_jump` applies) every compare-and-branch op ends with. Returns the
    /// patch site.
    fn emit_cmp_jump_tail(&mut self, cmp: Op, constant: Option<u16>, pos: u32) -> usize {
        self.code.push(cmp as u8);
        if let Some(k_idx) = constant {
            self.emit_u16(k_idx);
        }
        let patch = self.code.len();
        self.emit_u16(0);
        self.note_binary_pos(pos);
        patch
    }

    /// Compiles `test` and a forward jump taken when it is falsy; returns the
    /// patch site. A comparison fuses into `CmpJumpIfFalse` (its result is
    /// always a Boolean, so ToBoolean is the identity), and one whose operands
    /// are locals, or a local and a number literal, reads them in the jump op.
    pub(super) fn compile_cond_jump(&mut self, test: &Expression) -> Result<usize, CompileError> {
        if let Expression::Binary(op, lhs, rhs, pos) = test
            && Self::is_comparison(*op)
        {
            let cmp = Self::binary_op(*op)?;
            if let Some((a, b)) = self.local_pair(lhs, rhs) {
                self.emit(Op::LocalLocalCmpJump);
                self.emit_local_ref(a);
                self.emit_local_ref(b);
                self.push_n(2);
                self.pop_n(2);
                return Ok(self.emit_cmp_jump_tail(cmp, None, *pos));
            }
            if let Expression::Literal(Literal::Number(k)) = rhs.as_ref()
                && let Some(local) = self.local_ref(lhs)
            {
                let k_idx = self.add_constant(Constant::Number(*k))?;
                self.emit(Op::LocalConstCmpJump);
                self.emit_local_ref(local);
                self.push_n(2);
                self.pop_n(2);
                return Ok(self.emit_cmp_jump_tail(cmp, Some(k_idx), *pos));
            }
            self.compile_expr(lhs)?;
            self.compile_expr(rhs)?;
            self.pop_n(2);
            self.emit(Op::CmpJumpIfFalse);
            return Ok(self.emit_cmp_jump_tail(cmp, None, *pos));
        }
        self.compile_expr(test)?;
        self.pop_n(1);
        Ok(self.emit_jump(Op::JumpIfFalse))
    }
}
