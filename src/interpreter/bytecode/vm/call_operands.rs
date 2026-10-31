//! Operand handling of the call ops.

use super::unroot_stack_value;
use crate::interpreter::Interpreter;
use crate::types::JsValue;

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
