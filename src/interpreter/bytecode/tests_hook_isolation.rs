//! #1403: loading the node prelude turns on call-position recording and name
//! inference for the thread that owns the interpreter, and only that thread.
//! The unit tests share one process, so a process-wide switch let a prelude
//! test change the op streams other tests assert on.

use super::op::Op;
use super::tests::decode_ops;
use crate::interpreter::Interpreter;
use crate::interpreter::builtins::node_host_stack::call_positions_enabled;
use crate::parser::{Parser, inferred_names};

fn ops_of_free_name_call() -> Vec<(Op, usize)> {
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
    decode_ops(&chunk)
        .into_iter()
        .map(|(op, operands)| (op, operands.len()))
        .collect()
}

fn load_prelude() {
    let mut interp = Interpreter::new();
    interp.enable_node_host();
    interp.set_node_host_argv("jsse".into(), "/bin/jsse".into(), None, Vec::new());
    interp.load_node_prelude().expect("prelude loads");
}

type Snapshot = (bool, bool, Vec<(Op, usize)>);

/// Runs `f` on a fresh thread with the engine's stack (the prelude overflows
/// a debug test thread's default one) and reports that thread's hook state.
fn on_fresh_thread(f: fn()) -> Snapshot {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(crate::ENGINE_STACK_SIZE)
            .spawn_scoped(scope, || {
                f();
                (
                    call_positions_enabled(),
                    inferred_names::enabled(),
                    ops_of_free_name_call(),
                )
            })
            .expect("spawn")
            .join()
            .expect("thread")
    })
}

#[test]
fn a_prelude_on_one_thread_does_not_change_what_another_thread_compiles() {
    let (positions, names, plain) = on_fresh_thread(|| {});
    assert!(!positions && !names);
    assert!(plain.contains(&(Op::Call, 2)), "{plain:?}");
    assert!(
        !plain.iter().any(|(op, _)| *op == Op::SetCallPos),
        "{plain:?}"
    );

    // The position travels inside the call op (#1492): a third operand, and
    // no separate op before it.
    let (positions, names, hooked) = on_fresh_thread(load_prelude);
    assert!(positions && names, "the prelude's own thread has the hooks");
    assert!(hooked.contains(&(Op::Call, 3)), "{hooked:?}");
    assert!(
        !hooked.iter().any(|(op, _)| *op == Op::SetCallPos),
        "{hooked:?}"
    );

    assert_eq!(on_fresh_thread(|| {}), (false, false, plain));
}
