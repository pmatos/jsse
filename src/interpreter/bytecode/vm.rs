use super::chunk::Chunk;
use super::op::Op;
use crate::ast::{BinaryOp, CallSiteId, UnaryOp, UpdateOp};
use crate::interpreter::eval::{IdentifierRef, is_primitive_string_concat};
use crate::interpreter::ic::CallIcSlot;
use crate::interpreter::types::{BindingKind, Completion, Environment};
use crate::interpreter::{Atom, EnvRef, Interpreter};
use crate::types::{JsPropertyKey, JsValue};

fn decode_i16(chunk: &Chunk, pc: usize) -> i16 {
    let lo = chunk.code[pc] as u16;
    let hi = chunk.code[pc + 1] as u16;
    ((hi << 8) | lo) as i16
}

/// Decodes the mode byte shared by `UpdateName` and `UpdateLocal`.
fn update_mode(mode: u8) -> (UpdateOp, bool) {
    match mode {
        0 => (UpdateOp::Increment, false),
        1 => (UpdateOp::Increment, true),
        2 => (UpdateOp::Decrement, false),
        3 => (UpdateOp::Decrement, true),
        _ => panic!("invalid update mode"),
    }
}

/// `LoadName` and the miss path of `LoadLocal`: the frame's own binding when
/// the current scope is a plain declarative one, else full identifier
/// resolution.
fn load_name_value(interp: &mut Interpreter, name: &Atom, env: &EnvRef) -> Completion {
    let (local_value, strict) = {
        let scope = env.borrow();
        let value = if scope.with_object.is_none() && scope.indirect_bindings.is_none() {
            scope
                .bindings
                .get(name)
                .and_then(|binding| binding.initialized.then(|| binding.value.clone()))
        } else {
            None
        };
        (value, scope.strict)
    };
    match local_value {
        Some(value) => Completion::Normal(value),
        None => interp.resolve_identifier(name, env, strict),
    }
}

/// Where a `break`/`continue` completion leaving an `ExecAst` statement
/// resumes, if its label names a loop compiled into this chunk.
fn escape_target(
    chunk: &Chunk,
    table: u16,
    is_continue: bool,
    label: Option<&str>,
) -> Option<usize> {
    if table == super::chunk::NO_ESCAPE_TABLE {
        return None;
    }
    chunk.escape_tables[table as usize]
        .iter()
        .find(|e| e.is_continue == is_continue && e.label.as_deref() == label)
        .map(|e| e.target as usize)
}

fn decode_u16(chunk: &Chunk, pc: usize) -> u16 {
    let lo = chunk.code[pc] as u16;
    let hi = chunk.code[pc + 1] as u16;
    (hi << 8) | lo
}

fn decode_u32(chunk: &Chunk, pc: usize) -> u32 {
    u32::from_le_bytes(chunk.code[pc..pc + 4].try_into().unwrap())
}

/// Roots every object-valued `JsValue` currently on the operand stack, not
/// just the current opcode's own operands. A value pushed by an earlier `GetProp`/
/// `GetElement` (e.g. the base of `a.b.value = a.c`) can still be pending on
/// the stack while a *later* opcode's own getter/proxy-trap/`ToPropertyKey`
/// coercion runs and reaches a nested `gc_safepoint()` — rooting only this
/// opcode's own operands would miss that older, still-pending value.
///
/// `GetElement`'s numeric-index fast path deliberately skips this temporary
/// frame: it only borrows existing typed-array/array storage and cannot run
/// user code or reach a nested safepoint. Its operands remain independently
/// rooted in `gc_bytecode_roots` until `pop_value` consumes them.
fn root_operand_stack(interp: &mut Interpreter, stack: &[JsValue]) -> usize {
    let frame = interp.gc_root_frame();
    for v in stack {
        interp.gc_root_value(v);
    }
    frame
}

fn root_stack_value(interp: &mut Interpreter, value: &JsValue) {
    if let Some(object_id) = value.as_object_id() {
        interp.gc_bytecode_roots.push(object_id);
    }
}

fn unroot_stack_value(interp: &mut Interpreter, value: &JsValue) {
    if let Some(object_id) = value.as_object_id() {
        interp.gc_bytecode_roots.pop_expected(object_id);
    }
}

fn push_value(interp: &mut Interpreter, stack: &mut Vec<JsValue>, value: JsValue) {
    root_stack_value(interp, &value);
    stack.push(value);
}

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
    let obj_val = if base.is_object() {
        base.clone()
    } else {
        match interp.to_object(base) {
            Completion::Normal(v) => v,
            abrupt => return abrupt,
        }
    };
    let Some(object_id) = obj_val.as_object_id() else {
        return Completion::Normal(JsValue::UNDEFINED);
    };
    interp.get_object_property(object_id, name, &obj_val)
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
    let obj_val = if base.is_object() {
        base.clone()
    } else {
        match interp.to_object(base) {
            Completion::Normal(v) => v,
            abrupt => return abrupt,
        }
    };
    let Some(object_id) = obj_val.as_object_id() else {
        return Completion::Normal(JsValue::UNDEFINED);
    };
    interp.get_object_property(object_id, &key, &obj_val)
}

fn member_set(
    interp: &mut Interpreter,
    base: JsValue,
    name: &str,
    rhs: JsValue,
    strict: bool,
) -> Result<(), JsValue> {
    if base.is_nullish() {
        return Err(interp.create_type_error(&format!(
            "Cannot set properties of {base} (setting '{name}')"
        )));
    }
    interp.set_object_with_key(base, name, rhs, strict)
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
    interp.set_object_with_key(base, &key, rhs, strict)
}

/// `++`/`--` on a member reference: the base and key are evaluated once by
/// the caller; GetValue → ToNumeric → ±1 → PutValue, with ToPropertyKey run
/// once (sec-postfix-increment-operator-runtime-semantics-evaluation,
/// sec-prefix-increment-operator-runtime-semantics-evaluation). The postfix
/// result is the numeric old value. `key_val` is `None` for `base.name`.
fn member_update(
    interp: &mut Interpreter,
    base: &JsValue,
    name: Option<&str>,
    key_val: Option<&JsValue>,
    op: UpdateOp,
    prefix: bool,
    strict: bool,
) -> Completion {
    let key = match (name, key_val) {
        (Some(name), _) => {
            if base.is_nullish() {
                return Completion::Throw(interp.create_type_error(&format!(
                    "Cannot read properties of {base} (reading '{name}')"
                )));
            }
            JsPropertyKey::from(name)
        }
        (None, Some(key_val)) => {
            if base.is_nullish() {
                return Completion::Throw(interp.create_type_error(&format!(
                    "Cannot read properties of {base} (reading property)"
                )));
            }
            match interp.to_property_key(key_val) {
                Ok(k) => k,
                Err(e) => return Completion::Throw(e),
            }
        }
        (None, None) => unreachable!("member update needs a name or a key value"),
    };
    let wrapper = if base.is_object() {
        base.clone()
    } else {
        match interp.to_object(base) {
            Completion::Normal(v) => v,
            abrupt => return abrupt,
        }
    };
    let current = match wrapper.as_object_id() {
        Some(id) => match interp.get_object_property(id, &key, base) {
            Completion::Normal(v) => v,
            abrupt => return abrupt,
        },
        None => JsValue::UNDEFINED,
    };
    let (old_val, new_val) = match interp.apply_update_numeric(&current, op) {
        Ok(pair) => pair,
        Err(e) => return Completion::Throw(e),
    };
    match interp.set_object_with_key(base.clone(), &key, new_val.clone(), strict) {
        Ok(()) => Completion::Normal(if prefix { new_val } else { old_val }),
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
    result
}

/// Hits a GC safepoint, asserting the VM's two scratch stacks are empty —
/// both safepoint sites (a loop back-edge, a statement boundary) land
/// between statements, where the compiler guarantees a net-zero stack.
fn safepoint_with_empty_stacks(
    interp: &mut Interpreter,
    stack: &[JsValue],
    refs: &[IdentifierRef],
    context: &str,
) {
    debug_assert!(stack.is_empty(), "operand stack live at {context}");
    debug_assert!(refs.is_empty(), "reference stack live at {context}");
    interp.gc_safepoint();
}

fn run_chunk_inner(
    interp: &mut Interpreter,
    chunk: &Chunk,
    env: &EnvRef,
    _this: JsValue,
    declare_chunk_vars: bool,
    stack: &mut Vec<JsValue>,
    refs: &mut Vec<IdentifierRef>,
) -> Completion {
    interp.bytecode_chunks_executed += 1;
    if declare_chunk_vars {
        declare_var_prologue(chunk, env);
    }
    let mut completion_value: Option<JsValue> = None;
    let mut pc: usize = 0;
    loop {
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
                match load_name_value(interp, name, env) {
                    Completion::Normal(v) => push_value(interp, stack, v),
                    abrupt => return abrupt,
                }
            }
            Op::LoadLocal => {
                let idx = decode_u16(chunk, pc);
                let slot = decode_u16(chunk, pc + 2);
                pc += 4;
                let name = &chunk.names[idx as usize];
                let hit = {
                    let scope = env.borrow();
                    match scope.bindings.entry_at(slot as usize) {
                        Some((atom, binding))
                            if atom.ptr_eq(name)
                                && binding.initialized
                                && scope.with_object.is_none()
                                && scope.indirect_bindings.is_none() =>
                        {
                            Some(binding.value.clone())
                        }
                        _ => None,
                    }
                };
                match hit {
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
                        match load_name_value(interp, name, env) {
                            Completion::Normal(v) => push_value(interp, stack, v),
                            abrupt => return abrupt,
                        }
                    }
                }
            }
            Op::StoreLocal => {
                let idx = decode_u16(chunk, pc);
                let slot = decode_u16(chunk, pc + 2);
                pc += 4;
                let name = &chunk.names[idx as usize];
                let value = stack.last().expect("stack underflow on StoreLocal");
                let stored = {
                    let mut scope = env.borrow_mut();
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
                } else {
                    #[cfg(feature = "perf-counters")]
                    {
                        interp.perf.local_slot_misses += 1;
                    }
                    let id_ref = match interp.resolve_identifier_ref(name, env) {
                        Ok(id_ref) => id_ref,
                        Err(e) => return Completion::Throw(e),
                    };
                    if let Completion::Throw(e) =
                        interp.put_value_by_ref(name, value.clone(), &id_ref, env)
                    {
                        return Completion::Throw(e);
                    }
                }
            }
            Op::UpdateLocal => {
                let idx = decode_u16(chunk, pc);
                let slot = decode_u16(chunk, pc + 2);
                let mode = chunk.code[pc + 4];
                pc += 5;
                let (op, prefix) = update_mode(mode);
                let name = &chunk.names[idx as usize];
                let fast = {
                    let mut scope = env.borrow_mut();
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
                        push_value(interp, stack, result);
                    }
                    None => {
                        #[cfg(feature = "perf-counters")]
                        {
                            interp.perf.local_slot_misses += 1;
                        }
                        match interp.eval_identifier_update(op, prefix, name, env) {
                            Completion::Normal(value) => push_value(interp, stack, value),
                            abrupt => return abrupt,
                        }
                    }
                }
            }
            Op::LoadThis => match interp.resolve_this_binding(env) {
                Completion::Normal(v) => push_value(interp, stack, v),
                abrupt => return abrupt,
            },
            Op::LoadCalleeName => {
                let idx = decode_u16(chunk, pc);
                pc += 2;
                let name = &chunk.names[idx as usize];
                let strict = env.borrow().strict;
                interp.last_identifier_with_base = None;
                let callee_result = interp.resolve_identifier(name, env, strict);
                let this_value = match interp.last_identifier_with_base.take() {
                    Some(id) => JsValue::object(id),
                    None => JsValue::UNDEFINED,
                };
                let callee = match callee_result {
                    Completion::Normal(value) => value,
                    abrupt => return abrupt,
                };
                push_value(interp, stack, callee);
                push_value(interp, stack, this_value);
            }
            Op::ResolveName => {
                let idx = decode_u16(chunk, pc);
                pc += 2;
                let name = &chunk.names[idx as usize];
                let local_ref = {
                    let scope = env.borrow();
                    scope.with_object.is_none()
                        && scope.indirect_bindings.is_none()
                        && scope.bindings.contains_key(name)
                };
                if local_ref {
                    refs.push(IdentifierRef::SpecificEnv(env.clone()));
                } else {
                    match interp.resolve_identifier_ref(name, env) {
                        Ok(id_ref) => refs.push(id_ref),
                        Err(e) => return Completion::Throw(e),
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
                match interp.get_identifier_value_by_ref(name, id_ref, env) {
                    Completion::Normal(value) => push_value(interp, stack, value),
                    abrupt => return abrupt,
                }
            }
            Op::StoreResolvedName => {
                let idx = decode_u16(chunk, pc);
                pc += 2;
                let name = &chunk.names[idx as usize];
                let id_ref = refs
                    .pop()
                    .expect("reference stack underflow on StoreResolvedName");
                let value = stack.last().expect("stack underflow on StoreResolvedName");
                let local_store = if let IdentifierRef::SpecificEnv(local_env) = &id_ref {
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
                        interp.put_value_by_ref(name, value.clone(), &id_ref, env)
                {
                    return Completion::Throw(e);
                }
            }
            Op::UpdateName => {
                let idx = decode_u16(chunk, pc);
                let mode = chunk.code[pc + 2];
                pc += 3;
                let (op, prefix) = update_mode(mode);
                let name = &chunk.names[idx as usize];
                match interp.eval_identifier_update(op, prefix, name, env) {
                    Completion::Normal(value) => push_value(interp, stack, value),
                    abrupt => return abrupt,
                }
            }
            Op::GetProp => {
                let idx = decode_u16(chunk, pc);
                pc += 2;
                let name = chunk.names[idx as usize].clone();
                let gc_frame = root_operand_stack(interp, stack);
                let base = stack.pop().expect("stack underflow on GetProp");
                let result = member_get(interp, &base, &name);
                unroot_stack_value(interp, &base);
                interp.gc_unroot_frame(gc_frame);
                match result {
                    Completion::Normal(v) => push_value(interp, stack, v),
                    abrupt => return abrupt,
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
                    let gc_frame = root_operand_stack(interp, stack);
                    let key_val = stack.pop().expect("stack underflow on GetElement key");
                    let base = stack.pop().expect("stack underflow on GetElement base");
                    let result = member_get_computed(interp, &base, &key_val);
                    unroot_stack_value(interp, &key_val);
                    unroot_stack_value(interp, &base);
                    interp.gc_unroot_frame(gc_frame);
                    match result {
                        Completion::Normal(v) => push_value(interp, stack, v),
                        abrupt => return abrupt,
                    }
                }
            }
            Op::LoadMethod => {
                let idx = decode_u16(chunk, pc);
                pc += 2;
                let name = &chunk.names[idx as usize];
                let gc_frame = root_operand_stack(interp, stack);
                let base = stack.pop().expect("stack underflow on LoadMethod");
                let result = member_get(interp, &base, name);
                unroot_stack_value(interp, &base);
                interp.gc_unroot_frame(gc_frame);
                match result {
                    Completion::Normal(method) => {
                        push_value(interp, stack, method);
                        push_value(interp, stack, base);
                    }
                    abrupt => return abrupt,
                }
            }
            Op::LoadMethodElement => {
                let gc_frame = root_operand_stack(interp, stack);
                let key_val = stack
                    .pop()
                    .expect("stack underflow on LoadMethodElement key");
                let base = stack
                    .pop()
                    .expect("stack underflow on LoadMethodElement base");
                let result = member_get_computed(interp, &base, &key_val);
                unroot_stack_value(interp, &key_val);
                unroot_stack_value(interp, &base);
                interp.gc_unroot_frame(gc_frame);
                match result {
                    Completion::Normal(method) => {
                        push_value(interp, stack, method);
                        push_value(interp, stack, base);
                    }
                    abrupt => return abrupt,
                }
            }
            Op::SetProp => {
                let idx = decode_u16(chunk, pc);
                pc += 2;
                let name = chunk.names[idx as usize].clone();
                let gc_frame = root_operand_stack(interp, stack);
                let rhs = stack.pop().expect("stack underflow on SetProp rhs");
                let base = stack.pop().expect("stack underflow on SetProp base");
                let base_root = base.clone();
                let strict = env.borrow().strict;
                let result = member_set(interp, base, &name, rhs.clone(), strict);
                unroot_stack_value(interp, &rhs);
                unroot_stack_value(interp, &base_root);
                interp.gc_unroot_frame(gc_frame);
                match result {
                    Ok(()) => push_value(interp, stack, rhs),
                    Err(e) => return Completion::Throw(e),
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
                let gc_frame = root_operand_stack(interp, stack);
                let rhs = stack.pop().expect("stack underflow on SetElement rhs");
                let key_val = stack.pop().expect("stack underflow on SetElement key");
                let base = stack.pop().expect("stack underflow on SetElement base");
                let base_root = base.clone();
                let strict = env.borrow().strict;
                let result = member_set_computed(interp, base, &key_val, rhs.clone(), strict);
                unroot_stack_value(interp, &rhs);
                unroot_stack_value(interp, &key_val);
                unroot_stack_value(interp, &base_root);
                interp.gc_unroot_frame(gc_frame);
                match result {
                    Ok(()) => push_value(interp, stack, rhs),
                    Err(e) => return Completion::Throw(e),
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
                if op == Op::ReturnCall && env.borrow().strict {
                    let (callee, this_value, args) = take_call_operands(stack, argc);
                    release_call_operands(interp, &callee, &this_value, &args);
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
                    abrupt => return abrupt,
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
                let result = interp.construct_from_evaluated(&callee, &stack[base + 1..], env);
                pop_operands(interp, stack, argc + 1);
                match result {
                    Completion::Normal(value) | Completion::Return(value) => {
                        push_value(interp, stack, value);
                    }
                    Completion::Empty => {
                        push_value(interp, stack, JsValue::UNDEFINED);
                    }
                    abrupt => return abrupt,
                }
            }
            Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Mod
            | Op::Pow
            | Op::Eq
            | Op::NotEq
            | Op::StrictEq
            | Op::StrictNotEq
            | Op::Lt
            | Op::Gt
            | Op::LtEq
            | Op::GtEq
            | Op::BitAnd
            | Op::BitOr
            | Op::BitXor
            | Op::Shl
            | Op::Shr
            | Op::UShr => {
                let r = stack.pop().expect("stack underflow on binop rhs");
                let l = stack.pop().expect("stack underflow on binop lhs");
                if matches!(op, Op::Add) && is_primitive_string_concat(&l, &r) {
                    // Primitives are never rooted, so there is nothing to unroot.
                    match interp.concat_primitives(&l, &r) {
                        Completion::Normal(v) => push_value(interp, stack, v),
                        abrupt => return abrupt,
                    }
                    continue;
                }
                let bop = match op {
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
                    _ => unreachable!(),
                };
                let result = interp.eval_binary(bop, &l, &r);
                unroot_stack_value(interp, &r);
                unroot_stack_value(interp, &l);
                match result {
                    Completion::Normal(v) => push_value(interp, stack, v),
                    abrupt => return abrupt,
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
                    abrupt => return abrupt,
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
                match interp.eval_expr(expr, env) {
                    Completion::Normal(v) => push_value(interp, stack, v),
                    abrupt => return abrupt,
                }
            }
            Op::ExecAst => {
                let idx = decode_u16(chunk, pc);
                let table = decode_u16(chunk, pc + 2);
                pc += 4;
                let super::chunk::AstNode::Stmt(stmt) = &chunk.ast[idx as usize] else {
                    unreachable!("ExecAst operand names a statement node");
                };
                #[cfg(feature = "perf-counters")]
                {
                    interp.perf.ast_fallback_execs += 1;
                }
                match interp.exec_statement(stmt, env) {
                    Completion::Normal(_) | Completion::Empty => {}
                    Completion::Break(label, value) => {
                        match escape_target(chunk, table, false, label.as_deref()) {
                            Some(target) => pc = target,
                            None => return Completion::Break(label, value),
                        }
                    }
                    Completion::Continue(label, value) => {
                        match escape_target(chunk, table, true, label.as_deref()) {
                            Some(target) => pc = target,
                            None => return Completion::Continue(label, value),
                        }
                    }
                    abrupt => return abrupt,
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
                    abrupt => return abrupt,
                }
            }
            Op::Throw => {
                let value = pop_value(interp, stack, "stack underflow on Throw");
                return Completion::Throw(value);
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
                match interp.eval_typeof_identifier(name, env) {
                    Completion::Normal(v) => push_value(interp, stack, v),
                    abrupt => return abrupt,
                }
            }
            Op::UpdateProp => {
                let idx = decode_u16(chunk, pc);
                let mode = chunk.code[pc + 2];
                pc += 3;
                let (update, prefix) = update_mode(mode);
                let name = &chunk.names[idx as usize];
                let base = stack.pop().expect("stack underflow on UpdateProp");
                let strict = env.borrow().strict;
                let result = member_update(interp, &base, Some(name), None, update, prefix, strict);
                unroot_stack_value(interp, &base);
                match result {
                    Completion::Normal(v) => push_value(interp, stack, v),
                    abrupt => return abrupt,
                }
            }
            Op::UpdateElement => {
                let mode = chunk.code[pc];
                pc += 1;
                let (update, prefix) = update_mode(mode);
                let key = stack.pop().expect("stack underflow on UpdateElement key");
                let base = stack.pop().expect("stack underflow on UpdateElement base");
                let strict = env.borrow().strict;
                let result = member_update(interp, &base, None, Some(&key), update, prefix, strict);
                unroot_stack_value(interp, &key);
                unroot_stack_value(interp, &base);
                match result {
                    Completion::Normal(v) => push_value(interp, stack, v),
                    abrupt => return abrupt,
                }
            }
            Op::Pop => {
                pop_value(interp, stack, "stack underflow on Pop");
            }
            Op::SetCompletion => {
                let value = pop_value(interp, stack, "stack underflow on SetCompletion");
                if let Some(previous) = completion_value.replace(value.clone()) {
                    unroot_stack_value(interp, &previous);
                }
                root_stack_value(interp, &value);
            }
            Op::Jump => {
                let offset = decode_i16(chunk, pc) as i32;
                if offset < 0 {
                    safepoint_with_empty_stacks(interp, stack, refs, "loop backedge");
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
                    return Completion::Throw(
                        interp.create_type_error(&format!("Cannot read properties of {base}")),
                    );
                }
                let gc_frame = root_operand_stack(interp, stack);
                let key = stack.pop().expect("stack underflow on ToPrimitiveKey key");
                let result = interp.to_primitive(&key, "string");
                unroot_stack_value(interp, &key);
                interp.gc_unroot_frame(gc_frame);
                match result {
                    Ok(v) => push_value(interp, stack, v),
                    Err(e) => return Completion::Throw(e),
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
            Op::Safepoint => {
                safepoint_with_empty_stacks(interp, stack, refs, "statement boundary");
            }
        }
    }
}
