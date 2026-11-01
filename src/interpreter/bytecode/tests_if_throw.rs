use super::compiler::compile_body;
use super::vm::run_chunk;
use crate::ast::{Expression, Literal, Statement};
use crate::interpreter::Interpreter;
use crate::interpreter::types::Completion;

fn run(chunk: super::chunk::Chunk) -> Completion {
    let mut interp = Interpreter::new();
    let env = interp.realm().global_env.clone();
    run_chunk(&mut interp, &chunk, &env)
}

#[test]
fn if_with_throw_branch_compiles_and_throws() {
    use crate::ast::IfStatement;
    let body = vec![Statement::If(Box::new(IfStatement {
        test: Expression::Literal(Literal::Boolean(true)),
        consequent: Box::new(Statement::Throw(
            Expression::Literal(Literal::Number(1.0)),
            0,
        )),
        alternate: None,
        at: 0,
    }))];
    let chunk = compile_body(&body).expect("compile if with throw");
    match run(chunk) {
        Completion::Throw(value) => assert_eq!(value.as_number(), Some(1.0)),
        other => panic!("expected Throw(Number(1.0)), got {other:?}"),
    }
}

#[test]
fn if_with_throw_alternate_compiles_and_throws() {
    use crate::ast::IfStatement;
    let body = vec![Statement::If(Box::new(IfStatement {
        test: Expression::Literal(Literal::Boolean(false)),
        consequent: Box::new(Statement::Return(
            Some(Expression::Literal(Literal::Number(1.0))),
            0,
        )),
        alternate: Some(Box::new(Statement::Throw(
            Expression::Literal(Literal::Number(2.0)),
            0,
        ))),
        at: 0,
    }))];
    let chunk = compile_body(&body).expect("compile if/else with throw");
    match run(chunk) {
        Completion::Throw(value) => assert_eq!(value.as_number(), Some(2.0)),
        other => panic!("expected Throw(Number(2.0)), got {other:?}"),
    }
}
