//! The identifier a statement evaluates first (#1511). V8 puts the statement's
//! position on the first bytecode of a statement, so a missing or
//! uninitialised binding read there is reported at the statement, not at the
//! identifier; any other identifier keeps its own position. The parser records
//! an identifier's offset when it reads it and clears the leading one here.

use super::*;

impl Parser<'_> {
    /// The identifier's offset plus one under `--node`, else 0.
    pub(super) fn ident_pos(start: usize) -> u32 {
        if inferred_names::enabled() {
            u32::try_from(start + 1).unwrap_or(0)
        } else {
            0
        }
    }

    /// A concise arrow body as the `return` it stands for: V8 positions it at
    /// the expression's start.
    pub(super) fn arrow_return(&mut self) -> Result<Statement, ParseError> {
        let at = self.op_pos();
        let body = self.parse_arrow_expression_body();
        Self::returned(body, at)
    }

    /// `arrow_return` for the body of an async arrow.
    pub(super) fn async_arrow_return(&mut self) -> Result<Statement, ParseError> {
        let at = self.op_pos();
        let body = self.async_arrow_expr_body();
        Self::returned(body, at)
    }

    fn returned(body: Result<Expression, ParseError>, at: u32) -> Result<Statement, ParseError> {
        let mut body = body?;
        Self::clear_lead_ident(&mut body);
        let at = match &body {
            Expression::Binary(.., op) | Expression::Assign(.., op) if *op != 0 => *op,
            other => other.tail_pos(at),
        };
        Ok(Statement::Return(Some(body), at))
    }

    /// Clears the position of the identifier `expr` evaluates first, if any.
    /// Iterative: operator chains can be arbitrarily deep.
    pub(super) fn clear_lead_ident(expr: &mut Expression) {
        if !inferred_names::enabled() {
            return;
        }
        let mut cur = expr;
        loop {
            let next = match cur {
                Expression::Identifier(_, at) => {
                    *at = 0;
                    return;
                }
                Expression::Binary(_, first, ..)
                | Expression::Logical(_, first, _)
                | Expression::Conditional(first, ..)
                | Expression::Unary(_, first)
                | Expression::Typeof(first)
                | Expression::Void(first)
                | Expression::Update(_, _, first, _)
                | Expression::Member(first, ..)
                | Expression::Call(first, ..)
                | Expression::New(first, ..)
                | Expression::TaggedTemplate(first, _)
                | Expression::OptionalChain(first, _) => &mut **first,
                Expression::Assign(AssignOp::Assign, target, value, _) => {
                    if matches!(**target, Expression::Identifier(..)) {
                        &mut **value
                    } else {
                        &mut **target
                    }
                }
                Expression::Assign(_, target, ..) => &mut **target,
                Expression::Sequence(list) => match list.first_mut() {
                    Some(first) => first,
                    None => return,
                },
                _ => return,
            };
            cur = next;
        }
    }
}
