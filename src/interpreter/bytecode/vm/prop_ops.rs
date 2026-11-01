use super::{
    finish_named_get, member_set, member_update_named, pop_value, push_value, root_stack_value,
    unroot_stack_value, update_mode,
};
use crate::ast::UpdateOp;
use crate::interpreter::property::NamedGet;
use crate::interpreter::types::Completion;
use crate::interpreter::{Atom, EnvRef, Interpreter};
use crate::types::JsValue;
use std::cell::Cell;

/// The named-property ops (`GetProp`, `LoadMethod`, `SetProp`, `UpdateProp`)
/// live out of line, fast path and generic path together, so the dispatch loop
/// carries one call per op and its code generation is unaffected.
///
/// `GetProp`: replaces the base on top of the stack with the property value.
#[inline(never)]
pub(super) fn get_prop_op(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    name: &Atom,
    hint: Option<&Cell<u32>>,
) -> Result<(), Completion> {
    let slot = stack.last_mut().expect("stack underflow on GetProp");
    let getter = match interp.named_get_fast(slot, name.property_key(), hint) {
        NamedGet::Value(value) => {
            let base = std::mem::replace(slot, value);
            unroot_stack_value(interp, &base);
            root_stack_value(interp, slot);
            return Ok(());
        }
        NamedGet::Getter(getter) => Some(getter),
        NamedGet::Slow => None,
    };
    let base = stack.pop().expect("checked above");
    let value = finish_named_get(interp, &base, name, getter)?;
    push_value(interp, stack, value);
    Ok(())
}

/// `LoadMethod`: the base stays as the call's `this`, under the method.
#[inline(never)]
pub(super) fn load_method_op(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    name: &Atom,
    hint: Option<&Cell<u32>>,
) -> Result<(), Completion> {
    let base = stack.last().expect("stack underflow on LoadMethod");
    let getter = match interp.named_get_fast(base, name.property_key(), hint) {
        NamedGet::Value(method) => {
            let base = pop_value(interp, stack, "stack underflow on LoadMethod");
            push_value(interp, stack, method);
            push_value(interp, stack, base);
            return Ok(());
        }
        NamedGet::Getter(getter) => Some(getter),
        NamedGet::Slow => None,
    };
    let base = stack.pop().expect("checked above");
    let method = finish_named_get(interp, &base, name, getter)?;
    push_value(interp, stack, method);
    push_value(interp, stack, base);
    Ok(())
}

/// `SetProp`: replaces the base and right-hand side with the right-hand side.
#[inline(never)]
pub(super) fn set_prop_op(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    name: &Atom,
    hint: Option<&Cell<u32>>,
    frame_env: &EnvRef,
) -> Result<(), JsValue> {
    let len = stack.len();
    if len >= 2
        && interp.named_store_fast(&stack[len - 2], name.property_key(), &stack[len - 1], hint)
    {
        let rhs = pop_value(interp, stack, "stack underflow on SetProp rhs");
        pop_value(interp, stack, "stack underflow on SetProp base");
        push_value(interp, stack, rhs);
        return Ok(());
    }
    let rhs = stack.pop().expect("stack underflow on SetProp rhs");
    let base = stack.pop().expect("stack underflow on SetProp base");
    let base_root = base.clone();
    let strict = frame_env.borrow().strict;
    let result = member_set(interp, base, name, rhs.clone(), strict);
    unroot_stack_value(interp, &rhs);
    unroot_stack_value(interp, &base_root);
    result?;
    push_value(interp, stack, rhs);
    Ok(())
}

/// `UpdateProp`: replaces the base with the update expression's result.
#[inline(never)]
pub(super) fn update_prop_op(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    name: &Atom,
    hint: Option<&Cell<u32>>,
    mode: u8,
    frame_env: &EnvRef,
) -> Result<(), Completion> {
    let (update, prefix, _) = update_mode(mode);
    let delta = match update {
        UpdateOp::Increment => 1.0,
        UpdateOp::Decrement => -1.0,
    };
    let slot = stack.last_mut().expect("stack underflow on UpdateProp");
    if let Some((old, new)) = interp.named_update_fast(slot, name.property_key(), delta, hint) {
        let base = std::mem::replace(slot, JsValue::number(if prefix { new } else { old }));
        unroot_stack_value(interp, &base);
        return Ok(());
    }
    let base = stack.pop().expect("stack underflow on UpdateProp");
    let strict = frame_env.borrow().strict;
    let result = member_update_named(interp, &base, name, update, prefix, strict);
    unroot_stack_value(interp, &base);
    match result {
        Completion::Normal(v) => {
            push_value(interp, stack, v);
            Ok(())
        }
        abrupt => Err(abrupt),
    }
}
