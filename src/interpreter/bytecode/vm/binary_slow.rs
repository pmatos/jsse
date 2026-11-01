//! The binary ops: the Number-pair computations and the out-of-line paths
//! for operands that are not both Numbers.

use super::*;

/// The non-Number-pair path of every binary op. Both operands are already
/// popped from the operand stack but still rooted in `gc_bytecode_roots`, so
/// they stay alive across the `ToPrimitive`/`valueOf` calls `eval_binary` can
/// make; they are released (right, then left: LIFO) once it returns.
#[inline(never)]
pub(super) fn binary_slow(
    interp: &mut Interpreter,
    op: BinaryOp,
    l: JsValue,
    r: JsValue,
    at: (&Chunk, usize),
) -> Completion {
    member_pos::note_binary(interp, at.0, at.1);
    let result = if matches!(op, BinaryOp::Add) && is_primitive_string_concat(&l, &r) {
        interp.concat_primitives(&l, &r)
    } else {
        interp.eval_binary(op, &l, &r)
    };
    unroot_stack_value(interp, &r);
    unroot_stack_value(interp, &l);
    result
}

/// A binary op when the two operands on top of the stack are BigInts and `op`
/// has a BigInt/BigInt form (see `bigint_binary_op`), else `None` with the
/// stack untouched. BigInts are never rooted (only objects are), so the
/// operands are replaced by the result with no unrooting. A thrown RangeError
/// (division by zero, bad exponent) comes back as `Err`. The operands are
/// popped and owned rather than borrowed in place: that drops them without
/// `NanBoxedValue`'s tag dispatch and measured 1-3% fewer instructions.
#[inline(never)]
pub(super) fn bigint_pair_op(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    op: BinaryOp,
    at: (&Chunk, usize),
) -> Option<Result<(), JsValue>> {
    let len = stack.len();
    if len < 2 || !stack[len - 2].is_bigint() || !stack[len - 1].is_bigint() {
        return None;
    }
    let (Some(r), Some(l)) = (
        stack.pop().and_then(JsValue::into_bigint),
        stack.pop().and_then(JsValue::into_bigint),
    ) else {
        unreachable!("both operands are BigInts");
    };
    let Some(result) = bigint_binary_op(op, &l.value, &r.value) else {
        stack.push(JsValue::bigint(l));
        stack.push(JsValue::bigint(r));
        return None;
    };
    Some(match result {
        Ok(v) => {
            stack.push(v);
            Ok(())
        }
        Err(message) => {
            member_pos::note_binary(interp, at.0, at.1);
            Err(interp.create_error("RangeError", message))
        }
    })
}

/// The relational `cmp` over the two BigInts on top of the stack, popping
/// both, or `None` with the stack untouched when they are not both BigInts.
#[inline(never)]
pub(super) fn bigint_compare_pop(stack: &mut Vec<JsValue>, cmp: Op) -> Option<bool> {
    let len = stack.len();
    if len < 2 {
        return None;
    }
    let holds = stack[len - 2].with_bigint(|a| {
        stack[len - 1].with_bigint(|b| match cmp {
            Op::Lt => a < b,
            Op::Gt => a > b,
            Op::LtEq => a <= b,
            Op::GtEq => a >= b,
            _ => unreachable!("not a relational op: {cmp:?}"),
        })
    })??;
    stack.truncate(len - 2);
    Some(holds)
}

/// A binary op over two Numbers, computed with the same `number_ops` calls as
/// `eval_binary`'s own Number fast path.
#[inline(always)]
pub(super) fn number_binop(op: Op, l: f64, r: f64) -> JsValue {
    match op {
        Op::Add => JsValue::number(number_ops::add(l, r)),
        Op::Sub => JsValue::number(number_ops::subtract(l, r)),
        Op::Mul => JsValue::number(number_ops::multiply(l, r)),
        Op::Div => JsValue::number(number_ops::divide(l, r)),
        Op::Mod => JsValue::number(number_ops::remainder(l, r)),
        Op::Pow => JsValue::number(number_ops::exponentiate(l, r)),
        Op::Shl => JsValue::number(number_ops::left_shift(l, r)),
        Op::Shr => JsValue::number(number_ops::signed_right_shift(l, r)),
        Op::UShr => JsValue::number(number_ops::unsigned_right_shift(l, r)),
        Op::BitAnd => JsValue::number(number_ops::bitwise_and(l, r)),
        Op::BitOr => JsValue::number(number_ops::bitwise_or(l, r)),
        Op::BitXor => JsValue::number(number_ops::bitwise_xor(l, r)),
        Op::Lt
        | Op::Gt
        | Op::LtEq
        | Op::GtEq
        | Op::Eq
        | Op::StrictEq
        | Op::NotEq
        | Op::StrictNotEq => JsValue::boolean(number_compare(op, l, r)),
        _ => unreachable!("not a binary op: {op:?}"),
    }
}

/// A comparison over two Numbers, as `eval_binary`'s own Number fast path.
#[inline(always)]
pub(super) fn number_compare(op: Op, l: f64, r: f64) -> bool {
    match op {
        Op::Lt => number_ops::less_than(l, r) == Some(true),
        Op::Gt => number_ops::less_than(r, l) == Some(true),
        Op::LtEq => number_ops::less_than(r, l) == Some(false),
        Op::GtEq => number_ops::less_than(l, r) == Some(false),
        Op::Eq | Op::StrictEq => number_ops::equal(l, r),
        Op::NotEq | Op::StrictNotEq => !number_ops::equal(l, r),
        _ => unreachable!("not a comparison: {op:?}"),
    }
}
