//! The `await` keyword's offset through the async-function rewrite, which
//! turns each `await` into a `yield` and the `Yield` terminators back into
//! `Await` ones. Under `--node` the operand travels as `(offset, operand)`
//! in a `Comma`; a yield the transform hoisted into a temporary loses it.

use crate::ast::{Expression, Literal};

pub(super) fn tag(operand: Expression, at: u32) -> Expression {
    if at != 0 && crate::interpreter::builtins::node_host_stack::call_positions_enabled() {
        let at = Expression::Literal(Literal::Number(f64::from(at)));
        return Expression::Comma(vec![at, operand]);
    }
    operand
}

/// The offset `operand` carries, if it is tagged.
pub(super) fn at_of(operand: &Expression) -> u32 {
    match operand {
        Expression::Comma(parts) => match parts.as_slice() {
            [Expression::Literal(Literal::Number(at)), _] => *at as u32,
            _ => 0,
        },
        _ => 0,
    }
}

pub(super) fn untag(value: &Option<Expression>) -> (Option<Expression>, u32) {
    match value {
        Some(Expression::Comma(parts)) => match parts.as_slice() {
            [Expression::Literal(Literal::Number(at)), operand] => {
                (Some(operand.clone()), *at as u32)
            }
            _ => (value.clone(), 0),
        },
        _ => (value.clone(), 0),
    }
}
