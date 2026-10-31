//! What the parser does differently once the `--node` stack hooks are
//! installed (issues #1483, #1484): dynamic `import()` carries the position V8
//! reports for the call, and the common "this token is not allowed here"
//! errors read like V8's (`Unexpected token '='`) instead of naming the
//! production the parser was in. Default mode keeps its own wording.

use super::*;

impl<'a> Parser<'a> {
    /// `await import(...)`, `await import(...).then(...)`: V8 reports the
    /// `await` for the call, so the dynamic import takes its position.
    pub(super) fn retarget_import(expr: &mut Expression, at: u32) {
        match expr {
            Expression::Import(_, _, call) => *call = at,
            Expression::Call(callee, ..) | Expression::Member(callee, ..) => {
                Self::retarget_import(callee, at)
            }
            _ => {}
        }
    }

    fn raw_current(&self) -> &str {
        self.source_text_source
            .get(self.current_token_start..self.current_token_end)
            .unwrap_or("")
    }

    /// V8's message for the current token, which the grammar has no place for.
    fn v8_unexpected(&self) -> String {
        let raw = self.raw_current();
        match &self.current {
            Token::Eof => "Unexpected end of input".to_string(),
            Token::Keyword(Keyword::Let | Keyword::Static | Keyword::Yield) if self.strict => {
                "Unexpected strict mode reserved word".to_string()
            }
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
        self.v8_error("Expected semicolon")
    }
}
