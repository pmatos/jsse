use super::*;

impl<'a> Parser<'a> {
    pub(in crate::parser) fn parse_variable_declaration_list(
        &mut self,
    ) -> Result<Vec<VariableDeclarator>, ParseError> {
        self.parse_declarators(false)
    }

    /// The declaration list of a `for (var|let|const ...` head, where a
    /// declarator may turn out to be the ForBinding of a for-in/for-of.
    pub(in crate::parser) fn parse_for_declaration_list(
        &mut self,
    ) -> Result<Vec<VariableDeclarator>, ParseError> {
        self.parse_declarators(true)
    }

    fn parse_declarators(&mut self, for_head: bool) -> Result<Vec<VariableDeclarator>, ParseError> {
        let mut decls = Vec::new();
        loop {
            let pattern = self.parse_binding_pattern()?;
            let init = if self.current == Token::Assign {
                self.advance()?;
                Some(self.parse_assignment_expression()?)
            } else {
                // A BindingPattern declarator needs an Initializer unless it is
                // the ForBinding of a for-in/for-of head (§14.3.1.1, §14.3.2.1).
                let for_binding =
                    for_head && matches!(self.current, Token::Keyword(Keyword::In | Keyword::Of));
                if !matches!(pattern, Pattern::Identifier(_)) && !for_binding {
                    return Err(self.error("Missing initializer in destructuring declaration"));
                }
                None
            };
            decls.push(VariableDeclarator { pattern, init });
            if self.current == Token::Comma {
                self.advance()?;
            } else {
                break;
            }
        }
        Ok(decls)
    }
}
