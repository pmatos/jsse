use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_try_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // try
        self.eat(&Token::LeftBrace)?;
        let block =
            self.with_block_scope(Self::enter_block_scope, |p| p.parse_block_statements(false))?;
        self.eat(&Token::RightBrace)?;

        let handler = if self.current == Token::Keyword(Keyword::Catch) {
            self.advance()?;
            let param = if self.current == Token::LeftParen {
                self.advance()?;
                let p = self.parse_binding_pattern()?;
                self.eat(&Token::RightParen)?;
                Some(p)
            } else {
                None
            };
            self.eat(&Token::LeftBrace)?;
            let body = self
                .with_block_scope(Self::enter_block_scope, |p| p.parse_block_statements(false))?;
            self.eat(&Token::RightBrace)?;
            self.check_catch_clause(param.as_ref(), &body)?;
            Some(CatchClause { param, body })
        } else {
            None
        };

        let finalizer = if self.current == Token::Keyword(Keyword::Finally) {
            self.advance()?;
            self.eat(&Token::LeftBrace)?;
            let body = self
                .with_block_scope(Self::enter_block_scope, |p| p.parse_block_statements(false))?;
            self.eat(&Token::RightBrace)?;
            Some(body)
        } else {
            None
        };

        if handler.is_none() && finalizer.is_none() {
            return Err(self.error("Missing catch or finally after try"));
        }
        Ok(Statement::Try(Box::new(TryStatement {
            block,
            handler,
            finalizer,
        })))
    }
}
