use super::*;

impl Parser<'_> {
    pub(super) fn parse_do_while_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // do
        let body = self.parse_iteration_body()?;
        self.eat(&Token::Keyword(Keyword::While))?;
        self.eat(&Token::LeftParen)?;
        let test = self.parse_expression()?;
        self.eat(&Token::RightParen)?;
        // §12.9.1 special ASI rule: semicolon after do-while's `)` is always auto-inserted
        if self.current == Token::Semicolon {
            self.advance()?;
        }
        Ok(Statement::DoWhile(DoWhileStatement { test, body }))
    }
}
