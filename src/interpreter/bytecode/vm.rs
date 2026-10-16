use super::chunk::{
    ArrayTemplate, BLOCK_SLOT_FLAG, Chunk, Constant, EXEC_NO_TAIL_CALL, EXEC_SET_COMPLETION,
    HeadBinding,
};
use super::op::{Op, UPDATE_DISCARD};
use crate::ast::{BinaryOp, CallSiteId, UnaryOp, UpdateOp};
use crate::interpreter::eval::{IdentifierRef, is_primitive_string_concat};
use crate::interpreter::ic::CallIcSlot;
use crate::interpreter::types::{Binding, BindingKind, Completion, Environment, PrivateId};
use crate::interpreter::{Atom, EnvRef, GlobalHint, Interpreter};
use crate::types::{JsValue, number_ops};
use smallvec::SmallVec;

#[inline(always)]
fn decode_i16(chunk: &Chunk, pc: usize) -> i16 {
    let lo = chunk.code[pc] as u16;
    let hi = chunk.code[pc + 1] as u16;
    ((hi << 8) | lo) as i16
}

/// Decodes the mode byte shared by `UpdateName` and `UpdateLocal`.
/// Returns the operation, whether it is a prefix form, and whether the result
/// is discarded (bit 2: effect context, the result is not pushed).
fn update_mode(mode: u8) -> (UpdateOp, bool, bool) {
    let discard = mode & UPDATE_DISCARD != 0;
    match mode & 0b11 {
        0 => (UpdateOp::Increment, false, discard),
        1 => (UpdateOp::Increment, true, discard),
        2 => (UpdateOp::Decrement, false, discard),
        _ => (UpdateOp::Decrement, true, discard),
    }
}

/// The `BinaryOp` a binary-op byte (an `Op` discriminant) stands for.
fn binary_of_op(op: Op) -> BinaryOp {
    match op {
        Op::Add => BinaryOp::Add,
        Op::Sub => BinaryOp::Sub,
        Op::Mul => BinaryOp::Mul,
        Op::Div => BinaryOp::Div,
        Op::Mod => BinaryOp::Mod,
        Op::Pow => BinaryOp::Exp,
        Op::Eq => BinaryOp::Eq,
        Op::NotEq => BinaryOp::NotEq,
        Op::StrictEq => BinaryOp::StrictEq,
        Op::StrictNotEq => BinaryOp::StrictNotEq,
        Op::Lt => BinaryOp::Lt,
        Op::Gt => BinaryOp::Gt,
        Op::LtEq => BinaryOp::LtEq,
        Op::GtEq => BinaryOp::GtEq,
        Op::BitAnd => BinaryOp::BitAnd,
        Op::BitOr => BinaryOp::BitOr,
        Op::BitXor => BinaryOp::BitXor,
        Op::Shl => BinaryOp::LShift,
        Op::Shr => BinaryOp::RShift,
        Op::UShr => BinaryOp::URShift,
        _ => unreachable!("not a binary op: {op:?}"),
    }
}

/// A binary op over two Numbers, computed with the same `number_ops` calls as
/// `eval_binary`'s own Number fast path.
#[inline(always)]
fn number_binop(op: Op, l: f64, r: f64) -> JsValue {
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
fn number_compare(op: Op, l: f64, r: f64) -> bool {
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

/// The non-Number-pair path of every binary op. Both operands are already
/// popped from the operand stack but still rooted in `gc_bytecode_roots`, so
/// they stay alive across the `ToPrimitive`/`valueOf` calls `eval_binary` can
/// make; they are released (right, then left: LIFO) once it returns.
#[inline(never)]
fn binary_slow(interp: &mut Interpreter, op: BinaryOp, l: JsValue, r: JsValue) -> Completion {
    let result = if matches!(op, BinaryOp::Add) && is_primitive_string_concat(&l, &r) {
        interp.concat_primitives(&l, &r)
    } else {
        interp.eval_binary(op, &l, &r)
    };
    unroot_stack_value(interp, &r);
    unroot_stack_value(interp, &l);
    result
}

/// `StoreLocal`'s store: the frame slot when the hint verifies, else the
/// by-name reference path.
#[inline(always)]
fn store_local(
    interp: &mut Interpreter,
    name: &Atom,
    slot: u16,
    target: &EnvRef,
    env: &EnvRef,
    value: &JsValue,
    hint: Option<&GlobalHint>,
) -> Result<(), JsValue> {
    let stored = {
        let mut scope = target.borrow_mut();
        let plain = scope.with_object.is_none()
            && scope.indirect_bindings.is_none()
            && scope.global_object_id.is_none();
        match scope.bindings.entry_at_mut(slot as usize) {
            Some((atom, binding))
                if plain
                    && atom.ptr_eq(name)
                    && binding.initialized
                    && matches!(binding.kind, BindingKind::Var | BindingKind::Let) =>
            {
                binding.value = value.clone();
                true
            }
            _ => false,
        }
    };
    if stored {
        #[cfg(feature = "perf-counters")]
        {
            interp.perf.local_slot_hits += 1;
        }
        return Ok(());
    }
    #[cfg(feature = "perf-counters")]
    {
        interp.perf.local_slot_misses += 1;
    }
    let id_ref = interp.resolve_identifier_ref_hinted(name, env, hint)?;
    match interp.put_value_by_ref_hinted(name, value.clone(), &id_ref, env, hint) {
        Completion::Throw(e) => Err(e),
        _ => Ok(()),
    }
}

/// `StoreResolvedName`'s store through the reference `ResolveName` created.
#[inline(always)]
fn store_resolved_name(
    interp: &mut Interpreter,
    name: &Atom,
    id_ref: &IdentifierRef,
    env: &EnvRef,
    value: &JsValue,
    hint: Option<&GlobalHint>,
) -> Result<(), JsValue> {
    let local_store = if let IdentifierRef::SpecificEnv(local_env) = id_ref {
        let mut scope = local_env.borrow_mut();
        if scope.global_object_id.is_none()
            && scope.indirect_bindings.is_none()
            && let Some(binding) = scope.bindings.get_mut(name)
            && binding.initialized
            && matches!(binding.kind, BindingKind::Var | BindingKind::Let)
        {
            binding.value = value.clone();
            true
        } else {
            false
        }
    } else {
        false
    };
    if !local_store
        && let Completion::Throw(e) =
            interp.put_value_by_ref_hinted(name, value.clone(), id_ref, env, hint)
    {
        return Err(e);
    }
    Ok(())
}

/// The environment a local-op slot operand addresses, and the slot within it:
/// the frame (function) environment, or one of the chunk's open block
/// environments (see `BLOCK_SLOT_FLAG`). Only a hint: the caller verifies the
/// binding by atom identity and falls back to by-name resolution from the
/// current environment.
#[inline(always)]
fn local_target<'a>(frame_env: &'a EnvRef, scopes: &'a [EnvRef], slot: u16) -> (&'a EnvRef, u16) {
    if slot & BLOCK_SLOT_FLAG == 0 {
        (frame_env, slot)
    } else {
        let scope = usize::from((slot >> 8) & 0x7f);
        debug_assert!(
            scope < scopes.len(),
            "block slot names environment {scope} of {}",
            scopes.len()
        );
        (scopes.get(scope).unwrap_or(frame_env), slot & 0xff)
    }
}

/// The frame binding a verified slot hint names, when it is initialized and
/// the scope is a plain declarative one; `f` sees its value in place.
#[inline(always)]
fn with_local_hit<R>(
    env: &EnvRef,
    name: &Atom,
    slot: u16,
    f: impl FnOnce(&JsValue) -> R,
) -> Option<R> {
    let scope = env.borrow();
    match scope.bindings.entry_at(slot as usize) {
        Some((atom, binding))
            if atom.ptr_eq(name)
                && binding.initialized
                && scope.with_object.is_none()
                && scope.indirect_bindings.is_none() =>
        {
            Some(f(&binding.value))
        }
        _ => None,
    }
}

/// Where a `break`/`continue` completion leaving an `ExecAst` statement
/// resumes, if its label names a loop compiled into this chunk.
fn escape_target(
    chunk: &Chunk,
    table: u16,
    is_continue: bool,
    label: Option<&str>,
) -> Option<(usize, usize, usize)> {
    if table == super::chunk::NO_ESCAPE_TABLE {
        return None;
    }
    chunk.escape_tables[table as usize]
        .iter()
        .find(|e| e.is_continue == is_continue && e.label.as_deref() == label)
        .map(|e| {
            (
                e.target as usize,
                usize::from(e.scope_depth),
                usize::from(e.stack_height),
            )
        })
}

/// Replaces the Script's completion value, keeping the new value rooted in
/// `gc_bytecode_roots` for as long as the accumulator holds it.
fn set_script_completion(interp: &mut Interpreter, slot: &mut Option<JsValue>, value: JsValue) {
    if let Some(previous) = slot.replace(value.clone()) {
        unroot_stack_value(interp, &previous);
    }
    root_stack_value(interp, &value);
}

/// A `break`/`continue` completion that carries a value updates the Script's
/// completion value before the jump (UpdateEmpty).
fn carry_script_completion(
    interp: &mut Interpreter,
    slot: &mut Option<JsValue>,
    carried: &Option<JsValue>,
) {
    if let Some(value) = carried {
        set_script_completion(interp, slot, value.clone());
    }
}

/// Drops operand-stack values (unrooting them, innermost first) until
/// `height` remain.
fn truncate_stack(interp: &mut Interpreter, stack: &mut Vec<JsValue>, height: usize) {
    while stack.len() > height {
        pop_value(interp, stack, "stack underflow truncating operands");
    }
}

/// Stores one for-in/for-of iteration value through a head binding the
/// compiler left to the tree-walker's pattern machinery.
fn bind_head(
    interp: &mut Interpreter,
    head: &HeadBinding,
    value: JsValue,
    env: &EnvRef,
) -> Completion {
    match head {
        HeadBinding::Declare { pattern, kind } => interp.bind_pattern(pattern, value, *kind, env),
        HeadBinding::Assign { pattern } => interp.assign_to_for_pattern(pattern, value, env),
    }
}

/// Leaves block environments until `depth` remain, keeping the VM's scope
/// stack and its GC-rooted mirror in `call_stack_envs` in step.
fn unwind_scopes(
    interp: &mut Interpreter,
    scopes: &mut SmallVec<[EnvRef; 4]>,
    envs_base: usize,
    depth: usize,
) {
    interp.call_stack_envs.truncate(envs_base + depth);
    while scopes.len() > depth {
        let env = scopes.pop().expect("length checked above");
        interp.recycle_block_environment(env);
    }
}

#[inline(always)]
fn decode_u16(chunk: &Chunk, pc: usize) -> u16 {
    let lo = chunk.code[pc] as u16;
    let hi = chunk.code[pc + 1] as u16;
    (hi << 8) | lo
}

fn decode_u32(chunk: &Chunk, pc: usize) -> u32 {
    u32::from_le_bytes(chunk.code[pc..pc + 4].try_into().unwrap())
}

/// Debug check of the operand-stack rooting invariant: every object-valued
/// entry of the operand stack was pushed through `push_value`, which roots it
/// in `gc_bytecode_roots`, so this frame's slice of that stack holds one id per
/// object operand (plus one for an object the Script's completion value
/// accumulator holds). An opcode that can run user code (a getter, proxy trap
/// or `ToPropertyKey` coercion reaching a nested `gc_safepoint`) therefore
/// needs no further rooting of the operands still pending below its own; a raw
/// `stack.push` of an object would fail this check.
#[inline(always)]
fn debug_assert_operands_rooted(
    interp: &Interpreter,
    stack: &[JsValue],
    roots_base: usize,
    completion_value: Option<&JsValue>,
) {
    if cfg!(debug_assertions) {
        let operands = stack.iter().filter(|v| v.as_object_id().is_some()).count();
        let accumulator = completion_value.is_some_and(|v| v.as_object_id().is_some());
        debug_assert_eq!(
            interp.gc_bytecode_roots.len() - roots_base,
            operands + usize::from(accumulator),
            "an object operand reached the stack without push_value"
        );
    }
}

#[inline(always)]
fn root_stack_value(interp: &mut Interpreter, value: &JsValue) {
    if let Some(object_id) = value.as_object_id() {
        interp.gc_bytecode_roots.push(object_id);
    }
}

#[inline(always)]
fn unroot_stack_value(interp: &mut Interpreter, value: &JsValue) {
    if let Some(object_id) = value.as_object_id() {
        interp.gc_bytecode_roots.pop_expected(object_id);
    }
}

#[inline(always)]
fn push_value(interp: &mut Interpreter, stack: &mut Vec<JsValue>, value: JsValue) {
    root_stack_value(interp, &value);
    stack.push(value);
}

#[inline(always)]
fn pop_value(interp: &mut Interpreter, stack: &mut Vec<JsValue>, context: &'static str) -> JsValue {
    let value = stack.pop().unwrap_or_else(|| panic!("{context}"));
    unroot_stack_value(interp, &value);
    value
}

fn take_call_operands(stack: &mut Vec<JsValue>, argc: usize) -> (JsValue, JsValue, Vec<JsValue>) {
    assert!(stack.len() >= argc + 2, "stack underflow on call operands");
    let args = stack.split_off(stack.len() - argc);
    let this_value = stack.pop().expect("stack underflow on call this");
    let callee = stack.pop().expect("stack underflow on call callee");
    (callee, this_value, args)
}

fn release_call_operands(
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

/// Takes the top `count` operands as the elements of an array literal and
/// unroots them (innermost first, LIFO with the pushes). Nothing can collect
/// before the caller has allocated the array from them.
fn take_literal_values(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    count: usize,
) -> Vec<JsValue> {
    assert!(stack.len() >= count, "stack underflow on array literal");
    let values = stack.split_off(stack.len() - count);
    for value in values.iter().rev() {
        unroot_stack_value(interp, value);
    }
    values
}

/// Pops and unroots `count` operands, innermost first (LIFO with the pushes).
fn pop_operands(interp: &mut Interpreter, stack: &mut Vec<JsValue>, count: usize) {
    for _ in 0..count {
        pop_value(interp, stack, "stack underflow releasing call operands");
    }
}

fn member_get(interp: &mut Interpreter, base: &JsValue, name: &str) -> Completion {
    if base.is_nullish() {
        let err = interp.create_type_error(&format!(
            "Cannot read properties of {base} (reading '{name}')"
        ));
        return Completion::Throw(err);
    }
    if let Some(object_id) = base.as_object_id() {
        return interp.get_object_property(object_id, name, base);
    }
    // GetValue on a primitive base keeps the primitive as the [[Get]]
    // receiver, so a strict getter sees `this` unboxed.
    interp.access_property_on_value(base, name)
}

fn member_get_computed(interp: &mut Interpreter, base: &JsValue, key_val: &JsValue) -> Completion {
    if base.is_nullish() {
        let err = interp.create_type_error(&format!(
            "Cannot read properties of {base} (reading property)"
        ));
        return Completion::Throw(err);
    }
    let key = match interp.to_property_key(key_val) {
        Ok(k) => k,
        Err(e) => return Completion::Throw(e),
    };
    if let Some(object_id) = base.as_object_id() {
        return interp.get_object_property(object_id, &key, base);
    }
    interp.access_property_on_value(base, &key)
}

fn member_set(
    interp: &mut Interpreter,
    base: JsValue,
    name: &Atom,
    rhs: JsValue,
    strict: bool,
) -> Result<(), JsValue> {
    if base.is_nullish() {
        return Err(interp.create_type_error(&format!(
            "Cannot set properties of {base} (setting '{}')",
            name.as_str()
        )));
    }
    interp.put_member_value(base, name.property_key(), rhs, strict)
}

fn member_set_computed(
    interp: &mut Interpreter,
    base: JsValue,
    key_val: &JsValue,
    rhs: JsValue,
    strict: bool,
) -> Result<(), JsValue> {
    let key = interp.to_property_key(key_val)?;
    if base.is_nullish() {
        return Err(interp.create_type_error(&format!(
            "Cannot set properties of {base} (setting '{key}')"
        )));
    }
    interp.put_member_value(base, &key, rhs, strict)
}

/// `++`/`--` on a member reference: the base and key are evaluated once by
/// the caller; GetValue → ToNumeric → ±1 → PutValue, with ToPropertyKey run
/// once (sec-postfix-increment-operator-runtime-semantics-evaluation,
/// sec-prefix-increment-operator-runtime-semantics-evaluation). The postfix
/// result is the numeric old value. `base.name` reads and writes through the
/// interned name without building a key.
fn member_update_named(
    interp: &mut Interpreter,
    base: &JsValue,
    name: &str,
    op: UpdateOp,
    prefix: bool,
    strict: bool,
) -> Completion {
    if base.is_nullish() {
        return Completion::Throw(interp.create_type_error(&format!(
            "Cannot read properties of {base} (reading '{name}')"
        )));
    }
    interp.update_member_value(base, name, op, prefix, strict)
}

fn member_update_computed(
    interp: &mut Interpreter,
    base: &JsValue,
    key_val: &JsValue,
    op: UpdateOp,
    prefix: bool,
    strict: bool,
) -> Completion {
    if base.is_nullish() {
        return Completion::Throw(interp.create_type_error(&format!(
            "Cannot read properties of {base} (reading property)"
        )));
    }
    match interp.to_property_key(key_val) {
        Ok(key) => interp.update_member_value(base, &key, op, prefix, strict),
        Err(e) => Completion::Throw(e),
    }
}

/// Declares the chunk's `var` names in first-use order, which is the order
/// its local slot hints assume. Declaration instantiation that runs first
/// (`Chunk::needs_instantiation`) calls this beforehand so it cannot choose a
/// different binding order.
pub(crate) fn declare_var_prologue(chunk: &Chunk, env: &EnvRef) {
    if chunk.var_names.is_empty() {
        return;
    }
    let var_scope = Environment::find_var_scope(env);
    let mut scope = var_scope.borrow_mut();
    for &name_idx in &chunk.var_names {
        let name = &chunk.names[name_idx as usize];
        if !scope.bindings.contains_key(name) {
            scope.declare_atom(name, BindingKind::Var);
        }
    }
}

/// Declares `lexicals` (`names` index, immutable) in TDZ, in order.
fn declare_lexicals(scope: &mut Environment, chunk: &Chunk, lexicals: &[(u16, bool)]) {
    for &(name_idx, is_const) in lexicals {
        scope.declare_atom(
            &chunk.names[usize::from(name_idx)],
            BindingKind::lexical(is_const),
        );
    }
}

/// `GetIterator` / the for-in head: `[value]` -> `[iterator]`.
#[inline(never)]
fn iteration_head(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    is_for_in: bool,
) -> Result<(), JsValue> {
    let value = stack.pop().expect("stack underflow on GetIterator");
    let result = if is_for_in {
        interp.for_in_head_iterator(&value)
    } else {
        interp.get_iterator(&value)
    };
    unroot_stack_value(interp, &value);
    let iterator = result?;
    push_value(interp, stack, iterator);
    Ok(())
}

/// `IterStep`/`ForInStep`: `[it]` -> `[it, value]` (true), or the iterator is
/// dropped (false) when it is exhausted.
#[inline(never)]
fn iteration_step(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    is_for_in: bool,
) -> Result<bool, JsValue> {
    let iterator = stack.last().expect("stack underflow on IterStep").clone();
    let step = if is_for_in {
        match iterator.as_object_id() {
            Some(id) => interp.for_in_enumerator_step(id),
            None => Ok(None),
        }
    } else {
        interp.iterator_step_value(&iterator)
    };
    match step? {
        Some(value) => {
            push_value(interp, stack, value);
            Ok(true)
        }
        None => {
            pop_value(interp, stack, "stack underflow on IterStep");
            Ok(false)
        }
    }
}

/// `IterClose`: IteratorClose with a normal completion for the iterator
/// `depth` values below the top, which is removed.
#[inline(never)]
fn iteration_close(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    depth: usize,
) -> Result<(), JsValue> {
    let index = stack
        .len()
        .checked_sub(1 + depth)
        .expect("stack underflow on IterClose");
    // The iterator and any pending value above it stay rooted while
    // `return()` runs.
    let iterator = stack[index].clone();
    interp.iterator_close_result(&iterator)?;
    let mut held: SmallVec<[JsValue; 1]> = SmallVec::new();
    for _ in 0..depth {
        held.push(pop_value(interp, stack, "stack underflow on IterClose"));
    }
    pop_value(interp, stack, "stack underflow on IterClose");
    for value in held.into_iter().rev() {
        push_value(interp, stack, value);
    }
    Ok(())
}

/// `IterCloseThrow`: `[it, thrown]`; closes the iterator without letting its
/// `return()` replace the throw, and yields the throw to rethrow.
#[inline(never)]
fn iteration_close_throw(interp: &mut Interpreter, stack: &mut Vec<JsValue>) -> JsValue {
    let thrown = stack
        .last()
        .expect("stack underflow on IterCloseThrow")
        .clone();
    let iterator = stack[stack.len() - 2].clone();
    interp.iterator_close(&iterator, thrown.clone());
    pop_value(interp, stack, "stack underflow on IterCloseThrow");
    pop_value(interp, stack, "stack underflow on IterCloseThrow");
    thrown
}

#[inline(never)]
fn drop_below(interp: &mut Interpreter, stack: &mut Vec<JsValue>, has_value: bool, count: usize) {
    let held = if has_value {
        Some(pop_value(interp, stack, "stack underflow on DropBelow"))
    } else {
        None
    };
    for _ in 0..count {
        pop_value(interp, stack, "stack underflow on DropBelow");
    }
    if let Some(value) = held {
        push_value(interp, stack, value);
    }
}

/// `EndFinally`: pops `[value, kind]`. Kind 0 is a normal completion (`None`),
/// 1 rethrows `value` (`Err`), and `n >= 2` resumes at the finalizer's exit
/// `n - 2` (`Some(target)`), pushing `value` back when it carries one.
#[inline(never)]
fn end_finally(
    interp: &mut Interpreter,
    chunk: &Chunk,
    stack: &mut Vec<JsValue>,
    table: usize,
) -> Result<Option<usize>, JsValue> {
    let kind = pop_value(interp, stack, "stack underflow on EndFinally kind");
    let value = pop_value(interp, stack, "stack underflow on EndFinally value");
    let kind = kind.as_number().expect("finalizer completion kind") as usize;
    match kind {
        0 => Ok(None),
        1 => Err(value),
        n => {
            let exit = chunk.finally_tables[table][n - 2];
            if exit.keep_value {
                push_value(interp, stack, value);
            }
            Ok(Some(exit.target as usize))
        }
    }
}

#[inline(never)]
fn bind_head_op(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    head: &HeadBinding,
    env: &EnvRef,
) -> Result<(), Completion> {
    let value = stack.last().expect("stack underflow on BindHead").clone();
    let result = bind_head(interp, head, value, env);
    pop_value(interp, stack, "stack underflow on BindHead");
    match result {
        Completion::Normal(_) | Completion::Empty => Ok(()),
        abrupt => Err(abrupt),
    }
}

/// Where a `Throw` raised at code offset `at` resumes: the innermost handler
/// covering it (the compiler lists inner ranges first). The operand stack and
/// block environments are cut back to the handler's depth and the thrown
/// value is pushed. `Err` is the completion the chunk leaves with instead.
#[allow(clippy::too_many_arguments)]
#[cold]
#[inline(never)]
fn resume_at_handler(
    interp: &mut Interpreter,
    chunk: &Chunk,
    stack: &mut Vec<JsValue>,
    refs: &mut Vec<IdentifierRef>,
    scopes: &mut SmallVec<[EnvRef; 4]>,
    envs_base: usize,
    roots_base: usize,
    at: u32,
    thrown: JsValue,
) -> Result<usize, Completion> {
    let Some(handler) = chunk.handlers.iter().find(|h| h.start <= at && at < h.end) else {
        return Err(Completion::Throw(thrown));
    };
    if handler.checks_exit
        && let Some(code) = interp.pending_exit
    {
        return Err(Completion::Exit(code));
    }
    stack.truncate(usize::from(handler.stack_height));
    let kept_roots = roots_base + stack.iter().filter(|v| v.as_object_id().is_some()).count();
    if interp.gc_bytecode_roots.len() > kept_roots {
        interp.gc_bytecode_roots.truncate(kept_roots);
    }
    refs.clear();
    unwind_scopes(interp, scopes, envs_base, usize::from(handler.scope_depth));
    push_value(interp, stack, thrown);
    Ok(handler.target as usize)
}

pub(crate) fn run_chunk(
    interp: &mut Interpreter,
    chunk: &Chunk,
    env: &EnvRef,
    this_value: JsValue,
) -> Completion {
    run_chunk_with_var_prologue(interp, chunk, env, this_value, true)
}

/// Run a Script chunk after its caller has completed
/// GlobalDeclarationInstantiation. Unlike function chunks, Script chunks must
/// not create declarative `var` bindings: global vars remain backed by global
/// object properties, including properties that predate the Script.
pub(crate) fn run_script_chunk(
    interp: &mut Interpreter,
    chunk: &Chunk,
    env: &EnvRef,
    this_value: JsValue,
) -> Completion {
    run_chunk_with_var_prologue(interp, chunk, env, this_value, false)
}

const MAX_POOLED_VM_STACKS: usize = 64;
const MAX_POOLED_VM_STACK_CAPACITY: usize = 1024;

/// Drops whatever an abrupt exit left in `buf` (it may hold environments) and
/// keeps the allocation for the next activation; empty and oversized buffers
/// are not worth pinning.
fn recycle_vm_buffer<T>(pool: &mut Vec<Vec<T>>, mut buf: Vec<T>) {
    buf.clear();
    if buf.capacity() != 0
        && pool.len() < MAX_POOLED_VM_STACKS
        && buf.capacity() <= MAX_POOLED_VM_STACK_CAPACITY
    {
        pool.push(buf);
    }
}

fn run_chunk_with_var_prologue(
    interp: &mut Interpreter,
    chunk: &Chunk,
    env: &EnvRef,
    this_value: JsValue,
    declare_chunk_vars: bool,
) -> Completion {
    let gc_frame = interp.gc_bytecode_roots.len();
    let envs_frame = interp.call_stack_envs.len();
    let mut stack = interp.vm_stack_pool.pop().unwrap_or_default();
    let mut refs = if chunk.max_refs == 0 {
        Vec::new()
    } else {
        interp.vm_ref_pool.pop().unwrap_or_default()
    };
    stack.reserve(chunk.max_stack as usize);
    refs.reserve(chunk.max_refs as usize);
    let result = run_chunk_inner(
        interp,
        chunk,
        env,
        this_value,
        declare_chunk_vars,
        &mut stack,
        &mut refs,
    );
    recycle_vm_buffer(&mut interp.vm_stack_pool, stack);
    recycle_vm_buffer(&mut interp.vm_ref_pool, refs);
    // `Throw`/`Exit` are abrupt and can leave operands from an
    // outer-in-progress expression still on the stack (issue #331) — every
    // other completion is produced only after the chunk's own opcode
    // handlers have already popped/unrooted their one live value.
    if !matches!(result, Completion::Throw(_) | Completion::Exit(_)) {
        interp
            .gc_bytecode_roots
            .assert_depth(gc_frame, "a bytecode chunk exit");
    }
    interp.gc_bytecode_roots.truncate(gc_frame);
    // Block environments a `return`, `throw` or escaping `break` left open.
    interp.call_stack_envs.truncate(envs_frame);
    result
}

/// Hits a GC safepoint, asserting the VM's two scratch stacks are empty —
/// both safepoint sites (a loop back-edge, a statement boundary) land
/// between statements, where the compiler guarantees a net-zero stack.
#[inline(always)]
fn safepoint_with_empty_stacks(
    interp: &mut Interpreter,
    stack: &[JsValue],
    persistent: usize,
    refs: &[IdentifierRef],
    context: &str,
) {
    debug_assert!(stack.len() <= persistent, "operand stack live at {context}");
    debug_assert!(refs.is_empty(), "reference stack live at {context}");
    interp.gc_safepoint_poll();
}

fn run_chunk_inner(
    interp: &mut Interpreter,
    chunk: &Chunk,
    frame_env: &EnvRef,
    _this: JsValue,
    declare_chunk_vars: bool,
    stack: &mut Vec<JsValue>,
    refs: &mut Vec<IdentifierRef>,
) -> Completion {
    interp.bytecode_chunks_executed += 1;
    if declare_chunk_vars {
        declare_var_prologue(chunk, frame_env);
        if let Some(lexicals) = &chunk.top_level_lexicals {
            declare_lexicals(&mut frame_env.borrow_mut(), chunk, lexicals);
        }
    }
    let mut completion_value: Option<JsValue> = None;
    let mut pc: usize = 0;
    // Block environments `PushEnv` opened, innermost last, mirrored in
    // `call_stack_envs` from `envs_base` so the GC sees their bindings. The
    // frame environment stays the one the caller rooted. Local-op slot hints
    // address `frame_env` or one of these directly; every other environment
    // use goes through `cur_env!()`.
    let mut scopes: SmallVec<[EnvRef; 4]> = SmallVec::new();
    let envs_base = interp.call_stack_envs.len();
    let roots_base = interp.gc_bytecode_roots.len();
    // Before an opcode that can run user code with operands pending.
    macro_rules! assert_operands_rooted {
        () => {
            debug_assert_operands_rooted(interp, stack, roots_base, completion_value.as_ref())
        };
    }
    macro_rules! cur_env {
        () => {
            scopes.last().unwrap_or(frame_env)
        };
    }
    // Private Names this frame has resolved, by `Chunk::private_names` index.
    // The Private Environment is fixed for the life of a frame (a class
    // evaluated inside it scopes only its own members' closures), so one
    // walk of the environment chain per name serves every access.
    let mut private_ids: SmallVec<[PrivateId; 4]> =
        SmallVec::from_elem(PrivateId::UNRESOLVED, chunk.private_names.len());
    macro_rules! private_id {
        ($idx:expr) => {{
            let slot = $idx as usize;
            if private_ids[slot] == PrivateId::UNRESOLVED {
                private_ids[slot] =
                    interp.resolve_private_name(&chunk.private_names[slot], frame_env);
            }
            private_ids[slot]
        }};
    }
    // A `Throw` resumes at the innermost handler covering the op (below the
    // dispatch loop); every other abrupt completion leaves the chunk.
    macro_rules! bail {
        ($l:lifetime, $completion:expr) => {
            match $completion {
                Completion::Throw(e) => throw!($l, e),
                other => return other,
            }
        };
    }
    // A chunk without `try`/for-of ranges has nobody to resume: leave at
    // once, exactly as before handlers existed.
    macro_rules! throw {
        ($l:lifetime, $e:expr) => {{
            let thrown = $e;
            if chunk.handlers.is_empty() {
                return Completion::Throw(thrown);
            }
            break $l thrown;
        }};
    }
    // One arm per binary op, so the Number-pair fast path is specialised to
    // the op instead of re-dispatching on it.
    macro_rules! binary_arm {
        ($l:lifetime, $bop:expr) => {{
            let len = stack.len();
            if len >= 2
                && let (Some(l), Some(r)) = (stack[len - 2].as_number(), stack[len - 1].as_number())
            {
                // Numbers are never rooted, so nothing needs unrooting.
                stack.truncate(len - 1);
                stack[len - 2] = number_binop($bop, l, r);
            } else {
                let r = stack.pop().expect("stack underflow on binop rhs");
                let l = stack.pop().expect("stack underflow on binop lhs");
                match binary_slow(interp, binary_of_op($bop), l, r) {
                    Completion::Normal(v) => push_value(interp, stack, v),
                    abrupt => bail!($l, abrupt),
                }
            }
        }};
    }
    loop {
        let thrown: JsValue = 'dispatch: loop {
            let op_byte = chunk.code[pc];
            let op = Op::from_u8(op_byte).expect("invalid opcode");
            #[cfg(feature = "perf-counters")]
            interp.perf.record_op(op);
            pc += 1;
            match op {
                Op::LoadConst => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let v = chunk.constants[idx as usize].to_value();
                    push_value(interp, stack, v);
                }
                Op::LoadName => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let name = &chunk.names[idx as usize];
                    match interp.resolve_name_hinted(name, cur_env!(), chunk.global_hint(idx)) {
                        Completion::Normal(v) => push_value(interp, stack, v),
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::LoadLocal => {
                    let idx = decode_u16(chunk, pc);
                    let slot = decode_u16(chunk, pc + 2);
                    pc += 4;
                    let name = &chunk.names[idx as usize];
                    let (target, hint_slot) = local_target(frame_env, &scopes, slot);
                    match with_local_hit(target, name, hint_slot, JsValue::clone) {
                        Some(value) => {
                            #[cfg(feature = "perf-counters")]
                            {
                                interp.perf.local_slot_hits += 1;
                            }
                            push_value(interp, stack, value);
                        }
                        None => {
                            #[cfg(feature = "perf-counters")]
                            {
                                interp.perf.local_slot_misses += 1;
                            }
                            match interp.resolve_name_hinted(
                                name,
                                cur_env!(),
                                chunk.global_hint(idx),
                            ) {
                                Completion::Normal(v) => push_value(interp, stack, v),
                                abrupt => bail!('dispatch, abrupt),
                            }
                        }
                    }
                }
                Op::StoreLocal | Op::StoreLocalPop => {
                    let idx = decode_u16(chunk, pc);
                    let slot = decode_u16(chunk, pc + 2);
                    pc += 4;
                    let name = &chunk.names[idx as usize];
                    let value = stack.last().expect("stack underflow on StoreLocal");
                    let (target, hint_slot) = local_target(frame_env, &scopes, slot);
                    if let Err(e) = store_local(
                        interp,
                        name,
                        hint_slot,
                        target,
                        cur_env!(),
                        value,
                        chunk.global_hint(idx),
                    ) {
                        throw!('dispatch, e);
                    }
                    if op == Op::StoreLocalPop {
                        pop_value(interp, stack, "stack underflow on StoreLocalPop");
                    }
                }
                Op::UpdateLocal => {
                    let idx = decode_u16(chunk, pc);
                    let slot = decode_u16(chunk, pc + 2);
                    let mode = chunk.code[pc + 4];
                    pc += 5;
                    let (op, prefix, discard) = update_mode(mode);
                    let name = &chunk.names[idx as usize];
                    let (target, hint_slot) = local_target(frame_env, &scopes, slot);
                    let fast = {
                        let mut scope = target.borrow_mut();
                        let plain = scope.with_object.is_none()
                            && scope.indirect_bindings.is_none()
                            && scope.global_object_id.is_none();
                        match scope.bindings.entry_at_mut(hint_slot as usize) {
                            Some((atom, binding))
                                if plain
                                    && atom.ptr_eq(name)
                                    && binding.initialized
                                    && matches!(
                                        binding.kind,
                                        BindingKind::Var | BindingKind::Let
                                    ) =>
                            {
                                binding.value.as_number().map(|old| {
                                    let new = match op {
                                        UpdateOp::Increment => old + 1.0,
                                        UpdateOp::Decrement => old - 1.0,
                                    };
                                    binding.value = JsValue::number(new);
                                    JsValue::number(if prefix { new } else { old })
                                })
                            }
                            _ => None,
                        }
                    };
                    match fast {
                        Some(result) => {
                            #[cfg(feature = "perf-counters")]
                            {
                                interp.perf.local_slot_hits += 1;
                            }
                            if !discard {
                                stack.push(result);
                            }
                        }
                        None => {
                            #[cfg(feature = "perf-counters")]
                            {
                                interp.perf.local_slot_misses += 1;
                            }
                            match interp.eval_identifier_update_hinted(
                                op,
                                prefix,
                                name,
                                cur_env!(),
                                chunk.global_hint(idx),
                            ) {
                                Completion::Normal(value) => {
                                    if !discard {
                                        push_value(interp, stack, value);
                                    }
                                }
                                abrupt => bail!('dispatch, abrupt),
                            }
                        }
                    }
                }
                Op::LoadThis => match interp.resolve_this_binding(cur_env!()) {
                    Completion::Normal(v) => push_value(interp, stack, v),
                    abrupt => bail!('dispatch, abrupt),
                },
                Op::LoadCalleeName => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let name = &chunk.names[idx as usize];
                    let strict = frame_env.borrow().strict;
                    interp.last_identifier_with_base = None;
                    let callee_result = interp.resolve_identifier_hinted(
                        name,
                        cur_env!(),
                        strict,
                        chunk.global_hint(idx),
                    );
                    let this_value = match interp.last_identifier_with_base.take() {
                        Some(id) => JsValue::object(id),
                        None => JsValue::UNDEFINED,
                    };
                    let callee = match callee_result {
                        Completion::Normal(value) => value,
                        abrupt => bail!('dispatch, abrupt),
                    };
                    push_value(interp, stack, callee);
                    push_value(interp, stack, this_value);
                }
                Op::ResolveName => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let name = &chunk.names[idx as usize];
                    let local_ref = {
                        let scope = cur_env!().borrow();
                        scope.with_object.is_none()
                            && scope.indirect_bindings.is_none()
                            && scope.bindings.contains_key(name)
                    };
                    if local_ref {
                        refs.push(IdentifierRef::SpecificEnv(cur_env!().clone()));
                    } else {
                        match interp.resolve_identifier_ref_hinted(
                            name,
                            cur_env!(),
                            chunk.global_hint(idx),
                        ) {
                            Ok(id_ref) => refs.push(id_ref),
                            Err(e) => throw!('dispatch, e),
                        }
                    }
                }
                Op::LoadResolvedName => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let name = &chunk.names[idx as usize];
                    let id_ref = refs
                        .last()
                        .expect("reference stack underflow on LoadResolvedName");
                    match interp.get_identifier_value_by_ref_hinted(
                        name,
                        id_ref,
                        cur_env!(),
                        chunk.global_hint(idx),
                    ) {
                        Completion::Normal(value) => push_value(interp, stack, value),
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::StoreResolvedName | Op::StoreResolvedNamePop => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let name = &chunk.names[idx as usize];
                    let id_ref = refs
                        .pop()
                        .expect("reference stack underflow on StoreResolvedName");
                    let value = stack.last().expect("stack underflow on StoreResolvedName");
                    if let Err(e) = store_resolved_name(
                        interp,
                        name,
                        &id_ref,
                        cur_env!(),
                        value,
                        chunk.global_hint(idx),
                    ) {
                        throw!('dispatch, e);
                    }
                    if op == Op::StoreResolvedNamePop {
                        pop_value(interp, stack, "stack underflow on StoreResolvedNamePop");
                    }
                }
                Op::UpdateName => {
                    let idx = decode_u16(chunk, pc);
                    let mode = chunk.code[pc + 2];
                    pc += 3;
                    let (op, prefix, discard) = update_mode(mode);
                    let name = &chunk.names[idx as usize];
                    match interp.eval_identifier_update_hinted(
                        op,
                        prefix,
                        name,
                        cur_env!(),
                        chunk.global_hint(idx),
                    ) {
                        Completion::Normal(value) => {
                            if !discard {
                                push_value(interp, stack, value);
                            }
                        }
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::GetProp => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let name = chunk.names[idx as usize].clone();
                    assert_operands_rooted!();
                    let base = stack.pop().expect("stack underflow on GetProp");
                    let result = member_get(interp, &base, &name);
                    unroot_stack_value(interp, &base);
                    match result {
                        Completion::Normal(v) => push_value(interp, stack, v),
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::GetPrivate | Op::LoadMethodPrivate => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let id = private_id!(idx);
                    assert_operands_rooted!();
                    let base = stack.pop().expect("stack underflow on private read");
                    let result =
                        interp.private_get_resolved(&base, id, &chunk.private_names[idx as usize]);
                    unroot_stack_value(interp, &base);
                    match result {
                        Completion::Normal(v) => {
                            push_value(interp, stack, v);
                            if op == Op::LoadMethodPrivate {
                                push_value(interp, stack, base);
                            }
                        }
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::SetPrivate => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let id = private_id!(idx);
                    assert_operands_rooted!();
                    let rhs = stack.pop().expect("stack underflow on SetPrivate rhs");
                    let base = stack.pop().expect("stack underflow on SetPrivate base");
                    let result = interp.set_private_resolved(
                        &base,
                        id,
                        &chunk.private_names[idx as usize],
                        rhs.clone(),
                    );
                    unroot_stack_value(interp, &rhs);
                    unroot_stack_value(interp, &base);
                    match result {
                        Ok(()) => push_value(interp, stack, rhs),
                        Err(e) => throw!('dispatch, e),
                    }
                }
                Op::UpdatePrivate => {
                    let idx = decode_u16(chunk, pc);
                    let mode = chunk.code[pc + 2];
                    pc += 3;
                    let (update, prefix, _) = update_mode(mode);
                    let id = private_id!(idx);
                    let base = stack.pop().expect("stack underflow on UpdatePrivate");
                    let result = interp.update_private_resolved(
                        &base,
                        id,
                        &chunk.private_names[idx as usize],
                        update,
                        prefix,
                    );
                    unroot_stack_value(interp, &base);
                    match result {
                        Completion::Normal(v) => push_value(interp, stack, v),
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::PrivateIn => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let id = private_id!(idx);
                    let object = pop_value(interp, stack, "stack underflow on PrivateIn");
                    match interp.private_in(id, &object) {
                        Completion::Normal(v) => push_value(interp, stack, v),
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::GetElement => {
                    let key_val = stack.last().expect("stack underflow on GetElement key");
                    let base_index = stack
                        .len()
                        .checked_sub(2)
                        .expect("stack underflow on GetElement base");
                    let base = &stack[base_index];
                    let fast_value = key_val
                        .as_number()
                        .and_then(|index| interp.numeric_index_fast_get(base, index));

                    if let Some(value) = fast_value {
                        pop_value(interp, stack, "stack underflow on GetElement key");
                        pop_value(interp, stack, "stack underflow on GetElement base");
                        push_value(interp, stack, value);
                    } else {
                        assert_operands_rooted!();
                        let key_val = stack.pop().expect("stack underflow on GetElement key");
                        let base = stack.pop().expect("stack underflow on GetElement base");
                        let result = member_get_computed(interp, &base, &key_val);
                        unroot_stack_value(interp, &key_val);
                        unroot_stack_value(interp, &base);
                        match result {
                            Completion::Normal(v) => push_value(interp, stack, v),
                            abrupt => bail!('dispatch, abrupt),
                        }
                    }
                }
                Op::LoadMethod => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let name = &chunk.names[idx as usize];
                    assert_operands_rooted!();
                    let base = stack.pop().expect("stack underflow on LoadMethod");
                    let result = member_get(interp, &base, name);
                    unroot_stack_value(interp, &base);
                    match result {
                        Completion::Normal(method) => {
                            push_value(interp, stack, method);
                            push_value(interp, stack, base);
                        }
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::LoadMethodElement => {
                    assert_operands_rooted!();
                    let key_val = stack
                        .pop()
                        .expect("stack underflow on LoadMethodElement key");
                    let base = stack
                        .pop()
                        .expect("stack underflow on LoadMethodElement base");
                    let result = member_get_computed(interp, &base, &key_val);
                    unroot_stack_value(interp, &key_val);
                    unroot_stack_value(interp, &base);
                    match result {
                        Completion::Normal(method) => {
                            push_value(interp, stack, method);
                            push_value(interp, stack, base);
                        }
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::SetProp => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let name = chunk.names[idx as usize].clone();
                    assert_operands_rooted!();
                    let rhs = stack.pop().expect("stack underflow on SetProp rhs");
                    let base = stack.pop().expect("stack underflow on SetProp base");
                    let base_root = base.clone();
                    let strict = frame_env.borrow().strict;
                    let result = member_set(interp, base, &name, rhs.clone(), strict);
                    unroot_stack_value(interp, &rhs);
                    unroot_stack_value(interp, &base_root);
                    match result {
                        Ok(()) => push_value(interp, stack, rhs),
                        Err(e) => throw!('dispatch, e),
                    }
                }
                Op::SetElement => {
                    let len = stack.len();
                    if len >= 3
                        && let Some(index) = stack[len - 2].as_number()
                        && interp.numeric_index_fast_set(&stack[len - 3], index, &stack[len - 1])
                    {
                        let rhs = pop_value(interp, stack, "stack underflow on SetElement rhs");
                        pop_value(interp, stack, "stack underflow on SetElement key");
                        pop_value(interp, stack, "stack underflow on SetElement base");
                        push_value(interp, stack, rhs);
                        continue;
                    }
                    assert_operands_rooted!();
                    let rhs = stack.pop().expect("stack underflow on SetElement rhs");
                    let key_val = stack.pop().expect("stack underflow on SetElement key");
                    let base = stack.pop().expect("stack underflow on SetElement base");
                    let base_root = base.clone();
                    let strict = frame_env.borrow().strict;
                    let result = member_set_computed(interp, base, &key_val, rhs.clone(), strict);
                    unroot_stack_value(interp, &rhs);
                    unroot_stack_value(interp, &key_val);
                    unroot_stack_value(interp, &base_root);
                    match result {
                        Ok(()) => push_value(interp, stack, rhs),
                        Err(e) => throw!('dispatch, e),
                    }
                }
                Op::LoadUndefined => {
                    push_value(interp, stack, JsValue::UNDEFINED);
                }
                Op::LoadTrue => {
                    push_value(interp, stack, JsValue::TRUE);
                }
                Op::LoadFalse => {
                    push_value(interp, stack, JsValue::FALSE);
                }
                Op::LoadNull => {
                    push_value(interp, stack, JsValue::NULL);
                }
                Op::Return => {
                    let v = if stack.is_empty() {
                        JsValue::UNDEFINED
                    } else {
                        pop_value(interp, stack, "stack underflow on Return")
                    };
                    return Completion::Return(v);
                }
                Op::ReturnUndefined => {
                    return Completion::Return(JsValue::UNDEFINED);
                }
                Op::ReturnCompletion => {
                    return match completion_value.take() {
                        Some(value) => {
                            unroot_stack_value(interp, &value);
                            Completion::Normal(value)
                        }
                        None => Completion::Empty,
                    };
                }
                Op::Call | Op::ReturnCall => {
                    let argc = decode_u16(chunk, pc) as usize;
                    pc += 2;
                    let site_id = CallSiteId(decode_u32(chunk, pc));
                    pc += 4;
                    // Counted here, before the strict tail-call return below: that
                    // path leaves the VM without ever reaching the dispatch site,
                    // so incrementing later would omit every strict tail call from
                    // `calls_from_vm` and understate VM-issued calls on
                    // tail-call-heavy code.
                    #[cfg(feature = "perf-counters")]
                    {
                        interp.perf.calls_from_vm += 1;
                    }
                    if op == Op::ReturnCall && frame_env.borrow().strict {
                        let (callee, this_value, args) = take_call_operands(stack, argc);
                        release_call_operands(interp, &callee, &this_value, &args);
                        // Persistent slots below the call (a for-in enumerator, a
                        // `finally` completion record) die with the frame.
                        truncate_stack(interp, stack, 0);
                        return Completion::TailCall {
                            func: callee,
                            this: this_value,
                            args,
                        };
                    }
                    assert!(stack.len() >= argc + 2, "stack underflow on call operands");
                    let base = stack.len() - argc - 2;
                    let callee = stack[base].clone();
                    let this_value = stack[base + 1].clone();
                    // Call-site IC probe + record, mirroring eval_call's tree-walker
                    // sequence (issue #432). `with_scope_depth != 0` is excluded
                    // because a `with`-resolved binding can dynamically pick a
                    // different callee per invocation, defeating monomorphic
                    // caching.
                    let ic_callee_id =
                        if site_id != CallSiteId::UNASSIGNED && interp.with_scope_depth == 0 {
                            callee.as_object_id()
                        } else {
                            None
                        };
                    let mut probe_hit = false;
                    if let Some(callee_id) = ic_callee_id {
                        let slot = *interp.call_slot(site_id);
                        if let CallIcSlot::Mono {
                            callee_obj_id,
                            callee_shape_id,
                            ..
                        } = slot
                            && callee_id == callee_obj_id
                            && let Some(obj_rc) = interp.get_object(callee_id)
                            && obj_rc.borrow().shape_id == callee_shape_id
                        {
                            interp
                                .call_ic_hit_count
                                .set(interp.call_ic_hit_count.get() + 1);
                            probe_hit = true;
                        }
                        if !probe_hit {
                            interp
                                .call_ic_slow_path_count
                                .set(interp.call_ic_slow_path_count.get() + 1);
                        }
                    }
                    // The operands stay on the operand stack (and in
                    // gc_bytecode_roots) for the complete nested invocation: the
                    // callee borrows its arguments straight from the stack and can
                    // execute arbitrary JS and hit any number of safepoints.
                    let result = if probe_hit {
                        interp.call_function_ic_validated(&callee, &this_value, &stack[base + 2..])
                    } else {
                        interp.call_function(&callee, &this_value, &stack[base + 2..])
                    };
                    // Record only on success to avoid caching error-paths.
                    if let Some(callee_id) = ic_callee_id
                        && !probe_hit
                        && matches!(result, Completion::Normal(_))
                    {
                        let slot = *interp.call_slot(site_id);
                        let new_slot = interp.classify_for_call_ic(callee_id);
                        let next = match (slot, new_slot) {
                            (CallIcSlot::Megamorphic, _) => CallIcSlot::Megamorphic,
                            (_, None) => CallIcSlot::Empty,
                            (CallIcSlot::Empty, Some(s)) => s,
                            (
                                CallIcSlot::Mono {
                                    callee_obj_id: prev,
                                    ..
                                },
                                Some(
                                    s @ CallIcSlot::Mono {
                                        callee_obj_id: new, ..
                                    },
                                ),
                            ) if prev == new => s,
                            (CallIcSlot::Mono { .. }, Some(_)) => CallIcSlot::Megamorphic,
                        };
                        *interp.call_slot(site_id) = next;
                    }
                    pop_operands(interp, stack, argc + 2);
                    match result {
                        Completion::Normal(value) | Completion::Return(value) => {
                            push_value(interp, stack, value);
                        }
                        Completion::Empty => {
                            push_value(interp, stack, JsValue::UNDEFINED);
                        }
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::Construct => {
                    let argc = decode_u16(chunk, pc) as usize;
                    pc += 2;
                    let _site_id = CallSiteId(decode_u32(chunk, pc));
                    pc += 4;
                    assert!(stack.len() > argc, "stack underflow on construct operands");
                    let base = stack.len() - argc - 1;
                    let callee = stack[base].clone();
                    // The operands stay on the operand stack (and in
                    // gc_bytecode_roots) for the complete nested construction,
                    // mirroring Op::Call: construct_from_evaluated can run
                    // arbitrary JS (constructor body, field initializers) and hit
                    // any number of safepoints before returning.
                    let result = interp.construct_from_evaluated(&callee, &stack[base + 1..]);
                    pop_operands(interp, stack, argc + 1);
                    match result {
                        Completion::Normal(value) | Completion::Return(value) => {
                            push_value(interp, stack, value);
                        }
                        Completion::Empty => {
                            push_value(interp, stack, JsValue::UNDEFINED);
                        }
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::Add => binary_arm!('dispatch, Op::Add),
                Op::Sub => binary_arm!('dispatch, Op::Sub),
                Op::Mul => binary_arm!('dispatch, Op::Mul),
                Op::Div => binary_arm!('dispatch, Op::Div),
                Op::Mod => binary_arm!('dispatch, Op::Mod),
                Op::Pow => binary_arm!('dispatch, Op::Pow),
                Op::Eq => binary_arm!('dispatch, Op::Eq),
                Op::NotEq => binary_arm!('dispatch, Op::NotEq),
                Op::StrictEq => binary_arm!('dispatch, Op::StrictEq),
                Op::StrictNotEq => binary_arm!('dispatch, Op::StrictNotEq),
                Op::Lt => binary_arm!('dispatch, Op::Lt),
                Op::Gt => binary_arm!('dispatch, Op::Gt),
                Op::LtEq => binary_arm!('dispatch, Op::LtEq),
                Op::GtEq => binary_arm!('dispatch, Op::GtEq),
                Op::BitAnd => binary_arm!('dispatch, Op::BitAnd),
                Op::BitOr => binary_arm!('dispatch, Op::BitOr),
                Op::BitXor => binary_arm!('dispatch, Op::BitXor),
                Op::Shl => binary_arm!('dispatch, Op::Shl),
                Op::Shr => binary_arm!('dispatch, Op::Shr),
                Op::UShr => binary_arm!('dispatch, Op::UShr),
                Op::BinConst => {
                    let bop = Op::from_u8(chunk.code[pc]).expect("invalid binary op byte");
                    let Constant::Number(k) = chunk.constants[decode_u16(chunk, pc + 1) as usize]
                    else {
                        unreachable!("BinConst constant is a Number")
                    };
                    pc += 3;
                    let top = stack.last_mut().expect("stack underflow on BinConst");
                    if let Some(l) = top.as_number() {
                        *top = number_binop(bop, l, k);
                    } else {
                        let l = stack.pop().expect("stack underflow on BinConst");
                        match binary_slow(interp, binary_of_op(bop), l, JsValue::number(k)) {
                            Completion::Normal(v) => push_value(interp, stack, v),
                            abrupt => bail!('dispatch, abrupt),
                        }
                    }
                }
                Op::LocalBinConst => {
                    let idx = decode_u16(chunk, pc);
                    let slot = decode_u16(chunk, pc + 2);
                    let bop = Op::from_u8(chunk.code[pc + 4]).expect("invalid binary op byte");
                    let Constant::Number(k) = chunk.constants[decode_u16(chunk, pc + 5) as usize]
                    else {
                        unreachable!("LocalBinConst constant is a Number")
                    };
                    pc += 7;
                    let name = &chunk.names[idx as usize];
                    let (target, hint_slot) = local_target(frame_env, &scopes, slot);
                    let hit = with_local_hit(target, name, hint_slot, |v| match v.as_number() {
                        Some(l) => Ok(number_binop(bop, l, k)),
                        None => Err(v.clone()),
                    });
                    let operand = match hit {
                        Some(Ok(result)) => {
                            #[cfg(feature = "perf-counters")]
                            {
                                interp.perf.local_slot_hits += 1;
                            }
                            stack.push(result);
                            None
                        }
                        Some(Err(l)) => {
                            #[cfg(feature = "perf-counters")]
                            {
                                interp.perf.local_slot_hits += 1;
                            }
                            Some(l)
                        }
                        None => {
                            #[cfg(feature = "perf-counters")]
                            {
                                interp.perf.local_slot_misses += 1;
                            }
                            match interp.resolve_name_hinted(
                                name,
                                cur_env!(),
                                chunk.global_hint(idx),
                            ) {
                                Completion::Normal(v) => Some(v),
                                abrupt => bail!('dispatch, abrupt),
                            }
                        }
                    };
                    if let Some(l) = operand {
                        // As `LoadLocal` then `BinConst`: the loaded value is
                        // rooted for the whole operation (an object operand's
                        // `valueOf` may rebind the local and collect).
                        root_stack_value(interp, &l);
                        match binary_slow(interp, binary_of_op(bop), l, JsValue::number(k)) {
                            Completion::Normal(v) => push_value(interp, stack, v),
                            abrupt => bail!('dispatch, abrupt),
                        }
                    }
                }
                Op::CmpJumpIfFalse => {
                    let cmp = Op::from_u8(chunk.code[pc]).expect("invalid comparison byte");
                    let offset = decode_i16(chunk, pc + 1) as i32;
                    pc += 3;
                    let len = stack.len();
                    let taken = if len >= 2
                        && let (Some(l), Some(r)) =
                            (stack[len - 2].as_number(), stack[len - 1].as_number())
                    {
                        stack.truncate(len - 2);
                        !number_compare(cmp, l, r)
                    } else {
                        let r = stack.pop().expect("stack underflow on CmpJumpIfFalse rhs");
                        let l = stack.pop().expect("stack underflow on CmpJumpIfFalse lhs");
                        match binary_slow(interp, binary_of_op(cmp), l, r) {
                            Completion::Normal(v) => !interp.to_boolean_val(&v),
                            abrupt => bail!('dispatch, abrupt),
                        }
                    };
                    if taken {
                        pc = (pc as i32 + offset) as usize;
                    }
                }
                Op::Neg | Op::Plus | Op::Not | Op::BitNot => {
                    let v = stack.pop().expect("stack underflow on unary");
                    let uop = match op {
                        Op::Neg => UnaryOp::Minus,
                        Op::Plus => UnaryOp::Plus,
                        Op::Not => UnaryOp::Not,
                        Op::BitNot => UnaryOp::BitNot,
                        _ => unreachable!(),
                    };
                    let result = interp.eval_unary(uop, &v);
                    unroot_stack_value(interp, &v);
                    match result {
                        Completion::Normal(r) => push_value(interp, stack, r),
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::EvalAst => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let super::chunk::AstNode::Expr(expr) = &chunk.ast[idx as usize] else {
                        unreachable!("EvalAst operand names an expression node");
                    };
                    #[cfg(feature = "perf-counters")]
                    {
                        interp.perf.ast_fallback_execs += 1;
                    }
                    // Operands already on the stack stay rooted in
                    // `gc_bytecode_roots` while the tree-walker runs.
                    match interp.eval_expr(expr, cur_env!()) {
                        Completion::Normal(v) => push_value(interp, stack, v),
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::ExecAst => {
                    let idx = decode_u16(chunk, pc);
                    let table = decode_u16(chunk, pc + 2);
                    let flags = chunk.code[pc + 4];
                    pc += 5;
                    let super::chunk::AstNode::Stmt(stmt) = &chunk.ast[idx as usize] else {
                        unreachable!("ExecAst operand names a statement node");
                    };
                    #[cfg(feature = "perf-counters")]
                    {
                        interp.perf.ast_fallback_execs += 1;
                    }
                    // Inside a `try` block the statement's calls are never in
                    // tail position: the handler must stay on the stack.
                    let saved_tco = interp.tco_suppress_depth;
                    if flags & EXEC_NO_TAIL_CALL != 0 {
                        interp.tco_suppress_depth = saved_tco + 1;
                    }
                    let completion = interp.exec_statement(stmt, cur_env!());
                    interp.tco_suppress_depth = saved_tco;
                    let set_completion = flags & EXEC_SET_COMPLETION != 0;
                    match completion {
                        Completion::Normal(value) => {
                            if set_completion {
                                set_script_completion(interp, &mut completion_value, value);
                            }
                        }
                        Completion::Empty => {}
                        Completion::Break(label, value) => {
                            if set_completion {
                                carry_script_completion(interp, &mut completion_value, &value);
                            }
                            match escape_target(chunk, table, false, label.as_deref()) {
                                Some((target, depth, height)) => {
                                    unwind_scopes(interp, &mut scopes, envs_base, depth);
                                    truncate_stack(interp, stack, height);
                                    pc = target;
                                }
                                None => return Completion::Break(label, value),
                            }
                        }
                        Completion::Continue(label, value) => {
                            if set_completion {
                                carry_script_completion(interp, &mut completion_value, &value);
                            }
                            match escape_target(chunk, table, true, label.as_deref()) {
                                Some((target, depth, height)) => {
                                    unwind_scopes(interp, &mut scopes, envs_base, depth);
                                    truncate_stack(interp, stack, height);
                                    pc = target;
                                }
                                None => return Completion::Continue(label, value),
                            }
                        }
                        left @ (Completion::Return(_) | Completion::TailCall { .. }) => {
                            truncate_stack(interp, stack, 0);
                            return left;
                        }
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::InitLexical => {
                    let idx = decode_u16(chunk, pc);
                    let kind = if chunk.code[pc + 2] == 0 {
                        BindingKind::Let
                    } else {
                        BindingKind::Const
                    };
                    let slot = decode_u16(chunk, pc + 3);
                    pc += 5;
                    let name = &chunk.names[idx as usize];
                    let value = pop_value(interp, stack, "stack underflow on InitLexical");
                    let binding = Binding {
                        value,
                        kind,
                        initialized: true,
                        deletable: false,
                    };
                    let mut scope = cur_env!().borrow_mut();
                    match scope.bindings.entry_at_mut(usize::from(slot)) {
                        Some((atom, entry)) if atom.ptr_eq(name) => *entry = binding,
                        _ => {
                            scope.bindings.insert_atom(name, binding);
                        }
                    }
                }
                Op::Instanceof | Op::In => {
                    let r = stack.pop().expect("stack underflow on relational rhs");
                    let l = stack.pop().expect("stack underflow on relational lhs");
                    let result = if op == Op::Instanceof {
                        interp.eval_instanceof(&l, &r)
                    } else {
                        interp.eval_binary(BinaryOp::In, &l, &r)
                    };
                    unroot_stack_value(interp, &r);
                    unroot_stack_value(interp, &l);
                    match result {
                        Completion::Normal(v) => push_value(interp, stack, v),
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::Throw => {
                    let value = pop_value(interp, stack, "stack underflow on Throw");
                    throw!('dispatch, value);
                }
                Op::Typeof => {
                    let value = pop_value(interp, stack, "stack underflow on Typeof");
                    let result = interp.typeof_value(&value);
                    push_value(interp, stack, result);
                }
                Op::TypeofName => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let name = &chunk.names[idx as usize];
                    match interp.eval_typeof_identifier(name, cur_env!()) {
                        Completion::Normal(v) => push_value(interp, stack, v),
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::UpdateProp => {
                    let idx = decode_u16(chunk, pc);
                    let mode = chunk.code[pc + 2];
                    pc += 3;
                    let (update, prefix, _) = update_mode(mode);
                    let name = &chunk.names[idx as usize];
                    let base = stack.pop().expect("stack underflow on UpdateProp");
                    let strict = frame_env.borrow().strict;
                    let result = member_update_named(interp, &base, name, update, prefix, strict);
                    unroot_stack_value(interp, &base);
                    match result {
                        Completion::Normal(v) => push_value(interp, stack, v),
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::UpdateElement => {
                    let mode = chunk.code[pc];
                    pc += 1;
                    let (update, prefix, _) = update_mode(mode);
                    let key = stack.pop().expect("stack underflow on UpdateElement key");
                    let base = stack.pop().expect("stack underflow on UpdateElement base");
                    let strict = frame_env.borrow().strict;
                    let result =
                        member_update_computed(interp, &base, &key, update, prefix, strict);
                    unroot_stack_value(interp, &key);
                    unroot_stack_value(interp, &base);
                    match result {
                        Completion::Normal(v) => push_value(interp, stack, v),
                        abrupt => bail!('dispatch, abrupt),
                    }
                }
                Op::Pop => {
                    pop_value(interp, stack, "stack underflow on Pop");
                }
                Op::SetCompletion => {
                    let value = pop_value(interp, stack, "stack underflow on SetCompletion");
                    set_script_completion(interp, &mut completion_value, value);
                }
                Op::Jump => {
                    let offset = decode_i16(chunk, pc) as i32;
                    if offset < 0 {
                        safepoint_with_empty_stacks(
                            interp,
                            stack,
                            usize::from(chunk.persistent_slots),
                            refs,
                            "loop backedge",
                        );
                    }
                    pc = (pc as i32 + 2 + offset) as usize;
                }
                Op::JumpIfFalse => {
                    let offset = decode_i16(chunk, pc) as i32;
                    pc += 2;
                    let v = pop_value(interp, stack, "stack underflow on JumpIfFalse");
                    if !interp.to_boolean_val(&v) {
                        pc = (pc as i32 + offset) as usize;
                    }
                }
                Op::JumpIfTrue => {
                    let offset = decode_i16(chunk, pc) as i32;
                    pc += 2;
                    let v = pop_value(interp, stack, "stack underflow on JumpIfTrue");
                    if interp.to_boolean_val(&v) {
                        pc = (pc as i32 + offset) as usize;
                    }
                }
                Op::JumpIfTruthyKeep => {
                    let offset = decode_i16(chunk, pc) as i32;
                    pc += 2;
                    let v = stack.last().expect("stack underflow on JumpIfTruthyKeep");
                    if interp.to_boolean_val(v) {
                        pc = (pc as i32 + offset) as usize;
                    }
                }
                Op::JumpIfFalsyKeep => {
                    let offset = decode_i16(chunk, pc) as i32;
                    pc += 2;
                    let v = stack.last().expect("stack underflow on JumpIfFalsyKeep");
                    if !interp.to_boolean_val(v) {
                        pc = (pc as i32 + offset) as usize;
                    }
                }
                Op::DupN => {
                    let count = chunk.code[pc] as usize;
                    pc += 1;
                    let start = stack
                        .len()
                        .checked_sub(count)
                        .expect("stack underflow on DupN");
                    for i in 0..count {
                        let v = stack[start + i].clone();
                        push_value(interp, stack, v);
                    }
                }
                Op::ToPrimitiveKey => {
                    // sec-getvalue: the base-nullish check runs before ToPropertyKey's
                    // ToPrimitive side effect, so peek the base (one below the key,
                    // don't pop it) and check it first, without touching the key.
                    let base_index = stack
                        .len()
                        .checked_sub(2)
                        .expect("stack underflow on ToPrimitiveKey base");
                    if stack[base_index].is_nullish() {
                        let base = stack[base_index].clone();
                        let error =
                            interp.create_type_error(&format!("Cannot read properties of {base}"));
                        throw!('dispatch, error);
                    }
                    assert_operands_rooted!();
                    let key = stack.pop().expect("stack underflow on ToPrimitiveKey key");
                    let result = interp.to_primitive(&key, "string");
                    unroot_stack_value(interp, &key);
                    match result {
                        Ok(v) => push_value(interp, stack, v),
                        Err(e) => throw!('dispatch, e),
                    }
                }
                Op::JumpIfNotNullishKeep => {
                    let offset = decode_i16(chunk, pc) as i32;
                    pc += 2;
                    let v = stack
                        .last()
                        .expect("stack underflow on JumpIfNotNullishKeep");
                    if !v.is_nullish() {
                        pc = (pc as i32 + offset) as usize;
                    }
                }
                Op::PushEnv => {
                    let scope = &chunk.block_scopes[decode_u16(chunk, pc) as usize];
                    pc += 2;
                    let parent = cur_env!().clone();
                    let block_env = interp.acquire_block_environment(
                        parent,
                        scope.functions.len() + scope.lexicals.len(),
                    );
                    if scope.simple_catch {
                        block_env.borrow_mut().is_simple_catch_scope = true;
                    }
                    // Reachable from the GC before any closure is built into it.
                    interp.call_stack_envs.push(block_env.clone());
                    for stmt in &scope.functions {
                        interp.hoist_block_function_declaration(stmt, &block_env);
                    }
                    if !scope.lexicals.is_empty() {
                        declare_lexicals(&mut block_env.borrow_mut(), chunk, &scope.lexicals);
                    }
                    scopes.push(block_env);
                    debug_assert_eq!(interp.call_stack_envs.len(), envs_base + scopes.len());
                }
                Op::PopEnv => {
                    let count = usize::from(chunk.code[pc]);
                    pc += 1;
                    let depth = scopes.len() - count;
                    unwind_scopes(interp, &mut scopes, envs_base, depth);
                }
                Op::CopyIterEnv => {
                    let current = scopes
                        .last()
                        .expect("CopyIterEnv inside a block environment");
                    // Only `scopes` and its `call_stack_envs` mirror own it: no
                    // closure took this iteration's environment, so a fresh copy
                    // could not be told apart from it.
                    if std::rc::Rc::strong_count(current) <= 2 {
                        continue;
                    }
                    let (parent, bindings) = {
                        let source = current.borrow();
                        (
                            source
                                .parent
                                .clone()
                                .expect("block environments have a parent"),
                            source.bindings.capacity(),
                        )
                    };
                    let fresh = interp.acquire_block_environment(parent, bindings);
                    {
                        let source = current.borrow();
                        let mut copy = fresh.borrow_mut();
                        let mut i = 0;
                        while let Some((name, binding)) = source.bindings.entry_at(i) {
                            copy.bindings.push_unique(
                                name.clone(),
                                Binding::new(binding.value.clone(), binding.kind, true),
                            );
                            i += 1;
                        }
                    }
                    let top = scopes.len() - 1;
                    interp.call_stack_envs[envs_base + top] = fresh.clone();
                    let replaced = std::mem::replace(&mut scopes[top], fresh);
                    interp.recycle_block_environment(replaced);
                }
                Op::MakeClosure => {
                    let idx = decode_u16(chunk, pc);
                    let is_method = chunk.code[pc + 2] != 0;
                    pc += 3;
                    let template = &chunk.functions[idx as usize];
                    let closure = interp.make_closure(
                        &template.spec(),
                        cur_env!(),
                        is_method,
                        template.inferred_name.as_ref(),
                    );
                    push_value(interp, stack, closure);
                }
                Op::NewArrayConst => {
                    let idx = decode_u16(chunk, pc);
                    pc += 2;
                    let array = match &chunk.array_templates[idx as usize] {
                        ArrayTemplate::Dense(values) => interp.create_array(values.clone()),
                        ArrayTemplate::Holey(items) => {
                            interp.create_array_with_holes(items.iter().cloned())
                        }
                    };
                    push_value(interp, stack, array);
                }
                Op::NewArray => {
                    let count = usize::from(decode_u16(chunk, pc));
                    pc += 2;
                    let values = take_literal_values(interp, stack, count);
                    let array = interp.create_array(values);
                    push_value(interp, stack, array);
                }
                Op::ArrayAppend => {
                    let count = usize::from(decode_u16(chunk, pc));
                    pc += 2;
                    let values = take_literal_values(interp, stack, count);
                    let array = stack.last().expect("stack underflow on ArrayAppend");
                    interp.array_literal_append(array, values);
                }
                Op::NewObject => {
                    let shape = &chunk.object_shapes[decode_u16(chunk, pc) as usize];
                    pc += 2;
                    let count = shape.props.len();
                    assert!(stack.len() >= count, "stack underflow on NewObject");
                    let object = interp.build_object_literal(shape, &stack[stack.len() - count..]);
                    pop_operands(interp, stack, count);
                    push_value(interp, stack, object);
                }
                Op::ObjectAppend => {
                    let shape = &chunk.object_shapes[decode_u16(chunk, pc) as usize];
                    pc += 2;
                    let count = shape.props.len();
                    assert!(stack.len() > count, "stack underflow on ObjectAppend");
                    let base = stack.len() - count;
                    let object = stack[base - 1].clone();
                    interp.extend_object_literal(&object, shape, &stack[base..]);
                    pop_operands(interp, stack, count);
                }
                Op::NewRegExp => {
                    let template = &chunk.regexps[decode_u16(chunk, pc) as usize];
                    pc += 2;
                    let regexp = interp
                        .alloc_regexp_literal(template.source.clone(), template.flags.clone());
                    push_value(interp, stack, regexp);
                }
                Op::GetIterator | Op::ForInHead => {
                    if let Err(e) = iteration_head(interp, stack, op == Op::ForInHead) {
                        throw!('dispatch, e);
                    }
                }
                Op::IterStep | Op::ForInStep => {
                    let offset = decode_i16(chunk, pc) as i32;
                    pc += 2;
                    match iteration_step(interp, stack, op == Op::ForInStep) {
                        Ok(true) => {}
                        Ok(false) => pc = (pc as i32 + offset) as usize,
                        Err(e) => throw!('dispatch, e),
                    }
                }
                Op::IterClose => {
                    let depth = usize::from(chunk.code[pc]);
                    pc += 1;
                    if let Err(e) = iteration_close(interp, stack, depth) {
                        throw!('dispatch, e);
                    }
                    // A `return()` that called `__host_exit` latched it (the
                    // close cannot carry the exit itself): it must stay
                    // uncatchable and skip every finalizer on the way out.
                    if let Some(code) = interp.pending_exit {
                        return Completion::Exit(code);
                    }
                }
                Op::IterCloseThrow => {
                    let thrown = iteration_close_throw(interp, stack);
                    if let Some(code) = interp.pending_exit {
                        return Completion::Exit(code);
                    }
                    throw!('dispatch, thrown);
                }
                Op::DropBelow => {
                    let has_value = chunk.code[pc] != 0;
                    let count = usize::from(chunk.code[pc + 1]);
                    pc += 2;
                    drop_below(interp, stack, has_value, count);
                }
                Op::EndFinally => {
                    let table = usize::from(decode_u16(chunk, pc));
                    pc += 2;
                    match end_finally(interp, chunk, stack, table) {
                        Ok(Some(target)) => pc = target,
                        Ok(None) => {}
                        Err(value) => throw!('dispatch, value),
                    }
                }
                Op::BindHead => {
                    let head = &chunk.head_bindings[usize::from(decode_u16(chunk, pc))];
                    pc += 2;
                    if let Err(e) = bind_head_op(interp, stack, head, cur_env!()) {
                        bail!('dispatch, e);
                    }
                }
                Op::CheckExit => {
                    if let Some(code) = interp.pending_exit {
                        return Completion::Exit(code);
                    }
                }
                Op::Safepoint => {
                    safepoint_with_empty_stacks(
                        interp,
                        stack,
                        usize::from(chunk.persistent_slots),
                        refs,
                        "statement boundary",
                    );
                }
            }
        };
        // A `Throw` from an op of the protected range resumes at its handler.
        // `pc - 1` lies inside the faulting op.
        match resume_at_handler(
            interp,
            chunk,
            stack,
            refs,
            &mut scopes,
            envs_base,
            roots_base,
            (pc - 1) as u32,
            thrown,
        ) {
            Ok(target) => pc = target,
            Err(completion) => return completion,
        }
    }
}
