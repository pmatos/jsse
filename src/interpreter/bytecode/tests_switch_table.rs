//! #1491: a `switch` over number/string literal cases compiles to one
//! `SwitchTable` op and its table answers like the `===` chain it replaces.

use super::chunk::Chunk;
use super::compiler::{FrameShape, compile_function_body};
use super::op::Op;
use super::switch_table::{CaseKey, SwitchTable};
use super::tests_decode::decode_ops;
use crate::parser::Parser;
use crate::types::{JsString, JsValue};

fn compile(body: &str) -> Chunk {
    let program = Parser::new(body)
        .and_then(|mut p| p.parse_program())
        .expect("parse");
    let params = vec!["x".to_string(), "y".to_string()];
    compile_function_body(
        program.body.as_slice(),
        &FrameShape {
            arguments_slot: false,
            simple_params: &params,
            separate_body_env: false,
        },
    )
    .expect("compile")
}

fn count(chunk: &Chunk, wanted: Op) -> usize {
    decode_ops(chunk)
        .iter()
        .filter(|(op, _)| *op == wanted)
        .count()
}

#[test]
fn literal_cases_compile_to_one_table() {
    let chunk = compile(
        "switch (x) { case 1: y = 1; break; case 'a': y = 2; break; case -3: y = 3; default: y = 4; }",
    );
    assert_eq!(count(&chunk, Op::SwitchTable), 1);
    assert_eq!(chunk.switch_tables.len(), 1);
    assert_eq!(count(&chunk, Op::CaseStrictEq), 0);
}

#[test]
fn a_non_literal_selector_keeps_the_chain() {
    let chunk =
        compile("switch (x) { case 1: y = 1; break; case 2: y = 2; break; case y: y = 3; }");
    assert_eq!(count(&chunk, Op::SwitchTable), 0);
    assert_eq!(count(&chunk, Op::CaseStrictEq), 3);
    let chunk =
        compile("switch (x) { case 1: y = 1; break; case 2n: y = 2; break; case 3: y = 3; }");
    assert_eq!(
        count(&chunk, Op::SwitchTable),
        0,
        "a BigInt selector is not tabled"
    );
}

#[test]
fn two_literal_cases_keep_the_chain() {
    let chunk = compile("switch (x) { case 1: y = 1; break; case 2: y = 2; }");
    assert_eq!(count(&chunk, Op::SwitchTable), 0);
    assert_eq!(count(&chunk, Op::CaseStrictEq), 2);
}

fn number(n: f64) -> JsValue {
    JsValue::number(n)
}

fn string(s: &str) -> JsValue {
    JsValue::string(JsString::from_str(s))
}

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

#[test]
fn table_picks_the_first_equal_selector_and_the_default_otherwise() {
    let table = SwitchTable::build(
        &[
            (CaseKey::Number(1.0), 10),
            (CaseKey::String(units("a")), 20),
            (CaseKey::Number(1.0), 30),
            (CaseKey::Number(-0.0), 40),
            (CaseKey::Number(f64::NAN), 50),
            (CaseKey::String(units("a")), 60),
        ],
        99,
    );
    assert_eq!(table.target(&number(1.0)), 10);
    assert_eq!(table.target(&string("a")), 20);
    assert_eq!(table.target(&number(0.0)), 40);
    assert_eq!(table.target(&number(-0.0)), 40);
    assert_eq!(table.target(&number(f64::NAN)), 99, "NaN never matches");
    assert_eq!(table.target(&string("1")), 99);
    assert_eq!(table.target(&number(2.0)), 99);
    assert_eq!(table.target(&JsValue::UNDEFINED), 99);
    assert_eq!(table.target(&JsValue::NULL), 99);
}

#[test]
fn dense_and_sparse_tables_agree_on_integers_and_fractions() {
    let dense: Vec<(CaseKey, u32)> = (-3..40)
        .map(|n| (CaseKey::Number(f64::from(n)), n as u32 + 100))
        .collect();
    let sparse: Vec<(CaseKey, u32)> = [-2_147_483_648.0, 7.0, 4_294_967_296.0, 0.5, 1e300]
        .iter()
        .enumerate()
        .map(|(i, n)| (CaseKey::Number(*n), i as u32))
        .collect();
    let dense = SwitchTable::build(&dense, 1);
    let sparse = SwitchTable::build(&sparse, 1);
    for n in -3..40 {
        assert_eq!(dense.target(&number(f64::from(n))), n as u32 + 100);
    }
    for probe in [
        -4.0,
        40.0,
        0.5,
        1e9,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ] {
        assert_eq!(dense.target(&number(probe)), 1, "dense miss {probe}");
    }
    for (i, n) in [-2_147_483_648.0, 7.0, 4_294_967_296.0, 0.5, 1e300]
        .iter()
        .enumerate()
    {
        assert_eq!(sparse.target(&number(*n)), i as u32);
    }
    for probe in [
        0.0,
        2_147_483_648.0,
        -2_147_483_649.0,
        8.0,
        f64::INFINITY,
        f64::NAN,
    ] {
        assert_eq!(sparse.target(&number(probe)), 1, "sparse miss {probe}");
    }
}
