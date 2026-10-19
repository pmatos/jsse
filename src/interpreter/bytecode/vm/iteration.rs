//! Out-of-line handlers of the iteration, cleanup and head-binding ops.

use super::{HeadBinding, bind_head, pop_value, push_value, unroot_stack_value};
use crate::interpreter::types::Completion;
use crate::interpreter::{EnvRef, Interpreter};
use crate::types::JsValue;
use smallvec::SmallVec;

use super::Chunk;

/// `GetIterator` / the for-in head: `[value]` -> `[iterator]`.
#[inline(never)]
pub(super) fn iteration_head(
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
pub(super) fn iteration_step(
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
pub(super) fn iteration_close(
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
pub(super) fn iteration_close_throw(interp: &mut Interpreter, stack: &mut Vec<JsValue>) -> JsValue {
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
pub(super) fn drop_below(
    interp: &mut Interpreter,
    stack: &mut Vec<JsValue>,
    has_value: bool,
    count: usize,
) {
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
pub(super) fn end_finally(
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
pub(super) fn bind_head_op(
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
