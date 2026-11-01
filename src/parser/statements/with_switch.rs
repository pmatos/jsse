//! `switch` and `with` statements.

use super::*;

impl Parser<'_> {
    pub(super) fn parse_switch_statement(&mut self) -> Result<Statement, ParseError> {
        let at = self.op_pos();
        self.advance()?; // switch
        self.eat(&Token::LeftParen)?;
        let mut discriminant = self.parse_expression()?;
        Self::clear_lead_ident(&mut discriminant);
        self.eat(&Token::RightParen)?;
        self.eat(&Token::LeftBrace)?;
        // The decrement below is unconditional, matching `parse_iteration_body`.
        // That is deliberate: a counter some inner construct zeroed without
        // restoring underflows here — a panic under `overflow-checks`, which
        // the fuzz targets and the test profile both enable — rather than
        // silently corrupting `break`/`continue` validation further out.
        self.in_switch += 1;
        let result = self.parse_switch_case_block();
        self.in_switch -= 1;
        let cases = result?;
        self.eat(&Token::RightBrace)?;
        Ok(Statement::Switch(SwitchStatement {
            discriminant,
            cases,
            at,
        }))
    }

    pub(super) fn parse_with_statement(&mut self) -> Result<Statement, ParseError> {
        if self.strict {
            return Err(self.error("Strict mode code may not include a with statement"));
        }
        self.advance()?; // with
        self.eat(&Token::LeftParen)?;
        let expr = self.parse_expression()?;
        self.eat(&Token::RightParen)?;
        // §14.11.1: FunctionDeclaration, GeneratorDeclaration, ClassDeclaration
        // are not allowed in Statement position of WithStatement
        if matches!(self.current, Token::Keyword(Keyword::Function)) {
            return Err(self.error("Function declaration not allowed in statement position"));
        }
        if matches!(self.current, Token::Keyword(Keyword::Class)) {
            return Err(self.error("Class declaration not allowed in statement position"));
        }
        let body = self.parse_statement()?;
        // §14.11.1: IsLabelledFunction(Statement) must be false
        if Self::is_labelled_function(&body) {
            return Err(self.error("Labelled function declaration not allowed in with statement"));
        }
        Ok(Statement::With(expr, Box::new(body)))
    }
}
