use super::chunk::Chunk;
use super::op::Op;
use crate::ast::{BinaryOp, CallSiteId, UnaryOp, UpdateOp};
use crate::interpreter::eval::IdentifierRef;
use crate::interpreter::ic::CallIcSlot;
use crate::interpreter::types::{BindingKind, Completion, Environment};
use crate::interpreter::{EnvRef, Interpreter};
use crate::types::JsValue;

fn decode_i16(chunk: &Chunk, pc: usize) -> i16 {
    let lo = chunk.code[pc] as u16;
    let hi = chunk.code[pc + 1] as u16;
    ((hi << 8) | lo) as i16
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
    if let Some(object_id) = value.as_object_id()
        && let Some(pos) = interp
            .gc_bytecode_roots
            .iter()
            .rposition(|&id| id == object_id)
    {
        interp.gc_bytecode_roots.remove(pos);
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

fn run_chunk_with_var_prologue(
    interp: &mut Interpreter,
    chunk: &Chunk,
    env: &EnvRef,
    this_value: JsValue,
    declare_chunk_vars: bool,
) -> Completion {
    let gc_frame = interp.gc_bytecode_roots.len();
    let result = run_chunk_inner(interp, chunk, env, this_value, declare_chunk_vars);
    interp.gc_bytecode_roots.truncate(gc_frame);
    result
}

fn run_chunk_inner(
    interp: &mut Interpreter,
    chunk: &Chunk,
    env: &EnvRef,
    _this: JsValue,
    declare_chunk_vars: bool,
) -> Completion {
    interp.bytecode_chunks_executed += 1;
    if declare_chunk_vars {
        let var_scope = Environment::find_var_scope(env);
        for &name_idx in &chunk.var_names {
            let name = &chunk.names[name_idx as usize];
            if !var_scope.borrow().bindings.contains_key(name.as_ref()) {
                var_scope.borrow_mut().declare(name, BindingKind::Var);
            }
        }
    }
    let mut stack: Vec<JsValue> = Vec::with_capacity(chunk.max_stack as usize);
    let mut refs: Vec<IdentifierRef> = Vec::with_capacity(chunk.max_refs as usize);
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
                push_value(interp, &mut stack, v);
            }
            Op::LoadName => {
                let idx = decode_u16(chunk, pc);
                pc += 2;
                let name = chunk.names[idx as usize].clone();
                let strict = env.borrow().strict;
                match interp.resolve_identifier(&name, env, strict) {
                    Completion::Normal(v) => push_value(interp, &mut stack, v),
                    abrupt => return abrupt,
                }
            }
            Op::LoadThis => match interp.resolve_this_binding(env) {
                Completion::Normal(v) => push_value(interp, &mut stack, v),
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
                push_value(interp, &mut stack, callee);
                push_value(interp, &mut stack, this_value);
            }
            Op::ResolveName => {
                let idx = decode_u16(chunk, pc);
                pc += 2;
                let name = &chunk.names[idx as usize];
                match interp.resolve_identifier_ref(name, env) {
                    Ok(id_ref) => refs.push(id_ref),
                    Err(e) => return Completion::Throw(e),
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
                    Completion::Normal(value) => push_value(interp, &mut stack, value),
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
                let value = stack
                    .last()
                    .expect("stack underflow on StoreResolvedName")
                    .clone();
                if let Completion::Throw(e) = interp.put_value_by_ref(name, value, &id_ref, env) {
                    return Completion::Throw(e);
                }
            }
            Op::UpdateName => {
                let idx = decode_u16(chunk, pc);
                let mode = chunk.code[pc + 2];
                pc += 3;
                let (op, prefix) = match mode {
                    0 => (UpdateOp::Increment, false),
                    1 => (UpdateOp::Increment, true),
                    2 => (UpdateOp::Decrement, false),
                    3 => (UpdateOp::Decrement, true),
                    _ => panic!("invalid UpdateName mode"),
                };
                let name = &chunk.names[idx as usize];
                match interp.eval_identifier_update(op, prefix, name, env) {
                    Completion::Normal(value) => push_value(interp, &mut stack, value),
                    abrupt => return abrupt,
                }
            }
            Op::GetProp => {
                let idx = decode_u16(chunk, pc);
                pc += 2;
                let name = chunk.names[idx as usize].clone();
                let gc_frame = root_operand_stack(interp, &stack);
                let base = stack.pop().expect("stack underflow on GetProp");
                let result = member_get(interp, &base, &name);
                unroot_stack_value(interp, &base);
                interp.gc_unroot_frame(gc_frame);
                match result {
                    Completion::Normal(v) => push_value(interp, &mut stack, v),
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
                    pop_value(interp, &mut stack, "stack underflow on GetElement key");
                    pop_value(interp, &mut stack, "stack underflow on GetElement base");
                    push_value(interp, &mut stack, value);
                } else {
                    let gc_frame = root_operand_stack(interp, &stack);
                    let key_val = stack.pop().expect("stack underflow on GetElement key");
                    let base = stack.pop().expect("stack underflow on GetElement base");
                    let result = member_get_computed(interp, &base, &key_val);
                    unroot_stack_value(interp, &key_val);
                    unroot_stack_value(interp, &base);
                    interp.gc_unroot_frame(gc_frame);
                    match result {
                        Completion::Normal(v) => push_value(interp, &mut stack, v),
                        abrupt => return abrupt,
                    }
                }
            }
            Op::SetProp => {
                let idx = decode_u16(chunk, pc);
                pc += 2;
                let name = chunk.names[idx as usize].clone();
                let gc_frame = root_operand_stack(interp, &stack);
                let rhs = stack.pop().expect("stack underflow on SetProp rhs");
                let base = stack.pop().expect("stack underflow on SetProp base");
                let base_root = base.clone();
                let strict = env.borrow().strict;
                let result = member_set(interp, base, &name, rhs.clone(), strict);
                unroot_stack_value(interp, &rhs);
                unroot_stack_value(interp, &base_root);
                interp.gc_unroot_frame(gc_frame);
                match result {
                    Ok(()) => push_value(interp, &mut stack, rhs),
                    Err(e) => return Completion::Throw(e),
                }
            }
            Op::SetElement => {
                let gc_frame = root_operand_stack(interp, &stack);
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
                    Ok(()) => push_value(interp, &mut stack, rhs),
                    Err(e) => return Completion::Throw(e),
                }
            }
            Op::LoadUndefined => {
                push_value(interp, &mut stack, JsValue::UNDEFINED);
            }
            Op::LoadTrue => {
                push_value(interp, &mut stack, JsValue::TRUE);
            }
            Op::LoadFalse => {
                push_value(interp, &mut stack, JsValue::FALSE);
            }
            Op::LoadNull => {
                push_value(interp, &mut stack, JsValue::NULL);
            }
            Op::Return => {
                let v = if stack.is_empty() {
                    JsValue::UNDEFINED
                } else {
                    pop_value(interp, &mut stack, "stack underflow on Return")
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
                let (callee, this_value, args) = take_call_operands(&mut stack, argc);
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
                    release_call_operands(interp, &callee, &this_value, &args);
                    return Completion::TailCall {
                        func: callee,
                        this: this_value,
                        args,
                    };
                }
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
                // The operands remain in gc_bytecode_roots for the complete
                // nested invocation even though they have been removed from
                // the operand Vec. A callee can execute arbitrary JS and hit
                // any number of safepoints before returning.
                let result = if probe_hit {
                    interp.call_function_ic_validated(&callee, &this_value, &args)
                } else {
                    interp.call_function(&callee, &this_value, &args)
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
                release_call_operands(interp, &callee, &this_value, &args);
                match result {
                    Completion::Normal(value) | Completion::Return(value) => {
                        push_value(interp, &mut stack, value);
                    }
                    Completion::Empty => {
                        push_value(interp, &mut stack, JsValue::UNDEFINED);
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
                    Completion::Normal(v) => push_value(interp, &mut stack, v),
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
                    Completion::Normal(r) => push_value(interp, &mut stack, r),
                    abrupt => return abrupt,
                }
            }
            Op::Pop => {
                pop_value(interp, &mut stack, "stack underflow on Pop");
            }
            Op::SetCompletion => {
                let value = pop_value(interp, &mut stack, "stack underflow on SetCompletion");
                if let Some(previous) = completion_value.replace(value.clone()) {
                    unroot_stack_value(interp, &previous);
                }
                root_stack_value(interp, &value);
            }
            Op::Jump => {
                let offset = decode_i16(chunk, pc) as i32;
                if offset < 0 {
                    debug_assert!(stack.is_empty(), "operand stack live at loop backedge");
                    debug_assert!(refs.is_empty(), "reference stack live at loop backedge");
                    interp.gc_safepoint();
                }
                pc = (pc as i32 + 2 + offset) as usize;
            }
            Op::JumpIfFalse => {
                let offset = decode_i16(chunk, pc) as i32;
                pc += 2;
                let v = pop_value(interp, &mut stack, "stack underflow on JumpIfFalse");
                if !interp.to_boolean_val(&v) {
                    pc = (pc as i32 + offset) as usize;
                }
            }
            Op::JumpIfTrue => {
                let offset = decode_i16(chunk, pc) as i32;
                pc += 2;
                let v = pop_value(interp, &mut stack, "stack underflow on JumpIfTrue");
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
        }
    }
}
