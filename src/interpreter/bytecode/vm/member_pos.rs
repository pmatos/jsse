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
        Some(Op::GetIterator | Op::ForInHead) => {
            if let Some(pos) = chunk.member_pos_at(pc) {
                interp.node_host.call_pos = pos;
            }
            return;
        }
        Some(Op::GetProp | Op::LoadMethod | Op::UpdateProp) => 1,
        Some(
            Op::GetElement
            | Op::LoadMethodElement
            | Op::SetProp
            | Op::UpdateElement
            | Op::ToPrimitiveKey,
        ) => 2,
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

/// A store or update is about to take its by-name path: a constant
/// assignment it throws is reported at the position the compiler recorded
/// for the instruction ending at `pc` (#1511).
#[inline]
pub(super) fn note_store(interp: &mut Interpreter, chunk: &Chunk, pc: usize) {
    if !chunk.binary_pos.is_empty()
        && let Some(pos) = chunk.binary_pos_at(pc)
    {
        interp.node_host.assign_pos = pos;
    }
}

/// A binary op is about to run: when the chunk recorded the operator's
/// position (#1511), the frame's position becomes it, so a throw from the op
/// or from a `valueOf` it calls reports where V8 does. `pc` is the offset
/// after the instruction.
#[inline]
pub(super) fn note_binary(interp: &mut Interpreter, chunk: &Chunk, pc: usize) {
    if !chunk.binary_pos.is_empty()
        && let Some(pos) = chunk.binary_pos_at(pc)
    {
        interp.node_host.call_pos = pos;
    }
}
