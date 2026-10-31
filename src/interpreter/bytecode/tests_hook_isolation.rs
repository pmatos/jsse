//! #1403: loading the node prelude turns on call-position recording and name
//! inference for the thread that owns the interpreter, and only that thread.
//! The unit tests share one process, so a process-wide switch let a prelude
//! test change the op streams other tests assert on.

use super::op::Op;
use super::tests::decode_ops;
use crate::interpreter::Interpreter;
use crate::interpreter::builtins::node_host_stack::call_positions_enabled;
use crate::parser::{Parser, inferred_names};

fn ops_of_free_name_call() -> Vec<Op> {
    use super::compiler::{FrameShape, compile_function_body};

    let program = Parser::new("g = f();")
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
    decode_ops(&chunk).into_iter().map(|(op, _)| op).collect()
}

fn load_prelude_on_this_thread() {
    let mut interp = Interpreter::new();
    interp.enable_node_host();
    interp.set_node_host_argv("jsse".into(), "/bin/jsse".into(), None, Vec::new());
    interp.load_node_prelude().expect("prelude loads");
}

#[test]
fn a_prelude_on_one_thread_does_not_change_what_another_thread_compiles() {
    assert!(!call_positions_enabled() && !inferred_names::enabled());
    let before = ops_of_free_name_call();
    assert!(!before.contains(&Op::SetCallPos), "{before:?}");

    let hooked = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(crate::ENGINE_STACK_SIZE)
            .spawn_scoped(scope, || {
                load_prelude_on_this_thread();
                (
                    call_positions_enabled(),
                    inferred_names::enabled(),
                    ops_of_free_name_call(),
                )
            })
            .expect("spawn")
            .join()
            .expect("prelude thread")
    });
    assert!(
        hooked.0 && hooked.1,
        "the prelude's own thread has the hooks"
    );
    assert!(hooked.2.contains(&Op::SetCallPos), "{:?}", hooked.2);

    assert!(!call_positions_enabled() && !inferred_names::enabled());
    assert_eq!(ops_of_free_name_call(), before);
}
