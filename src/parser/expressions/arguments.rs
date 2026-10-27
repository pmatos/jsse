use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_arguments(&mut self) -> Result<Vec<Expression>, ParseError> {
        self.eat(&Token::LeftParen)?;
        let mut args = Vec::new();
        while self.current != Token::RightParen {
            if self.current == Token::Ellipsis {
                self.advance()?;
                let expr = self.parse_assignment_expression()?;
                args.push(Expression::Spread(ExprBox::new(expr)));
            } else {
                args.push(self.parse_assignment_expression()?);
            }
            if self.current != Token::Comma {
                break;
            }
            self.advance()?;
        }
        self.eat(&Token::RightParen)?;
        Ok(args)
    }
}
