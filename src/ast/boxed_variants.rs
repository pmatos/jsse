//! Constructors for the `Expression` variants whose payload is boxed so that
//! an `Expression` stays 56 bytes (#1401): function, arrow, class and template
//! literals are rare next to identifiers, members and calls.

use super::{ArrowBody, ArrowFunction, Expression, Pattern, SourceText};

impl Expression {
    pub(crate) fn arrow(
        params: Vec<Pattern>,
        body: ArrowBody,
        is_async: bool,
        source_text: Option<SourceText>,
        body_is_strict: bool,
    ) -> Self {
        Expression::ArrowFunction(Box::new(ArrowFunction {
            params,
            body,
            is_async,
            source_text,
            body_is_strict,
            info: Default::default(),
        }))
    }
}
