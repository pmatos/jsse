use super::*;

fn names_of(pat: &Pattern) -> Vec<String> {
    let mut out = Vec::new();
    pat.bound_names(&mut out);
    out
}

fn ident(name: &str) -> Pattern {
    Pattern::Identifier(name.to_string())
}

#[test]
fn identifier_binds_its_name() {
    assert_eq!(names_of(&ident("x")), vec!["x"]);
}

#[test]
fn member_expression_binds_nothing() {
    // `[obj.prop] = ...` — an assignment target, not a declaration.
    let pat = Pattern::MemberExpression(ExprBox::new(Expression::Identifier("obj".to_string())));
    assert_eq!(names_of(&pat), Vec::<String>::new());
}

#[test]
fn assignment_default_binds_only_the_inner_name() {
    // `a = 5` binds `a`; the default-value expression contributes no names.
    let pat = Pattern::Assign(
        Box::new(ident("a")),
        ExprBox::new(Expression::Identifier("unused".to_string())),
    );
    assert_eq!(names_of(&pat), vec!["a"]);
}

#[test]
fn rest_binds_inner_name() {
    let pat = Pattern::Rest(Box::new(ident("r")));
    assert_eq!(names_of(&pat), vec!["r"]);
}

#[test]
fn array_pattern_binds_in_source_order_skipping_holes() {
    // `[a, , ...b]`
    let pat = Pattern::Array(vec![
        Some(ArrayPatternElement::Pattern(ident("a"))),
        None,
        Some(ArrayPatternElement::Rest(ident("b"))),
    ]);
    assert_eq!(names_of(&pat), vec!["a", "b"]);
}

#[test]
fn object_pattern_binds_shorthand_and_value_and_rest_but_not_key() {
    // `{ s, k: v, ...r }` binds s, v, r — the property key `k` is not bound.
    let pat = Pattern::Object(
        vec![
            ObjectPatternProperty::Shorthand("s".to_string()),
            ObjectPatternProperty::KeyValue(PropertyKey::Identifier("k".to_string()), ident("v")),
            ObjectPatternProperty::Rest(ident("r")),
        ]
        .into(),
    );
    assert_eq!(names_of(&pat), vec!["s", "v", "r"]);
}

#[test]
fn nested_pattern_flattens_in_source_order() {
    // `[{ a, k: [b, ...c] }]`
    let inner_array = Pattern::Array(vec![
        Some(ArrayPatternElement::Pattern(ident("b"))),
        Some(ArrayPatternElement::Rest(ident("c"))),
    ]);
    let inner_object = Pattern::Object(
        vec![
            ObjectPatternProperty::Shorthand("a".to_string()),
            ObjectPatternProperty::KeyValue(PropertyKey::Identifier("k".to_string()), inner_array),
        ]
        .into(),
    );
    let pat = Pattern::Array(vec![Some(ArrayPatternElement::Pattern(inner_object))]);
    assert_eq!(names_of(&pat), vec!["a", "b", "c"]);
}

#[test]
fn appends_to_existing_vec() {
    let mut out = vec!["pre".to_string()];
    ident("x").bound_names(&mut out);
    assert_eq!(out, vec!["pre", "x"]);
}
