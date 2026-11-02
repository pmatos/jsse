//! #1511: binary ops record the operator's V8 error position in
//! `Chunk::binary_pos` only once the node prelude installed the stack hooks,
//! so a default chunk keeps an empty table and the same bytes.

use super::compiler::{FrameShape, compile_function_body};
use crate::interpreter::Interpreter;
use crate::parser::Parser;

fn binary_positions(source: &str) -> Vec<(u32, u32)> {
    let program = Parser::new(source)
        .and_then(|mut p| p.parse_program())
        .expect("parse");
    let chunk = compile_function_body(
        program.body.as_slice(),
        &FrameShape {
            arguments_slot: false,
            simple_params: &[],
            separate_body_env: false,
        },
    )
    .expect("compile");
    chunk.binary_pos
}

fn on_fresh_thread<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(crate::ENGINE_STACK_SIZE)
            .spawn_scoped(scope, f)
            .expect("spawn")
            .join()
            .expect("thread")
    })
}

#[test]
fn only_a_hooked_thread_records_binary_positions() {
    let source = "f(a + b); f(a in b); if (a < 1) { f(a * 2); }";
    assert!(on_fresh_thread(|| binary_positions(source)).is_empty());
    let hooked = on_fresh_thread(|| {
        let mut interp = Interpreter::new();
        interp.enable_node_host();
        interp.set_node_host_argv("jsse".into(), "/bin/jsse".into(), None, Vec::new());
        interp.load_node_prelude().expect("prelude loads");
        binary_positions(source)
    });
    assert_eq!(hooked.len(), 4, "{hooked:?}");
    assert!(hooked.windows(2).all(|w| w[0].0 < w[1].0), "{hooked:?}");
    let operator_offsets = ["+", "in b", "<", "*"].map(|op| source.find(op).unwrap() as u32);
    for (&(_, pos), offset) in hooked.iter().zip(operator_offsets) {
        assert_eq!(pos, offset + 1, "{hooked:?}");
    }
}

/// The operator offset lives in `Expression::Binary`'s padding; a position on
/// every `ExprBox` would move each boxed node to the next allocator class.
#[cfg(target_pointer_width = "64")]
#[test]
fn the_operator_offset_does_not_grow_expression() {
    assert_eq!(std::mem::size_of::<crate::ast::Expression>(), 56);
}
