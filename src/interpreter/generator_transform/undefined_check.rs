//! The condition shared by the conditional-default lowerings.

use crate::ast::{BinaryOp, ExprBox, Expression, Literal};

/// `typeof <name> === "undefined"` -- the "if Initializer is present and v is
/// undefined" condition shared by every conditional-default site in this
/// file (`lower_conditional_default` and the array-assignment identifier
/// fast path in `lower_array_pattern_assignment`).
pub(super) fn undefined_check_condition(name: &str) -> Expression {
    Expression::Binary(
        BinaryOp::StrictEq,
        ExprBox::new(Expression::Typeof(ExprBox::new(Expression::Identifier(
            name.to_string(),
        )))),
        ExprBox::new(Expression::Literal(Literal::String(
            "undefined".encode_utf16().collect(),
        ))),
        0,
    )
}
