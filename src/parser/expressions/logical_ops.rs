//! `&&`, `||` and `??`. The operator's position is the one V8 gives the node
//! (#1524).

use super::*;

impl Parser<'_> {
    pub(super) fn parse_nullish_coalescing(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_bitwise_or()?;
        match self.current {
            Token::NullishCoalescing => {
                while self.current == Token::NullishCoalescing {
                    self.advance()?;
                    let at = v8_position::at(self.current_token_start);
                    let right = self.parse_bitwise_or()?;
                    v8_position::node(at);
                    left = Expression::Logical(
                        LogicalOp::NullishCoalescing,
                        ExprBox::new(left),
                        ExprBox::new(right),
                    );
                }
                if matches!(self.current, Token::LogicalOr | Token::LogicalAnd) {
                    return Err(self.error(
                        "Logical expressions and coalesce expressions cannot be mixed without parentheses",
                    ));
                }
            }
            Token::LogicalAnd | Token::LogicalOr => {
                while self.current == Token::LogicalAnd {
                    let at = self.op_pos();
                    self.advance()?;
                    let right = self.parse_bitwise_or()?;
                    v8_position::node(at);
                    left = Expression::Logical(
                        LogicalOp::And,
                        ExprBox::new(left),
                        ExprBox::new(right),
                    );
                }
                while self.current == Token::LogicalOr {
                    let at = self.op_pos();
                    self.advance()?;
                    let mut right = self.parse_bitwise_or()?;
                    while self.current == Token::LogicalAnd {
                        let and_at = self.op_pos();
                        self.advance()?;
                        let and_right = self.parse_bitwise_or()?;
                        v8_position::node(and_at);
                        right = Expression::Logical(
                            LogicalOp::And,
                            ExprBox::new(right),
                            ExprBox::new(and_right),
                        );
                    }
                    v8_position::node(at);
                    left =
                        Expression::Logical(LogicalOp::Or, ExprBox::new(left), ExprBox::new(right));
                }
                if self.current == Token::NullishCoalescing {
                    return Err(self.error(
                        "Logical expressions and coalesce expressions cannot be mixed without parentheses",
                    ));
                }
            }
            _ => {}
        }
        Ok(left)
    }
}
