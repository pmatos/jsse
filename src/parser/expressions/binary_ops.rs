//! Binary operator productions below `??`, from `|` down to `*`. Each records
//! the operator's source offset (`op_pos`) on the node, for V8's error
//! position (#1511).

use super::*;

impl Parser<'_> {
    /// The operator token's offset plus one under `--node`, else 0.
    pub(super) fn op_pos(&self) -> u32 {
        if inferred_names::enabled() {
            u32::try_from(self.current_token_start + 1).unwrap_or(0)
        } else {
            0
        }
    }

    pub(super) fn parse_bitwise_or(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_bitwise_xor()?;
        while self.current == Token::Pipe {
            let at = self.op_pos();
            self.advance()?;
            let right = self.parse_bitwise_xor()?;
            left = Expression::Binary(BinaryOp::BitOr, ExprBox::new(left), ExprBox::new(right), at);
        }
        Ok(left)
    }

    fn parse_bitwise_xor(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_bitwise_and()?;
        while self.current == Token::Caret {
            let at = self.op_pos();
            self.advance()?;
            let right = self.parse_bitwise_and()?;
            left = Expression::Binary(
                BinaryOp::BitXor,
                ExprBox::new(left),
                ExprBox::new(right),
                at,
            );
        }
        Ok(left)
    }

    fn parse_bitwise_and(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_equality()?;
        while self.current == Token::Ampersand {
            let at = self.op_pos();
            self.advance()?;
            let right = self.parse_equality()?;
            left = Expression::Binary(
                BinaryOp::BitAnd,
                ExprBox::new(left),
                ExprBox::new(right),
                at,
            );
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_relational()?;
        loop {
            let op = match &self.current {
                Token::Equal => BinaryOp::Eq,
                Token::NotEqual => BinaryOp::NotEq,
                Token::StrictEqual => BinaryOp::StrictEq,
                Token::StrictNotEqual => BinaryOp::StrictNotEq,
                _ => break,
            };
            let at = self.op_pos();
            self.advance()?;
            let right = self.parse_relational()?;
            left = Expression::Binary(op, ExprBox::new(left), ExprBox::new(right), at);
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<Expression, ParseError> {
        let mut left = if let Token::PrivateName(name) = &self.current {
            let name = name.clone();
            self.use_private_name(&name)?;
            self.advance()?;
            if matches!(self.current, Token::Keyword(Keyword::In)) {
                Expression::PrivateIdentifier(name)
            } else {
                return Err(ParseError {
                    message: format!("Unexpected token #{name}"),
                });
            }
        } else {
            self.parse_shift()?
        };
        loop {
            let op = match &self.current {
                Token::LessThan => BinaryOp::Lt,
                Token::GreaterThan => BinaryOp::Gt,
                Token::LessThanEqual => BinaryOp::LtEq,
                Token::GreaterThanEqual => BinaryOp::GtEq,
                Token::Keyword(Keyword::Instanceof) => BinaryOp::Instanceof,
                Token::Keyword(Keyword::In) if !self.no_in => BinaryOp::In,
                _ => break,
            };
            let at = self.op_pos();
            self.advance()?;
            let right = self.parse_shift()?;
            // §13.10: PrivateIdentifier in ShiftExpression — reject ArrowFunction
            if matches!(&left, Expression::PrivateIdentifier(_))
                && matches!(&right, Expression::ArrowFunction(_))
            {
                return Err(self.error("Invalid right-hand side in 'in' expression"));
            }
            left = Expression::Binary(op, ExprBox::new(left), ExprBox::new(right), at);
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match &self.current {
                Token::LeftShift => BinaryOp::LShift,
                Token::RightShift => BinaryOp::RShift,
                Token::UnsignedRightShift => BinaryOp::URShift,
                _ => break,
            };
            let at = self.op_pos();
            self.advance()?;
            let right = self.parse_additive()?;
            left = Expression::Binary(op, ExprBox::new(left), ExprBox::new(right), at);
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match &self.current {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            let at = self.op_pos();
            self.advance()?;
            let right = self.parse_multiplicative()?;
            left = Expression::Binary(op, ExprBox::new(left), ExprBox::new(right), at);
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_exponentiation()?;
        loop {
            let op = match &self.current {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Mod,
                _ => break,
            };
            let at = self.op_pos();
            self.advance()?;
            let right = self.parse_exponentiation()?;
            left = Expression::Binary(op, ExprBox::new(left), ExprBox::new(right), at);
        }
        Ok(left)
    }
}
