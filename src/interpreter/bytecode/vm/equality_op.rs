use super::*;

/// `==`/`!=` through `Interpreter::loose_equal_no_user_code`: replaces the two operands
/// with the Boolean result and returns `true`, or returns `false` and leaves
/// the stack alone.
#[inline(never)]
pub(super) fn loose_equality_op(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    negate: bool,
) -> bool {
    let len = stack.len();
    if len < 2 {
        return false;
    }
    let Some(equal) = interp.loose_equal_no_user_code(&stack[len - 2], &stack[len - 1]) else {
        return false;
    };
    pop_value(interp, stack, "stack underflow on binop rhs");
    pop_value(interp, stack, "stack underflow on binop lhs");
    stack.push(JsValue::boolean(equal != negate));
    true
}
