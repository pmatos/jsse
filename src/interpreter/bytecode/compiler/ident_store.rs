//! Stores to and updates of identifiers (`x = v`, `x += v`, `x++`), with the
//! `--node` error position of the `=`/`++` token (#1511): `Chunk::binary_pos`
//! maps the offset after the store instruction to it, which the VM reads when
//! the store throws a constant assignment.

use super::{CompileError, Compiler};
use crate::ast::{AssignOp, Expression, Literal, MemberProperty, PropSiteId, UpdateOp};
use crate::interpreter::bytecode::op::{Op, UPDATE_DISCARD};

impl Compiler {
    /// The site of the read a compound assignment or update makes of its
    /// member target: V8's position for a nullish base there.
    pub(super) fn read_site(
        obj: &Expression,
        prop: &MemberProperty,
        site: PropSiteId,
    ) -> PropSiteId {
        PropSiteId(
            site.0,
            crate::interpreter::builtins::node_host_assign_pos::last_member_pos(obj, prop, site),
        )
    }

    /// `name = value` / `name op= value`. Leaves the assigned value on the
    /// operand stack, or nothing when `discard`.
    pub(super) fn compile_identifier_assign(
        &mut self,
        op: AssignOp,
        name: &str,
        value: &Expression,
        discard: bool,
        at: u32,
    ) -> Result<(), CompileError> {
        let idx = self.name_site(name)?;
        if let Some(slot) = self.local_slot(name) {
            // A local of this frame: no `with`, closure or direct eval can
            // change what the name resolves to while the right-hand side
            // runs, so the reference needs no early resolution.
            if op == AssignOp::Assign
                && discard
                && let Some(src) = self.local_ref(value)
            {
                self.emit_move_local(src, (idx, slot));
                self.note_binary_pos(at);
                return Ok(());
            }
            if op == AssignOp::Assign {
                self.compile_named_value(value, name)?;
            } else if let Expression::Literal(Literal::Number(k)) = value {
                self.emit_local_bin_const(name, slot, Self::compound_binary_op(op)?, *k)?;
            } else if let Some(src) = self.local_ref(value) {
                self.emit_local_local_bin((idx, slot), src, Self::compound_binary_op(op)?);
            } else {
                self.compile_load_name(name)?;
                self.compile_expr(value)?;
                self.emit(Self::compound_binary_op(op)?);
                self.pop_n(2);
                self.push_n(1);
            }
            self.emit_store_local(idx, slot, discard);
            self.note_binary_pos(at);
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
            self.note_binary_pos(at);
        }
        Ok(())
    }

    /// `x++`, `o.p++`, `o[k]--` and friends. The mode byte's bit 2 means "do
    /// not push the result", which effect context sets for identifiers: the
    /// discarded value is unobservable.
    pub(super) fn compile_update(
        &mut self,
        op: UpdateOp,
        prefix: bool,
        target: &Expression,
        discard: bool,
        at: u32,
    ) -> Result<(), CompileError> {
        let mode = match (op, prefix) {
            (UpdateOp::Increment, false) => 0,
            (UpdateOp::Increment, true) => 1,
            (UpdateOp::Decrement, false) => 2,
            (UpdateOp::Decrement, true) => 3,
        };
        match target {
            Expression::Identifier(name) => {
                let idx = self.name_site(name)?;
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
                self.note_binary_pos(at);
                if !discard {
                    self.push_n(1);
                }
                Ok(())
            }
            Expression::Member(obj, prop @ MemberProperty::Dot(name), site) if !discard => {
                self.compile_expr(obj)?;
                let idx = self.add_name(name)?;
                self.emit_member(Op::UpdateProp, Self::read_site(obj, prop, *site));
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
            Expression::Member(obj, prop @ MemberProperty::Computed(key), site) if !discard => {
                self.compile_expr(obj)?;
                self.compile_expr(key)?;
                self.emit_member(Op::UpdateElement, Self::read_site(obj, prop, *site));
                self.code.push(mode);
                self.pop_n(2);
                self.push_n(1);
                Ok(())
            }
            _ => Err(CompileError::Unsupported("update target")),
        }
    }
}
