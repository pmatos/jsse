//! The `await` keyword's offset through the async-function rewrite, which
//! turns each `await` into a `yield` and the `Yield` terminators back into
//! `Await` ones. Under `--node` the operand travels as `(offset, operand)`
//! in a `Comma`; the `Yield` arm re-applies it to an operand it hoists.

use super::{GeneratorStateMachine, StateTerminator};
use crate::ast::{Expression, Literal, ident};

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
        Some(tagged @ Expression::Comma(parts)) if at_of(tagged) != 0 => {
            (parts.get(1).cloned(), at_of(tagged))
        }
        _ => (value.clone(), 0),
    }
}

pub(super) fn rewrite_terminators_yield_to_await(machine: &mut GeneratorStateMachine) {
    let states = std::rc::Rc::get_mut(&mut machine.states)
        .expect("a freshly transformed machine's states are unshared");
    for state in states {
        let replacement = match &state.terminator {
            StateTerminator::Yield {
                value,
                is_delegate: false,
                at: _,
                resume_state,
                sent_value_binding,
            } => {
                let (value, at) = untag(value);
                Some(StateTerminator::Await {
                    value: value.unwrap_or(ident("undefined".to_string())),
                    resume_state: *resume_state,
                    sent_value_binding: sent_value_binding.clone(),
                    at,
                })
            }
            _ => None,
        };
        if let Some(new_term) = replacement {
            state.terminator = new_term;
        }
    }
}
