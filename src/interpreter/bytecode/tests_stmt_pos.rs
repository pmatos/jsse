//! #1511: the start of an expression, `return`, `if`, `switch` or `throw`
//! statement and the offset of an identifier are recorded only once the node
//! prelude installed the stack hooks. A default chunk keeps its bytes: no
//! `SetCallPos` op and an empty position table.

use super::compiler::{FrameShape, compile_function_body};
use super::op::Op;
use super::tests::decode_ops;
use crate::ast::{Expression, Program, Statement};
use crate::interpreter::Interpreter;
use crate::parser::Parser;

const SOURCE: &str = "if (a) { b; } switch (c) { } throw d + e;";

fn parse(source: &str) -> Program {
    Parser::new(source)
        .and_then(|mut p| p.parse_program())
        .expect("parse")
}

/// The number of `SetCallPos` ops and the position table of `source`.
fn summary(source: &str) -> (usize, Vec<(u32, u32)>) {
    let chunk = compile_function_body(
        parse(source).body.as_slice(),
        &FrameShape {
            arguments_slot: false,
            simple_params: &[],
            separate_body_env: false,
        },
    )
    .expect("compile");
    let ops = decode_ops(&chunk);
    let positions = ops.iter().filter(|(op, _)| *op == Op::SetCallPos).count();
    (positions, chunk.member_pos)
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

fn hooked<T>(f: impl FnOnce() -> T) -> T {
    let mut interp = Interpreter::new();
    interp.enable_node_host();
    interp.set_node_host_argv("jsse".into(), "/bin/jsse".into(), None, Vec::new());
    interp.load_node_prelude().expect("prelude loads");
    f()
}

#[test]
fn a_default_chunk_has_no_statement_or_identifier_positions() {
    let (positions, table) = on_fresh_thread(|| summary(SOURCE));
    assert_eq!(positions, 0);
    assert!(table.is_empty(), "{table:?}");
}

#[test]
fn a_hooked_chunk_positions_each_statement_start_once() {
    let (positions, _) = on_fresh_thread(|| hooked(|| summary(SOURCE)));
    assert_eq!(
        positions, 5,
        "if, `b;`, switch, and the throw twice (before and after its operand)"
    );
}

#[test]
fn a_hooked_chunk_records_a_name_load_but_not_a_leading_one() {
    let (_, table) = on_fresh_thread(|| hooked(|| summary("throw 1 + d;")));
    assert_eq!(table.len(), 1, "{table:?}");
    assert_eq!(table[0].1, 11, "the offset of `d` plus one");
    let (_, leading) = on_fresh_thread(|| hooked(|| summary("throw d + 1;")));
    assert!(
        leading.is_empty(),
        "the statement start stands: {leading:?}"
    );
}

#[test]
fn the_parser_records_offsets_under_the_hooks_only() {
    fn first(program: &Program) -> (u32, u32) {
        let Statement::Expression(Expression::Binary(_, _, right, _), at) =
            &program.body.as_slice()[0]
        else {
            panic!("expected a binary expression statement");
        };
        let Expression::Identifier(_, ident) = &**right else {
            panic!("expected an identifier operand");
        };
        (*at, *ident)
    }
    let plain = on_fresh_thread(|| first(&parse("1 + a;")));
    assert_eq!(plain, (0, 0));
    let node = on_fresh_thread(|| hooked(|| first(&parse("1 + a;"))));
    assert_eq!(node, (1, 5));
}
