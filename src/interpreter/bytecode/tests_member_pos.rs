//! #1419: member ops record their V8 error position in `Chunk::member_pos`
//! only once the node prelude installed the stack hooks, so a default chunk
//! keeps an empty table and the same bytes.

use super::compiler::{FrameShape, compile_function_body};
use crate::interpreter::Interpreter;
use crate::parser::Parser;

fn member_positions(source: &str) -> Vec<(u32, u32)> {
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
    chunk.member_pos
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
fn only_a_hooked_thread_records_member_positions() {
    let source = "r = o.a.b; o.c = 1; o[k]();";
    assert!(on_fresh_thread(|| member_positions(source)).is_empty());
    let hooked = on_fresh_thread(|| {
        let mut interp = Interpreter::new();
        interp.enable_node_host();
        interp.set_node_host_argv("jsse".into(), "/bin/jsse".into(), None, Vec::new());
        interp.load_node_prelude().expect("prelude loads");
        member_positions(source)
    });
    assert_eq!(hooked.len(), 4, "{hooked:?}");
    assert!(hooked.windows(2).all(|w| w[0].0 < w[1].0), "{hooked:?}");
    assert!(hooked.iter().all(|&(_, pos)| pos != 0), "{hooked:?}");
}

fn pattern_sources(source: &str) -> Vec<String> {
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
    chunk.pattern_src.iter().map(|e| e.1.to_string()).collect()
}

#[test]
fn only_a_hooked_thread_records_pattern_sources() {
    let source = "{ const { a } = o.p; for (const x of f()) ; }";
    assert!(on_fresh_thread(|| pattern_sources(source)).is_empty());
    let hooked = on_fresh_thread(|| {
        let mut interp = Interpreter::new();
        interp.enable_node_host();
        interp.set_node_host_argv("jsse".into(), "/bin/jsse".into(), None, Vec::new());
        interp.load_node_prelude().expect("prelude loads");
        pattern_sources(source)
    });
    assert_eq!(
        hooked,
        [
            "o.p",
            "f is not a function or its return value is not iterable"
        ]
    );
}
