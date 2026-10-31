//! What the parser does differently once the `--node` stack hooks are
//! installed (issues #1483, #1484): dynamic `import()` carries the position V8
//! reports for the call, and the common "this token is not allowed here"
//! errors read like V8's (`Unexpected token '='`) instead of naming the
//! production the parser was in. Default mode keeps its own wording.

use super::*;

impl<'a> Parser<'a> {
    /// `await import(...)`, `await import(...).then(...)`: V8 reports the
    /// `await` for the call, so the dynamic import takes its position, unless
    /// the specifier is read by a node that has a position of its own.
    pub(super) fn retarget_import(expr: &mut Expression, at: u32) {
        match expr {
            Expression::Import(spec, _, call) if !Self::positions_itself(spec) => *call = at,
            Expression::Call(callee, ..) | Expression::Member(callee, ..) => {
                Self::retarget_import(callee, at)
            }
            _ => {}
        }
    }

    /// A specifier that is a variable or a property read: V8's latest emitted
    /// position when `import()` runs is that load, not the call.
    fn positions_itself(spec: &Expression) -> bool {
        match spec {
            Expression::Identifier(_) => true,
            Expression::Member(object, MemberProperty::Dot(_), _) => {
                matches!(&**object, Expression::Identifier(_)) || Self::positions_itself(object)
            }
            _ => false,
        }
    }

    /// `import(spec)`: the position V8 reports for the call.
    pub(super) fn import_call(
        &self,
        spec: Expression,
        options: Option<ExprBox>,
        at: u32,
        (spec_start, spec_end): (usize, usize),
    ) -> Expression {
        let mut at = at;
        if inferred_names::enabled() && Self::positions_itself(&spec) {
            let read = match &spec {
                Expression::Member(_, MemberProperty::Dot(name), _) => {
                    spec_end.saturating_sub(name.len())
                }
                _ => spec_start,
            };
            at = CallSiteId::at(read).1;
        }
        Expression::Import(ExprBox::new(spec), options, at)
    }

    /// The optional second argument of `import(spec, options)`.
    pub(super) fn parse_import_options(&mut self) -> Result<Option<ExprBox>, ParseError> {
        if self.current != Token::Comma {
            return Ok(None);
        }
        self.advance()?;
        if self.current == Token::RightParen {
            return Ok(None);
        }
        let options = self.parse_assignment_expression()?;
        if self.current == Token::Comma {
            self.advance()?;
        }
        Ok(Some(ExprBox::new(options)))
    }

    fn raw_current(&self) -> &str {
        self.source_text_source
            .get(self.current_token_start..self.current_token_end)
            .unwrap_or("")
    }

    /// V8's message for the current token, which the grammar has no place for.
    pub(super) fn v8_unexpected(&self) -> String {
        let raw = self.raw_current();
        match &self.current {
            Token::Eof => "Unexpected end of input".to_string(),
            Token::Keyword(Keyword::Let | Keyword::Static | Keyword::Yield) if self.strict => {
                "Unexpected strict mode reserved word".to_string()
            }
            Token::Identifier(name)
                if self.strict
                    && [
                        "implements",
                        "interface",
                        "package",
                        "private",
                        "protected",
                        "public",
                    ]
                    .contains(&name.as_str()) =>
            {
                "Unexpected strict mode reserved word".to_string()
            }
            Token::Keyword(Keyword::Enum) => "Unexpected reserved word".to_string(),
            Token::At => "Invalid or unexpected token".to_string(),
            Token::Keyword(Keyword::Await) if self.in_async || self.is_module => {
                "Unexpected reserved word".to_string()
            }
            Token::Identifier(_)
            | Token::IdentifierWithEscape(_)
            | Token::PrivateName(_)
            | Token::Keyword(Keyword::Await | Keyword::Let | Keyword::Static | Keyword::Yield) => {
                format!("Unexpected identifier '{raw}'")
            }
            Token::NumericLiteral(_)
            | Token::LegacyOctalLiteral(_)
            | Token::NonOctalDecimalLiteral(_)
            | Token::BigIntLiteral(_) => "Unexpected number".to_string(),
            Token::StringLiteral(_) => "Unexpected string".to_string(),
            Token::NoSubstitutionTemplate(..)
            | Token::TemplateHead(..)
            | Token::TemplateMiddle(..)
            | Token::TemplateTail(..) => "Unexpected template string".to_string(),
            Token::RegExpLiteral { .. } => "Unexpected regular expression".to_string(),
            _ => format!("Unexpected token '{raw}'"),
        }
    }

    /// The error for a token that is not allowed here. `fallback` is the
    /// parser's own wording followed by the token, used outside `--node`.
    pub(super) fn unexpected_token(&self, fallback: &str) -> ParseError {
        self.v8_error(&format!("{fallback}{:?}", self.current))
    }

    /// `message` outside `--node`, V8's wording for the current token inside.
    pub(super) fn v8_error(&self, message: &str) -> ParseError {
        if inferred_names::enabled() {
            self.error(self.v8_unexpected())
        } else {
            self.error(message)
        }
    }

    /// `f(1 2)`: V8 asks for the closing parenthesis of the argument list.
    pub(super) fn unterminated_arguments(&self) -> ParseError {
        if inferred_names::enabled() {
            self.error("missing ) after argument list")
        } else {
            self.expected_error(&Token::RightParen)
        }
    }

    pub(super) fn expected_error(&self, expected: &Token) -> ParseError {
        self.v8_error(&format!("Expected {expected:?}, got {:?}", self.current))
    }

    pub(super) fn expected_semicolon(&self) -> ParseError {
        let before = self.source_text_source[..self.prev_token_end].trim_end();
        if inferred_names::enabled()
            && before.ends_with("await")
            && !before[..before.len() - 5]
                .ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '$')
        {
            return self.error(
                "await is only valid in async functions and the top level bodies of modules",
            );
        }
        self.v8_error("Expected semicolon")
    }
}
