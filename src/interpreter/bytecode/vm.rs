use super::chunk::Chunk;
use super::op::Op;
use crate::ast::BinaryOp;
use crate::interpreter::types::Completion;
use crate::interpreter::{EnvRef, Interpreter};
use crate::types::JsValue;

pub(crate) fn run_chunk(
    interp: &mut Interpreter,
    chunk: &Chunk,
    _env: &EnvRef,
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
            Op::LoadUndefined => {
                stack.push(JsValue::Undefined);
            }
            Op::Return => {
                let v = stack.pop().unwrap_or(JsValue::Undefined);
                return Completion::Return(v);
            }
            Op::ReturnUndefined => {
                return Completion::Return(JsValue::Undefined);
            }
            Op::Add => {
                let r = stack.pop().expect("stack underflow on Add rhs");
                let l = stack.pop().expect("stack underflow on Add lhs");
                match interp.eval_binary(BinaryOp::Add, &l, &r) {
                    Completion::Normal(v) => stack.push(v),
                    abrupt => return abrupt,
                }
            }
        }
    }
}
