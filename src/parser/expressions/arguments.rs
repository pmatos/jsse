use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_arguments(&mut self) -> Result<Vec<Expression>, ParseError> {
        self.eat(&Token::LeftParen)?;
        let mut args = Vec::new();
        while self.current != Token::RightParen {
            if self.current == Token::Ellipsis {
                self.advance()?;
                let operand = v8_position::at(self.current_token_start);
                args.push(self.parse_spread_operand()?);
                v8_position::expression(operand);
            } else {
                args.push(self.parse_assignment_expression()?);
            }
            if self.current != Token::Comma {
                break;
            }
            self.advance()?;
        }
        if self.current != Token::RightParen {
            return Err(self.unterminated_arguments());
        }
        self.advance()?;
        Ok(args)
    }
}
