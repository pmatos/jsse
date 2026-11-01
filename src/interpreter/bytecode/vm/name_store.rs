//! The by-name store paths of the name ops: `StoreLocal`'s fallback and
//! `StoreResolvedName` through the reference `ResolveName` created.

use super::{hinted_writable, member_pos};
use crate::ast::UpdateOp;
use crate::interpreter::bytecode::chunk::Chunk;
use crate::interpreter::eval::IdentifierRef;
use crate::interpreter::types::{BindingKind, Completion};
use crate::interpreter::{Atom, EnvRef, GlobalHint, Interpreter};
use crate::types::JsValue;

/// `StoreLocal`'s store: the frame slot when the hint verifies, else the
/// by-name reference path.
#[inline(always)]
pub(super) fn store_local(
    interp: &mut Interpreter,
    name: &Atom,
    slot: u16,
    target: &EnvRef,
    env: &EnvRef,
    value: &JsValue,
    (chunk, idx, pc): (&Chunk, u16, usize),
) -> Result<(), JsValue> {
    let hint = chunk.global_hint(idx);
    let stored = match hinted_writable(&mut target.borrow_mut(), name, slot) {
        Some(binding) => {
            binding.value = value.clone();
            true
        }
        None => false,
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
    let done = {
        member_pos::note_store(interp, chunk, pc);
        interp.put_value_by_ref_hinted(name, value.clone(), &id_ref, env, hint)
    };
    interp.node_host.assign_pos = 0;
    match done {
        Completion::Throw(e) => Err(e),
        _ => Ok(()),
    }
}

/// `UpdateLocal`/`UpdateName`'s by-name update, reporting a constant
/// assignment at the position the compiler recorded for the op.
#[inline(always)]
pub(super) fn update_slow(
    interp: &mut Interpreter,
    (chunk, pc): (&Chunk, usize),
    (op, prefix): (UpdateOp, bool),
    name: &Atom,
    env: &EnvRef,
    hint: Option<&GlobalHint>,
) -> Completion {
    let prior = interp.node_host.call_pos;
    member_pos::note_store(interp, chunk, pc);
    let done = interp.eval_identifier_update_hinted(op, prefix, name, env, hint);
    interp.node_host.assign_pos = 0;
    if let Completion::Throw(thrown) = &done {
        interp.node_move_ident_error(thrown, prior);
    }
    done
}

/// `StoreResolvedName`'s store through the reference `ResolveName` created.
#[inline(always)]
pub(super) fn store_resolved_name(
    interp: &mut Interpreter,
    name: &Atom,
    id_ref: &IdentifierRef,
    env: &EnvRef,
    value: &JsValue,
    hint: Option<&GlobalHint>,
    (chunk, pc): (&Chunk, usize),
) -> Result<(), JsValue> {
    let local_store = if let IdentifierRef::SpecificEnv(local_env) = id_ref {
        let mut scope = local_env.borrow_mut();
        let global = scope.global_object_id.is_some();
        if scope.indirect_bindings.is_none()
            && let Some(binding) = scope.bindings.get_mut(name)
            && binding.initialized
            && match binding.kind {
                BindingKind::Var => !global,
                // A global `let` is not mirrored on the global object.
                BindingKind::Let => true,
                _ => false,
            }
        {
            binding.value = value.clone();
            true
        } else {
            false
        }
    } else {
        false
    };
    if !local_store {
        member_pos::note_store(interp, chunk, pc);
        let done = interp.put_value_by_ref_hinted(name, value.clone(), id_ref, env, hint);
        interp.node_host.assign_pos = 0;
        if let Completion::Throw(e) = done {
            return Err(e);
        }
    }
    Ok(())
}
