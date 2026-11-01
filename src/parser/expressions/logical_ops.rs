//! `&&`, `||` and `??`. V8 gives a single operation the position of its
//! operator (of the right operand for `??`) and a run of the same operator,
//! which it parses as one n-ary node, the start of the run (#1524).

use super::*;

/// The position V8 gives the node a run of one operator builds.
struct Run {
    start: u32,
    first: Option<u32>,
    operators: u32,
}

impl Run {
    fn new(start: u32) -> Self {
        Run {
            start,
            first: None,
            operators: 0,
        }
    }

    fn node(&mut self, at: u32) -> u32 {
        self.operators += 1;
        let first = *self.first.get_or_insert(at);
        if self.operators > 1 {
            self.start
        } else {
            first
        }
    }
}

impl Parser<'_> {
    pub(super) fn parse_nullish_coalescing(&mut self) -> Result<Expression, ParseError> {
        let mut run = Run::new(v8_position::at(self.current_token_start));
        let mut left = self.parse_bitwise_or()?;
        match self.current {
            Token::NullishCoalescing => {
                while self.current == Token::NullishCoalescing {
                    self.advance()?;
                    let at = v8_position::at(self.current_token_start);
                    let right = self.parse_bitwise_or()?;
                    v8_position::node(run.node(at));
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
                    v8_position::node(run.node(at));
                    left = Expression::Logical(
                        LogicalOp::And,
                        ExprBox::new(left),
                        ExprBox::new(right),
                    );
                }
                let mut run = Run::new(run.start);
                while self.current == Token::LogicalOr {
                    let at = self.op_pos();
                    self.advance()?;
                    let mut and_run = Run::new(v8_position::at(self.current_token_start));
                    let mut right = self.parse_bitwise_or()?;
                    while self.current == Token::LogicalAnd {
                        let and_at = self.op_pos();
                        self.advance()?;
                        let and_right = self.parse_bitwise_or()?;
                        v8_position::node(and_run.node(and_at));
                        right = Expression::Logical(
                            LogicalOp::And,
                            ExprBox::new(right),
                            ExprBox::new(and_right),
                        );
                    }
                    v8_position::node(run.node(at));
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
