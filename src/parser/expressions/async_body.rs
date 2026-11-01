use super::*;

impl<'a> Parser<'a> {
    /// AsyncConciseBody (15.9) is `ExpressionBody[?In, +Await]`, and
    /// ExpressionBody (15.3) is `AssignmentExpression[?In, ~Yield, ?Await]`:
    /// the body opens its own [+Await, ~Yield] context, so the enclosing
    /// FormalParameters restrictions on AwaitExpression (15.8.1) and
    /// YieldExpression (15.5.1) do not reach into it.
    pub(in crate::parser) fn async_arrow_expr_body(&mut self) -> Result<Expression, ParseError> {
        let prev_formal = std::mem::replace(&mut self.in_formal_parameters, false);
        let prev_generator = std::mem::replace(&mut self.in_generator, false);
        let result = self.parse_arrow_expression_body();
        self.in_generator = prev_generator;
        self.in_formal_parameters = prev_formal;
        result
    }
}
