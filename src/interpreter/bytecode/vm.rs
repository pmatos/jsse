use super::chunk::Chunk;
use super::op::Op;
use crate::ast::{BinaryOp, UnaryOp, UpdateOp};
use crate::interpreter::eval::IdentifierRef;
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

/// Roots every `JsValue::Object` currently on the operand stack, not just the
/// current opcode's own operands. A value pushed by an earlier `GetProp`/
/// `GetElement` (e.g. the base of `a.b.value = a.c`) can still be pending on
/// the stack while a *later* opcode's own getter/proxy-trap/`ToPropertyKey`
/// coercion runs and reaches a nested `gc_safepoint()` — rooting only this
/// opcode's own operands would miss that older, still-pending value.
fn root_operand_stack(interp: &mut Interpreter, stack: &[JsValue]) -> usize {
    let frame = interp.gc_root_frame();
    for v in stack {
        interp.gc_root_value(v);
    }
    frame
}

fn member_get(interp: &mut Interpreter, base: &JsValue, name: &str) -> Completion {
    if matches!(base, JsValue::Undefined | JsValue::Null) {
        let err = interp.create_type_error(&format!(
            "Cannot read properties of {base} (reading '{name}')"
        ));
        return Completion::Throw(err);
    }
    let obj_val = if matches!(base, JsValue::Object(_)) {
        base.clone()
    } else {
        match interp.to_object(base) {
            Completion::Normal(v) => v,
            abrupt => return abrupt,
        }
    };
    let JsValue::Object(o) = &obj_val else {
        return Completion::Normal(JsValue::Undefined);
    };
    interp.get_object_property(o.id, name, &obj_val)
}

fn member_get_computed(interp: &mut Interpreter, base: &JsValue, key_val: &JsValue) -> Completion {
    if matches!(base, JsValue::Undefined | JsValue::Null) {
        let err = interp.create_type_error(&format!(
            "Cannot read properties of {base} (reading property)"
        ));
        return Completion::Throw(err);
    }
    let key = match interp.to_property_key(key_val) {
        Ok(k) => k,
        Err(e) => return Completion::Throw(e),
    };
    let obj_val = if matches!(base, JsValue::Object(_)) {
        base.clone()
    } else {
        match interp.to_object(base) {
            Completion::Normal(v) => v,
            abrupt => return abrupt,
        }
    };
    let JsValue::Object(o) = &obj_val else {
        return Completion::Normal(JsValue::Undefined);
    };
    interp.get_object_property(o.id, &key, &obj_val)
}

fn member_set(
    interp: &mut Interpreter,
    base: JsValue,
    name: &str,
    rhs: JsValue,
    strict: bool,
) -> Result<(), JsValue> {
    if matches!(base, JsValue::Undefined | JsValue::Null) {
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
    if matches!(base, JsValue::Undefined | JsValue::Null) {
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
    _this: JsValue,
) -> Completion {
    interp.bytecode_chunks_executed += 1;
    let var_scope = Environment::find_var_scope(env);
    for &name_idx in &chunk.var_names {
        let name = &chunk.names[name_idx as usize];
        if !var_scope.borrow().bindings.contains_key(name.as_ref()) {
            var_scope.borrow_mut().declare(name, BindingKind::Var);
        }
    }
    let mut stack: Vec<JsValue> = Vec::with_capacity(chunk.max_stack as usize);
    let mut refs: Vec<IdentifierRef> = Vec::with_capacity(chunk.max_refs as usize);
    let mut pc: usize = 0;
    loop {
        let op_byte = chunk.code[pc];
        let op = Op::from_u8(op_byte).expect("invalid opcode");
        pc += 1;
        match op {
            Op::LoadConst => {
                let idx = decode_u16(chunk, pc);
                pc += 2;
                let v = chunk.constants[idx as usize].to_value();
                stack.push(v);
            }
            Op::LoadName => {
                let idx = decode_u16(chunk, pc);
                pc += 2;
                let name = chunk.names[idx as usize].clone();
                let strict = env.borrow().strict;
                match interp.resolve_identifier(&name, env, strict) {
                    Completion::Normal(v) => stack.push(v),
                    abrupt => return abrupt,
                }
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
                    Completion::Normal(value) => stack.push(value),
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
                    Completion::Normal(value) => stack.push(value),
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
                interp.gc_unroot_frame(gc_frame);
                match result {
                    Completion::Normal(v) => stack.push(v),
                    abrupt => return abrupt,
                }
            }
            Op::GetElement => {
                let gc_frame = root_operand_stack(interp, &stack);
                let key_val = stack.pop().expect("stack underflow on GetElement key");
                let base = stack.pop().expect("stack underflow on GetElement base");
                let result = member_get_computed(interp, &base, &key_val);
                interp.gc_unroot_frame(gc_frame);
                match result {
                    Completion::Normal(v) => stack.push(v),
                    abrupt => return abrupt,
                }
            }
            Op::SetProp => {
                let idx = decode_u16(chunk, pc);
                pc += 2;
                let name = chunk.names[idx as usize].clone();
                let gc_frame = root_operand_stack(interp, &stack);
                let rhs = stack.pop().expect("stack underflow on SetProp rhs");
                let base = stack.pop().expect("stack underflow on SetProp base");
                let strict = env.borrow().strict;
                let result = member_set(interp, base, &name, rhs.clone(), strict);
                interp.gc_unroot_frame(gc_frame);
                match result {
                    Ok(()) => stack.push(rhs),
                    Err(e) => return Completion::Throw(e),
                }
            }
            Op::SetElement => {
                let gc_frame = root_operand_stack(interp, &stack);
                let rhs = stack.pop().expect("stack underflow on SetElement rhs");
                let key_val = stack.pop().expect("stack underflow on SetElement key");
                let base = stack.pop().expect("stack underflow on SetElement base");
                let strict = env.borrow().strict;
                let result = member_set_computed(interp, base, &key_val, rhs.clone(), strict);
                interp.gc_unroot_frame(gc_frame);
                match result {
                    Ok(()) => stack.push(rhs),
                    Err(e) => return Completion::Throw(e),
                }
            }
            Op::LoadUndefined => {
                stack.push(JsValue::Undefined);
            }
            Op::LoadTrue => {
                stack.push(JsValue::Boolean(true));
            }
            Op::LoadFalse => {
                stack.push(JsValue::Boolean(false));
            }
            Op::LoadNull => {
                stack.push(JsValue::Null);
            }
            Op::Return => {
                let v = stack.pop().unwrap_or(JsValue::Undefined);
                return Completion::Return(v);
            }
            Op::ReturnUndefined => {
                return Completion::Return(JsValue::Undefined);
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
                match interp.eval_binary(bop, &l, &r) {
                    Completion::Normal(v) => stack.push(v),
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
                match interp.eval_unary(uop, &v) {
                    Completion::Normal(r) => stack.push(r),
                    abrupt => return abrupt,
                }
            }
            Op::Pop => {
                stack.pop().expect("stack underflow on Pop");
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
                let v = stack.pop().expect("stack underflow on JumpIfFalse");
                if !interp.to_boolean_val(&v) {
                    pc = (pc as i32 + offset) as usize;
                }
            }
            Op::JumpIfTrue => {
                let offset = decode_i16(chunk, pc) as i32;
                pc += 2;
                let v = stack.pop().expect("stack underflow on JumpIfTrue");
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
                if !matches!(v, JsValue::Undefined | JsValue::Null) {
                    pc = (pc as i32 + offset) as usize;
                }
            }
        }
    }
}
