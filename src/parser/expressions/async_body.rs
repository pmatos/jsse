use super::*;

impl<'a> Parser<'a> {
    /// AsyncConciseBody (15.9) is `ExpressionBody[?In, +Await]`: the body opens
    /// its own [+Await] context, so the enclosing FormalParameters restriction
    /// on AwaitExpression (15.8.1) does not reach into it.
    pub(super) fn async_arrow_expr_body(&mut self) -> Result<Expression, ParseError> {
        let prev_formal = std::mem::replace(&mut self.in_formal_parameters, false);
        let result = self.parse_arrow_expression_body();
        self.in_formal_parameters = prev_formal;
        result
    }
}
