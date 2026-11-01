//! Runtime of the spread-argument ops (see `compiler/calls.rs`): the
//! arguments live on the operand stack, rooted one by one like any operand,
//! with their running count in a Number slot above them.

use super::{Op, decode_u16, pop_operands, pop_value, push_value};
use crate::interpreter::Interpreter;
use crate::interpreter::bytecode::chunk::Chunk;
use crate::interpreter::types::Completion;
use crate::types::JsValue;

fn count_of(value: &JsValue) -> usize {
    value
        .as_number()
        .expect("spread argument count is a Number") as usize
}

/// Replaces the iterable on top of the stack (and the count slot under it,
/// when `counted`) with its elements as arguments and returns how many. On a
/// throw the operands stay where the handler's truncation will release them.
fn spread_values(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    counted: bool,
) -> Result<usize, JsValue> {
    let values =
        interp.iterate_call_spread_noted(stack.last().expect("stack underflow on spread"))?;
    pop_value(interp, stack, "stack underflow on spread");
    if counted {
        stack.pop();
    }
    let spread = values.len();
    if spread != 0 {
        stack.reserve(spread);
    }
    for value in values {
        push_value(interp, stack, value);
    }
    Ok(spread)
}

/// Runs one spread-argument op and returns the length of its operand, read
/// here (`before` is the operand of `ArgSpreadFirst`:
/// the arguments already pushed.
#[inline(never)]
pub(super) fn run(
    interp: &mut Interpreter,
    chunk: &Chunk,
    stack: &mut Vec<JsValue>,
    pc: usize,
    op: Op,
) -> Result<usize, Completion> {
    interp.node_host.pattern_src = chunk.pattern_src_at(pc - 1).cloned();
    if let Some(pos) = chunk.member_pos_at(pc - 1) {
        interp.node_host.call_pos = pos;
    }
    let (operand_len, before) = if op == Op::ArgSpreadFirst {
        (2, usize::from(decode_u16(chunk, pc)))
    } else {
        (0, 0)
    };
    let (argc, is_call) = match op {
        Op::ArgSpreadFirst => {
            let total = before + spread_values(interp, stack, false).map_err(Completion::Throw)?;
            stack.push(JsValue::number(total as f64));
            return Ok(operand_len);
        }
        Op::ArgSpread => {
            let before = count_of(&stack[stack.len() - 2]);
            let total = before + spread_values(interp, stack, true).map_err(Completion::Throw)?;
            stack.push(JsValue::number(total as f64));
            return Ok(operand_len);
        }
        Op::ArgPush => {
            let top = stack.len() - 1;
            let before = count_of(&stack[top - 1]);
            stack.swap(top - 1, top);
            stack[top] = JsValue::number((before + 1) as f64);
            return Ok(operand_len);
        }
        Op::CallSpreadSole | Op::ConstructSpreadSole => (
            spread_values(interp, stack, false).map_err(Completion::Throw)?,
            op == Op::CallSpreadSole,
        ),
        _ => (
            count_of(&stack.pop().expect("stack underflow on spread call")),
            op == Op::CallSpread,
        ),
    };
    finish_call(interp, stack, argc, is_call).map(|()| operand_len)
}

/// The layout under the arguments is the one `Op::Call`/`Op::Construct`
/// consume, and stays on the operand stack (rooted) for the whole nested call.
fn finish_call(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    argc: usize,
    is_call: bool,
) -> Result<(), Completion> {
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
