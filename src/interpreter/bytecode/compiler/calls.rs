//! Calls and `new` with spread arguments (sec-argumentlistevaluation).
//!
//! The callee (and `this`) sit below the arguments as for `Op::Call`. Plain
//! arguments before the first spread are pushed as usual; the first spread
//! (`ArgSpreadFirst`) appends the iterable's elements and leaves a running
//! argument count on top of them, which every later argument keeps current
//! (`ArgSpread`, `ArgPush`). `CallSpread`/`ConstructSpread` read the count to
//! find the callee, so the arguments are never copied into a list. A lone
//! spread needs no count: `CallSpreadSole`/`ConstructSpreadSole` push the
//! elements and call.
//!
//! The static stack model counts the count slot and the arguments before the
//! first spread only; the elements a spread contributes are dynamic, which no
//! operand addresses from below.

use super::{CompileError, Compiler};
use crate::ast::{CallSiteId, Expression};
use crate::interpreter::bytecode::op::Op;

pub(super) fn has_spread(args: &[Expression]) -> bool {
    args.iter().any(|arg| matches!(arg, Expression::Spread(_)))
}

/// A spread call that stays on the tree-walker: a strict tail call keeps its
/// `TailCall` completion there, and direct `eval` keeps the whole body
/// there (`compile_call`), so neither may reach the native lowering.
pub(super) fn is_unsupported_call(callee: &Expression, args: &[Expression], op: Op) -> bool {
    has_spread(args)
        && (op == Op::ReturnCall
            || matches!(callee, Expression::Identifier(name) if name == "eval"))
}

impl Compiler {
    /// `[..., callee, this]` → `[..., result]` (`below` is 2), or `[...,
    /// callee]` → `[..., result]` for `new` (`below` is 1).
    pub(super) fn compile_spread_arguments(
        &mut self,
        args: &[Expression],
        below: u16,
    ) -> Result<(), CompileError> {
        if usize::from(self.current_stack) + args.len() + 1 > usize::from(u16::MAX) {
            return Err(CompileError::Ineligible("operand stack overflow"));
        }
        if let [Expression::Spread(iterable)] = args {
            self.compile_expr(iterable)?;
            self.emit(if below == 2 {
                Op::CallSpreadSole
            } else {
                Op::ConstructSpreadSole
            });
            self.pop_n(below + 1);
            self.push_n(1);
            return Ok(());
        }
        let mut slots: u16 = 0;
        let mut spread_seen = false;
        for arg in args {
            match (arg, spread_seen) {
                (Expression::Spread(iterable), false) => {
                    self.compile_expr(iterable)?;
                    self.emit(Op::ArgSpreadFirst);
                    self.emit_u16(slots);
                    spread_seen = true;
                    slots += 1;
                }
                (Expression::Spread(iterable), true) => {
                    self.compile_expr(iterable)?;
                    self.emit(Op::ArgSpread);
                    self.pop_n(1);
                }
                (value, false) => {
                    self.compile_expr(value)?;
                    slots += 1;
                }
                (value, true) => {
                    self.compile_expr(value)?;
                    self.emit(Op::ArgPush);
                    self.pop_n(1);
                }
            }
        }
        self.emit(if below == 2 {
            Op::CallSpread
        } else {
            Op::ConstructSpread
        });
        self.pop_n(below + slots);
        self.push_n(1);
        Ok(())
    }

    pub(super) fn compile_new(
        &mut self,
        callee: &Expression,
        args: &[Expression],
        site_id: CallSiteId,
    ) -> Result<(), CompileError> {
        let argc = u16::try_from(args.len())
            .map_err(|_| CompileError::Ineligible("construct argument count overflow"))?;
        if usize::from(self.current_stack) + args.len() + 1 > usize::from(u16::MAX) {
            return Err(CompileError::Ineligible("operand stack overflow"));
        }
        // Construct never reads a `this` value from the callee reference
        // (unlike Call), so no LoadCalleeName-style receiver capture is
        // needed — any expression compiles as the callee directly.
        self.compile_expr(callee)?;
        if has_spread(args) {
            return self.compile_spread_arguments(args, 1);
        }
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
}
