//! Bytecode shapes of accessor bodies that run without a call frame.

use super::BytecodeCacheState;
use super::chunk::{Chunk, TrivialAccessor};
use super::op::Op;
use super::tests_decode::decode_ops;
use crate::interpreter::Interpreter;
use crate::interpreter::types::JsFunction;
use std::rc::Rc;

fn accessor_chunks(source: &str) -> (Option<Rc<Chunk>>, Option<Rc<Chunk>>) {
    use crate::parser::Parser;
    let program = Parser::new(source)
        .expect("parser init")
        .parse_program()
        .expect("parse");
    let mut interp = Interpreter::new();
    let _ = interp.run(&program);
    let chunk_of = |name: &str| {
        let function = interp.get_global_var_ref(name).expect("global defined");
        let object = interp
            .get_object_cell(function.as_object_id().expect("function object"))
            .expect("live object");
        let object = object.borrow();
        match object.callable.as_deref() {
            Some(JsFunction::User {
                bytecode_cache: BytecodeCacheState::Compiled(chunk),
                ..
            }) => Some(chunk.clone()),
            _ => None,
        }
    };
    (chunk_of("getter"), chunk_of("setter"))
}

fn shapes(class_body: &str) -> (Option<Rc<Chunk>>, Option<Rc<Chunk>>) {
    accessor_chunks(&format!(
        "class P {{ {class_body} }}\n\
         var d = Object.getOwnPropertyDescriptor(P.prototype, 'x');\n\
         var getter = d.get, setter = d.set;\n\
         var o = new P(); o.x = 1; o.x;"
    ))
}

fn trivial(chunk: &Option<Rc<Chunk>>) -> Option<TrivialAccessor> {
    chunk.as_ref().and_then(|chunk| chunk.trivial_accessor)
}

#[test]
fn this_member_getter_and_setter_are_recognised() {
    let (getter, setter) = shapes("get x() { return this._x; } set x(v) { this._x = v; }");
    let Some(TrivialAccessor::Getter(read)) = trivial(&getter) else {
        panic!("getter not recognised");
    };
    let Some(TrivialAccessor::Setter(write)) = trivial(&setter) else {
        panic!("setter not recognised");
    };
    let (getter, setter) = (getter.unwrap(), setter.unwrap());
    assert_eq!(getter.names[usize::from(read)].as_str(), "_x");
    assert_eq!(setter.names[usize::from(write)].as_str(), "_x");
    assert!(decode_ops(&getter).iter().any(|(op, _)| *op == Op::GetProp));
}

#[test]
fn other_getter_bodies_are_not_recognised() {
    for body in [
        "return this._x + 1;",
        "this._x;",
        "return globalThis._x;",
        "return this[0];",
        "return this._x.y;",
        "var a = this._x; return a;",
        "return super._x;",
        "return this._x, 1;",
    ] {
        let (getter, _) = shapes(&format!("get x() {{ {body} }} set x(v) {{}}"));
        assert_eq!(trivial(&getter), None, "getter body `{body}`");
    }
}

#[test]
fn other_setter_bodies_are_not_recognised() {
    for body in [
        "this._x = v + 1;",
        "this._x = 1;",
        "globalThis._x = v;",
        "this._x = v; return 1;",
        "this._x = v; this._y = v;",
        "this[0] = v;",
        "this._x += v;",
        "this._x = globalThis.v;",
        "this._x = arguments[0];",
        "super._x = v;",
        "var w = v; this._x = w;",
    ] {
        let (_, setter) = shapes(&format!("get x() {{ return 1; }} set x(v) {{ {body} }}"));
        assert_eq!(trivial(&setter), None, "setter body `{body}`");
    }
}

#[test]
fn destructured_setter_parameter_is_not_recognised() {
    let (_, setter) = shapes("get x() { return 1; } set x({ a }) { this._x = a; }");
    assert_eq!(trivial(&setter), None);
}
