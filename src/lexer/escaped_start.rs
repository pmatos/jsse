//! An identifier that starts with a unicode escape. V8 words an escape that is
//! well formed but names no identifier start (`\u0023`, `\u{D800}`) as an
//! illegal token, and a malformed one (`\u00`) as such.

use super::{LexError, Lexer, Token};

impl Lexer<'_> {
    /// The `\` is consumed and `u` is next.
    pub(super) fn read_escaped_start(&mut self) -> Result<Token, LexError> {
        self.advance(); // consume 'u'
        let esc = self.read_unicode_escape()?;
        match esc.chars().next() {
            Some(first) if Self::is_identifier_start(first) => {
                self.read_identifier_with_escape(first)
            }
            _ => Err(self.error("Invalid Unicode escape in identifier")),
        }
    }
}
