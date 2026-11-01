use super::{
    TransformContext, bind_expression_to_temp, expr_has_suspension, hoist_to_property_key,
    in_class_scope,
};
use crate::ast::{ClassElement, ExprBox, Expression, Literal, PropertyKey, ident};

/// Capture, in source order, every part of a class heritage clause and its
/// computed element keys that ClassDefinitionEvaluation (§15.7.14) evaluates
/// up to and including the last one containing a `yield`/`await`, then point
/// the rebuilt class at the captured temps. The heritage is bound to a temp;
/// each such key is bound to a temp and converted with `ToPropertyKey` at once,
/// so the suspension belongs to the enclosing generator and nothing the class
/// runs later is observable before it. Keys after the last suspension stay in
/// the class: they are evaluated when it is built, in order, after every temp.
///
/// When a key is captured, the heritage checks (IsConstructor, the `prototype`
/// read) run right after the heritage, before any key, and the rebuilt class
/// takes the resulting `protoParent` instead of reading `prototype` again.
pub(super) fn hoist_class_suspensions(
    class_name: Option<&str>,
    super_class: &mut Option<Box<Expression>>,
    elements: &mut [ClassElement],
    heritage_at: u32,
    ctx: &mut TransformContext,
) {
    let is_async = ctx.is_async;
    let last_key = elements.iter().rposition(|e| {
        e.computed_key()
            .is_some_and(|k| expr_has_suspension(k, is_async))
    });
    let heritage_suspends = super_class
        .as_deref()
        .is_some_and(|sc| expr_has_suspension(sc, is_async));
    if last_key.is_none() && !heritage_suspends {
        return;
    }
    if let Some(sc) = super_class
        && !matches!(**sc, Expression::Literal(_))
    {
        let temp_var = ctx.new_temp_var("class_heritage");
        let expr = in_class_scope(sc, class_name, is_async);
        bind_expression_to_temp(&expr, &temp_var, ctx);
        **sc = ident(temp_var);
    }
    let Some(last_key) = last_key else {
        return;
    };
    if let Some(sc) = super_class
        && !matches!(**sc, Expression::Literal(Literal::Null))
    {
        let proto_var = ctx.new_temp_var("class_proto_parent");
        let check = Expression::ClassProtoParent(ExprBox::new((**sc).clone()), heritage_at);
        bind_expression_to_temp(&check, &proto_var, ctx);
        **sc = Expression::ResolvedHeritage(
            ExprBox::new((**sc).clone()),
            ExprBox::new(ident(proto_var)),
        );
    }
    for key in elements[..=last_key]
        .iter_mut()
        .filter_map(ClassElement::key_mut)
    {
        if let PropertyKey::Computed(e) = key
            && !matches!(**e, Expression::Literal(_))
        {
            let temp_var = ctx.new_temp_var("class_key");
            let expr = in_class_scope(e, class_name, is_async);
            bind_expression_to_temp(&expr, &temp_var, ctx);
            hoist_to_property_key(&temp_var, ctx);
            **e = ident(temp_var);
        }
    }
}
