//! #1511: stores and updates of identifiers record the `=`/`++` position V8
//! reports for a constant assignment in `Chunk::binary_pos`, keyed by the
//! offset after the store instruction, only once the node prelude installed
//! the stack hooks.

use super::compiler::{FrameShape, compile_function_body};
use crate::interpreter::Interpreter;
use crate::parser::Parser;

fn store_positions(source: &str) -> Vec<(u32, u32)> {
    let program = Parser::new(source)
        .and_then(|mut p| p.parse_program())
        .expect("parse");
    compile_function_body(
        program.body.as_slice(),
        &FrameShape {
            arguments_slot: false,
            simple_params: &[],
            separate_body_env: false,
        },
    )
    .expect("compile")
    .binary_pos
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
fn only_a_hooked_thread_records_store_positions() {
    let source = "x = 1; x += 2; x++; ++x;";
    assert!(on_fresh_thread(|| store_positions(source)).is_empty());
    let hooked = on_fresh_thread(|| {
        let mut interp = Interpreter::new();
        interp.enable_node_host();
        interp.set_node_host_argv("jsse".into(), "/bin/jsse".into(), None, Vec::new());
        interp.load_node_prelude().expect("prelude loads");
        store_positions(source)
    });
    assert_eq!(hooked.len(), 4, "{hooked:?}");
    assert!(hooked.windows(2).all(|w| w[0].0 < w[1].0), "{hooked:?}");
    let token_offsets = [2, 9, 16, 22];
    for (&(_, pos), offset) in hooked.iter().zip(token_offsets) {
        assert_eq!(pos, offset + 1, "{hooked:?}");
    }
}
