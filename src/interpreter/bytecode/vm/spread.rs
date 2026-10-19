//! Runtime of the spread-argument ops (see `compiler/calls.rs`): the
//! arguments live on the operand stack, rooted one by one like any operand,
//! with their running count in a Number slot above them.

use super::{Op, pop_operands, pop_value, push_value};
use crate::interpreter::Interpreter;
use crate::interpreter::types::Completion;
use crate::types::JsValue;

fn count_of(value: &JsValue) -> usize {
    value
        .as_number()
        .expect("spread argument count is a Number") as usize
}

/// Replaces the iterable on top of the stack (and the count slot under it,
/// when `counted`) with its elements as arguments, then the updated count. On
/// a throw the operands stay where the handler's truncation will release them.
fn spread_into_arguments(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    before: usize,
    counted: bool,
) -> Result<(), JsValue> {
    let values = interp.iterate_to_vec(stack.last().expect("stack underflow on spread"))?;
    pop_value(interp, stack, "stack underflow on spread");
    if counted {
        stack.pop();
    }
    let total = before + values.len();
    if !values.is_empty() {
        stack.reserve(values.len());
    }
    for value in values {
        push_value(interp, stack, value);
    }
    stack.push(JsValue::number(total as f64));
    Ok(())
}

/// Runs one spread-argument op. `before` is the operand of `ArgSpreadFirst`:
/// the arguments already pushed.
#[inline(never)]
pub(super) fn run(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    op: Op,
    before: usize,
) -> Result<(), Completion> {
    match op {
        Op::ArgSpreadFirst => {
            spread_into_arguments(interp, stack, before, false).map_err(Completion::Throw)
        }
        Op::ArgSpread => {
            let before = count_of(&stack[stack.len() - 2]);
            spread_into_arguments(interp, stack, before, true).map_err(Completion::Throw)
        }
        Op::ArgPush => {
            let top = stack.len() - 1;
            let before = count_of(&stack[top - 1]);
            stack.swap(top - 1, top);
            stack[top] = JsValue::number((before + 1) as f64);
            Ok(())
        }
        _ => finish_call(interp, stack, op == Op::CallSpread),
    }
}

/// `CallSpread`/`ConstructSpread`: the layout under the count is the one
/// `Op::Call`/`Op::Construct` consume, and stays on the operand stack (rooted)
/// for the whole nested call.
fn finish_call(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    is_call: bool,
) -> Result<(), Completion> {
    let argc = count_of(&stack.pop().expect("stack underflow on spread call"));
    let below = if is_call { 2 } else { 1 };
    assert!(
        stack.len() >= argc + below,
        "stack underflow on spread call"
    );
    let base = stack.len() - argc - below;
    let callee = stack[base].clone();
    let result = if is_call {
        #[cfg(feature = "perf-counters")]
        {
            interp.perf.calls_from_vm += 1;
        }
        let this_value = stack[base + 1].clone();
        interp.call_function(&callee, &this_value, &stack[base + 2..])
    } else {
        interp.construct_from_evaluated(&callee, &stack[base + 1..])
    };
    pop_operands(interp, stack, argc + below);
    match result {
        Completion::Normal(value) | Completion::Return(value) => push_value(interp, stack, value),
        Completion::Empty => push_value(interp, stack, JsValue::UNDEFINED),
        abrupt => return Err(abrupt),
    }
    Ok(())
}
