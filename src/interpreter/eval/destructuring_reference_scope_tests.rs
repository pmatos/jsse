use super::DestructLRef;
use crate::ast::Expression;
use crate::interpreter::Interpreter;
use crate::interpreter::types::{Completion, Environment};
use crate::types::{JsString, JsValue};

#[test]
fn roots_and_finalizes_a_captured_member_reference() {
    let mut interp = Interpreter::new();
    let object_id = interp.create_object_id();
    let env = Environment::new(None);
    let baseline = interp.gc_root_frame();
    let reference = DestructLRef::Member(
        JsValue::object(object_id),
        JsValue::string(JsString::from_str("answer")),
    );

    let completion =
        interp.with_destruct_lref(Some(reference), &Expression::This, &env, |interp| {
            assert!(
                interp.gc_temp_roots.contains(&object_id),
                "captured member base must stay rooted while the body runs"
            );
            Completion::Normal(JsValue::number(42.0))
        });

    assert!(matches!(completion, Completion::Empty));
    assert_eq!(
        interp.gc_root_frame(),
        baseline,
        "reference roots must be released after finalization"
    );
    let descriptor = interp
        .get_object_cell_expect(object_id)
        .borrow()
        .get_own_property_full("answer")
        .expect("captured reference must receive the body value");
    assert_eq!(
        descriptor.value.and_then(|value| value.as_number()),
        Some(42.0)
    );
}
