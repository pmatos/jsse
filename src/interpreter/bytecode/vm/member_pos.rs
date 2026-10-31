//! `--node` error positions of member ops (#1419): when the base of a member
//! op is null or undefined, the frame's current position becomes the one the
//! compiler recorded, so the engine's TypeError reports where V8 does.

use crate::interpreter::Interpreter;
use crate::interpreter::bytecode::chunk::Chunk;
use crate::interpreter::bytecode::op::Op;
use crate::types::JsValue;

/// Called with `pc` at the op's opcode byte, before the op runs.
#[inline]
pub(super) fn note(interp: &mut Interpreter, chunk: &Chunk, stack: &[JsValue], pc: usize) {
    if chunk.member_pos.is_empty() && chunk.pattern_src.is_empty() {
        return;
    }
    let depth = match Op::from_u8(chunk.code[pc]) {
        Some(Op::GetProp | Op::LoadMethod) => 1,
        Some(Op::GetElement | Op::LoadMethodElement | Op::SetProp) => 2,
        Some(Op::SetElement) => 3,
        _ => return,
    };
    let Some(base) = stack.len().checked_sub(depth).map(|at| &stack[at]) else {
        return;
    };
    if base.is_nullish() {
        if let Some(pos) = chunk.member_pos_at(pc) {
            interp.node_host.call_pos = pos;
        }
        interp.node_host.pattern_src = chunk.pattern_src_at(pc).cloned();
    }
}
