//! `yield` and `yield*` (§15.5), recording the keyword's position for `--node`.

use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_yield_expression(&mut self) -> Result<Expression, ParseError> {
        let at = self.op_pos();
        self.advance()?;
        if self.prev_line_terminator
            || matches!(
                self.current,
                Token::RightBrace
                    | Token::Semicolon
                    | Token::RightParen
                    | Token::RightBracket
                    | Token::Colon
                    | Token::Comma
                    | Token::Eof
            )
        {
            return Ok(Expression::Yield(
                None,
                YieldKind {
                    delegate: false,
                    at,
                },
            ));
        }
        let delegate = if self.current == Token::Star {
            self.advance()?;
            true
        } else {
            false
        };
        let expr = self.parse_assignment_expression()?;
        Ok(Expression::Yield(
            Some(ExprBox::new(expr)),
            YieldKind { delegate, at },
        ))
    }
}
