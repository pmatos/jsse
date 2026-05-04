use super::chunk::Chunk;
use super::op::Op;
use crate::ast::{BinaryOp, UnaryOp};
use crate::interpreter::helpers::to_boolean;
use crate::interpreter::types::Completion;
use crate::interpreter::{EnvRef, Interpreter};
use crate::types::JsValue;

fn decode_i16(chunk: &Chunk, pc: usize) -> i16 {
    let lo = chunk.code[pc] as u16;
    let hi = chunk.code[pc + 1] as u16;
    ((hi << 8) | lo) as i16
}

pub(crate) fn run_chunk(
    interp: &mut Interpreter,
    chunk: &Chunk,
    env: &EnvRef,
    _this: JsValue,
) -> Completion {
    interp.bytecode_chunks_executed += 1;
    let mut stack: Vec<JsValue> = Vec::with_capacity(chunk.max_stack as usize);
    let mut pc: usize = 0;
    loop {
        let op_byte = chunk.code[pc];
        let op = Op::from_u8(op_byte).expect("invalid opcode");
        pc += 1;
        match op {
            Op::LoadConst => {
                let lo = chunk.code[pc] as u16;
                let hi = chunk.code[pc + 1] as u16;
                pc += 2;
                let idx = (hi << 8) | lo;
                let v = chunk.constants[idx as usize].to_value();
                stack.push(v);
            }
            Op::LoadName => {
                let lo = chunk.code[pc] as u16;
                let hi = chunk.code[pc + 1] as u16;
                pc += 2;
                let idx = (hi << 8) | lo;
                let name = chunk.names[idx as usize].clone();
                let strict = env.borrow().strict;
                match interp.resolve_identifier(&name, env, strict) {
                    Completion::Normal(v) => stack.push(v),
                    abrupt => return abrupt,
                }
            }
            Op::StoreName => {
                // Stack on entry: [..., value]
                // Stack on exit:  [..., value]   (assignment leaves value on stack)
                let lo = chunk.code[pc] as u16;
                let hi = chunk.code[pc + 1] as u16;
                pc += 2;
                let idx = (hi << 8) | lo;
                let name = chunk.names[idx as usize].clone();
                let value = stack.last().expect("stack underflow on StoreName").clone();
                let id_ref = match interp.resolve_identifier_ref(&name, env) {
                    Ok(r) => r,
                    Err(e) => return Completion::Throw(e),
                };
                if let Completion::Throw(e) = interp.put_value_by_ref(&name, value, &id_ref, env) {
                    return Completion::Throw(e);
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
                pc = (pc as i32 + 2 + offset) as usize;
            }
            Op::JumpIfFalse => {
                let offset = decode_i16(chunk, pc) as i32;
                pc += 2;
                let v = stack.pop().expect("stack underflow on JumpIfFalse");
                if !to_boolean(&v) {
                    pc = (pc as i32 + offset) as usize;
                }
            }
            Op::JumpIfTrue => {
                let offset = decode_i16(chunk, pc) as i32;
                pc += 2;
                let v = stack.pop().expect("stack underflow on JumpIfTrue");
                if to_boolean(&v) {
                    pc = (pc as i32 + offset) as usize;
                }
            }
            Op::JumpIfTruthyKeep => {
                let offset = decode_i16(chunk, pc) as i32;
                pc += 2;
                let v = stack.last().expect("stack underflow on JumpIfTruthyKeep");
                if to_boolean(v) {
                    pc = (pc as i32 + offset) as usize;
                }
            }
            Op::JumpIfFalsyKeep => {
                let offset = decode_i16(chunk, pc) as i32;
                pc += 2;
                let v = stack.last().expect("stack underflow on JumpIfFalsyKeep");
                if !to_boolean(v) {
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
