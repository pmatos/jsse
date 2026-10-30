use super::*;

impl<'a> Parser<'a> {
    /// The statement list of a `Block`, `try`, `catch` or `finally` body plus
    /// its §14.2.1 early-error check (LexicallyDeclaredNames has no duplicates
    /// and does not overlap VarDeclaredNames). Lexical-name bookkeeping is
    /// interleaved per statement so redeclaration errors are reported before a
    /// later syntax error in the same body.
    ///
    /// `stop_at_eof` is false for the `try` bodies: an unterminated body must
    /// fail inside `parse_statement_or_declaration` with its own error, not
    /// silently stop at `Eof` here.
    pub(super) fn parse_block_statements(
        &mut self,
        stop_at_eof: bool,
    ) -> Result<Vec<Statement>, ParseError> {
        let mut stmts = Vec::new();
        let mut lexical_names: Vec<String> = Vec::new();
        let mut func_decl_names: Vec<String> = Vec::new();
        while self.current != Token::RightBrace && !(stop_at_eof && self.current == Token::Eof) {
            let stmt = self.parse_statement_or_declaration()?;
            Self::collect_lexical_names_with_func_names(
                &stmt,
                &mut lexical_names,
                &mut Some(&mut func_decl_names),
                self.strict,
            )?;
            stmts.push(stmt);
        }
        if !lexical_names.is_empty() {
            let mut var_names = Vec::new();
            for stmt in &stmts {
                Self::collect_var_declared_names(stmt, &mut var_names);
            }
            for name in &var_names {
                if lexical_names.contains(name) {
                    return Err(ParseError {
                        message: format!("Identifier '{name}' has already been declared"),
                    });
                }
            }
        }
        Ok(stmts)
    }
}
