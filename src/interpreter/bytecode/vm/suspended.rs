//! Resuming async function bodies. The wrapper of `run_generator_chunk`,
//! kept separate so sync generators keep their single, inlined call site.

use super::{Chunk, GeneratorFrame, Resume, run_chunk_inner};
use crate::interpreter::types::Completion;
use crate::interpreter::{EnvRef, Interpreter};

/// Runs an async body from its start (`resume` is `None`) or from the `await`
/// it is parked at. `Completion::Yield(value)` is an `await` of `value`.
#[inline(never)]
pub(crate) fn run_async_chunk(
    interp: &mut Interpreter,
    chunk: &Chunk,
    env: &EnvRef,
    frame: &mut GeneratorFrame,
    resume: Option<Resume>,
) -> Completion {
    let gc_frame = interp.gc_bytecode_roots.len();
    let envs_frame = interp.call_stack_envs.len();
    let GeneratorFrame {
        control,
        stack,
        refs,
    } = frame;
    let result = run_chunk_inner::<true>(
        interp,
        chunk,
        env,
        false,
        stack,
        refs,
        Some((control, resume)),
    );
    if !matches!(result, Completion::Throw(_) | Completion::Exit(_)) {
        interp
            .gc_bytecode_roots
            .assert_depth(gc_frame, "an async chunk exit");
    }
    interp.gc_bytecode_roots.truncate(gc_frame);
    interp.call_stack_envs.truncate(envs_frame);
    result
}
