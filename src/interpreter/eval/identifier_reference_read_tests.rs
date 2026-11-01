use super::IdentifierRef;
use crate::ast::{AssignOp, Expression, Literal};
use crate::interpreter::Interpreter;
use crate::interpreter::types::{BindingKind, Completion, Environment, PropertyDescriptor};
use crate::types::JsValue;

fn assert_number(completion: Completion, expected: f64) {
    match completion {
        Completion::Normal(value) => assert_eq!(value.as_number(), Some(expected)),
        other => panic!("expected {expected}, got {other:?}"),
    }
}

#[test]
fn captured_identifier_read_preserves_environment_and_live_imports() {
    let mut interp = Interpreter::new();
    let outer = Environment::new(None);
    outer.borrow_mut().declare("value", BindingKind::Let);
    outer
        .borrow_mut()
        .initialize_binding("value", JsValue::number(7.0));
    let inner = Environment::new(Some(outer.clone()));
    let reference = interp.resolve_identifier_ref("value", &inner).unwrap();
    inner.borrow_mut().declare("value", BindingKind::Let);
    inner
        .borrow_mut()
        .initialize_binding("value", JsValue::number(99.0));
    assert_number(
        interp.get_identifier_value_by_ref_hinted("value", &reference, &inner, None),
        7.0,
    );

    let imports = Environment::new(Some(inner));
    imports
        .borrow_mut()
        .create_import_binding("value", outer.clone(), "value".into());
    let reference = interp.resolve_identifier_ref("value", &imports).unwrap();
    outer
        .borrow_mut()
        .initialize_binding("value", JsValue::number(8.0));
    assert_number(
        interp.get_identifier_value_by_ref_hinted("value", &reference, &imports, None),
        8.0,
    );

    outer.borrow_mut().declare("pending", BindingKind::Let);
    imports
        .borrow_mut()
        .create_import_binding("pending", outer.clone(), "pending".into());
    for (name, base) in [("pending", outer), ("pending", imports.clone())] {
        let reference = IdentifierRef::SpecificEnv(base);
        assert!(matches!(
            interp.get_identifier_value_by_ref_hinted(name, &reference, &imports, None),
            Completion::Throw(_)
        ));
    }
    assert!(matches!(
        interp.get_identifier_value_by_ref_hinted(
            "value",
            &IdentifierRef::Unresolvable,
            &imports,
            None
        ),
        Completion::Throw(_)
    ));
}

#[test]
fn identifier_assignment_reads_import_before_shadowed_global() {
    let mut interp = Interpreter::new();
    let global = interp.realm().global_env.clone();
    let global_id = global.borrow().global_object_id.unwrap();
    interp
        .get_object_cell_expect(global_id)
        .borrow_mut()
        .insert_property(
            "imported",
            PropertyDescriptor::data(JsValue::number(0.0), true, true, true),
        );
    let source = Environment::new(None);
    source.borrow_mut().declare("value", BindingKind::Let);
    source
        .borrow_mut()
        .initialize_binding("value", JsValue::number(42.0));
    let imports = Environment::new(Some(global));
    imports.borrow_mut().strict = true;
    imports
        .borrow_mut()
        .create_import_binding("imported", source, "value".into());
    let target = Expression::Identifier("imported".into(), 0);
    let rhs = Expression::Literal(Literal::Number(1.0));

    // A truthy import must short-circuit ||= even though the global is falsy.
    assert_number(
        interp.eval_logical_assign(AssignOp::LogicalOrAssign, &target, &rhs, 0, &imports),
        42.0,
    );
    let parenthesized = Expression::Sequence(vec![target.clone()]);
    assert_number(
        interp.eval_logical_assign(AssignOp::LogicalOrAssign, &parenthesized, &rhs, 0, &imports),
        42.0,
    );
}
