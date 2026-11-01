use super::*;

impl<'a> Parser<'a> {
    pub(in crate::parser) fn parse_variable_declaration_list(
        &mut self,
    ) -> Result<Vec<VariableDeclarator>, ParseError> {
        self.parse_declarators(false, false)
    }

    /// The declarators of `let` / `const`. V8 reports a `const` without
    /// initializer as soon as its declarator ends, and a strict `let` that
    /// starts no declaration as the reserved word it is (`--node` only).
    pub(in crate::parser) fn lexical_declarators(
        &mut self,
        kind: VarKind,
    ) -> Result<Vec<VariableDeclarator>, ParseError> {
        self.reserved_let(kind == VarKind::Let)?;
        self.parse_declarators(false, kind == VarKind::Const)
    }

    fn reserved_let(&self, is_let: bool) -> Result<(), ParseError> {
        if !(is_let && self.strict && inferred_names::enabled()) {
            return Ok(());
        }
        let binding_start = self.current_identifier_name().is_some()
            || matches!(self.current, Token::LeftBracket | Token::LeftBrace);
        let exported = self
            .source_before_prev_token("let")
            .is_some_and(|before| before.trim_end().ends_with("export"));
        if !binding_start && !exported {
            if self.current == Token::Keyword(Keyword::Await) {
                return Err(self.error("Unexpected reserved word"));
            }
            return Err(self.error("Unexpected strict mode reserved word"));
        }
        Ok(())
    }

    /// The declaration list of a `for (var|let|const ...` head, where a
    /// declarator may turn out to be the ForBinding of a for-in/for-of.
    pub(in crate::parser) fn parse_for_declaration_list(
        &mut self,
    ) -> Result<Vec<VariableDeclarator>, ParseError> {
        let after_let = self.source_before_prev_token("let").is_some();
        let after_const = self.source_before_prev_token("const").is_some();
        self.reserved_let(after_let)?;
        let await_pattern = self.for_await_pattern();
        let decls = self.parse_declarators(true, after_const);
        if await_pattern && decls.is_err() {
            return Err(self.for_await_binding_error());
        }
        decls
    }

    fn parse_declarators(
        &mut self,
        for_head: bool,
        is_const: bool,
    ) -> Result<Vec<VariableDeclarator>, ParseError> {
        let mut decls = Vec::new();
        loop {
            let pattern = self.parse_binding_pattern()?;
            let mut init_at = 0;
            let init = if self.current == Token::Assign {
                self.advance()?;
                init_at = self.op_pos();
                let init = self.parse_assignment_expression()?;
                // V8 words a parenthesized name by its value, not its name
                // (#1557); a one-element sequence keeps the parentheses.
                if crate::parser::inferred_names::enabled()
                    && matches!(pattern, Pattern::Array(_))
                    && matches!(init, Expression::Identifier(_))
                    && self.last_expr_parenthesized
                {
                    Some(Expression::Sequence(vec![init]))
                } else {
                    Some(init)
                }
            } else {
                // A BindingPattern declarator needs an Initializer unless it is
                // the ForBinding of a for-in/for-of head (§14.3.1.1, §14.3.2.1).
                let for_binding =
                    for_head && matches!(self.current, Token::Keyword(Keyword::In | Keyword::Of));
                if !matches!(pattern, Pattern::Identifier(_)) && !for_binding {
                    return Err(self.error("Missing initializer in destructuring declaration"));
                }
                if is_const && self.current == Token::Comma {
                    return Err(self.error("Missing initializer in const declaration"));
                }
                None
            };
            decls.push(VariableDeclarator {
                pattern,
                init,
                init_at,
            });
            if self.current == Token::Comma {
                self.advance()?;
            } else {
                break;
            }
        }
        Ok(decls)
    }
}
