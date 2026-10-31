//! Operand handling of the call ops.

use super::{decode_u16, decode_u32, unroot_stack_value};
use crate::interpreter::Interpreter;
use crate::interpreter::bytecode::chunk::{CALL_POS_FLAG, Chunk};
use crate::types::JsValue;

/// Decodes the operands of `Call`/`ReturnCall`/`Construct` at `pc`: the
/// argument count, the call-site id and the code offset after the op. With
/// `CALL_POS_FLAG` set in the count (only under the `--node` stack hooks,
/// #1492) a source position follows; it becomes the interpreter's current call
/// position, as a separate `SetCallPos` op would before the call.
#[inline(always)]
pub(super) fn call_operands(
    interp: &mut Interpreter,
    chunk: &Chunk,
    pc: usize,
) -> (usize, u32, usize) {
    let raw = decode_u16(chunk, pc);
    let site = decode_u32(chunk, pc + 2);
    if raw & CALL_POS_FLAG != 0 {
        interp.node_host.call_pos = decode_u32(chunk, pc + 6);
        return (usize::from(raw & !CALL_POS_FLAG), site, pc + 10);
    }
    (usize::from(raw), site, pc + 6)
}

pub(super) fn take_call_operands(
    stack: &mut Vec<JsValue>,
    argc: usize,
) -> (JsValue, JsValue, Vec<JsValue>) {
    assert!(stack.len() >= argc + 2, "stack underflow on call operands");
    let args = stack.split_off(stack.len() - argc);
    let this_value = stack.pop().expect("stack underflow on call this");
    let callee = stack.pop().expect("stack underflow on call callee");
    (callee, this_value, args)
}

pub(super) fn release_call_operands(
    interp: &mut Interpreter,
    callee: &JsValue,
    this_value: &JsValue,
    args: &[JsValue],
) {
    for arg in args.iter().rev() {
        unroot_stack_value(interp, arg);
    }
    unroot_stack_value(interp, this_value);
    unroot_stack_value(interp, callee);
}
