//! The `SwitchTable` op (#1491).

use super::{decode_u16, pop_value};
use crate::interpreter::Interpreter;
use crate::interpreter::bytecode::chunk::Chunk;
use crate::types::JsValue;

/// `SwitchTable` (#1491): pops the discriminant and returns the code offset
/// of the clause it selects.
#[inline(always)]
pub(super) fn switch_target(
    interp: &mut Interpreter,
    chunk: &Chunk,
    stack: &mut Vec<JsValue>,
    pc: usize,
) -> usize {
    let table = usize::from(decode_u16(chunk, pc));
    let value = pop_value(interp, stack, "stack underflow on SwitchTable");
    chunk.switch_tables[table].target(&value) as usize
}
