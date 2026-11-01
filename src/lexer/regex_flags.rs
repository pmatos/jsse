//! The flags of a regular expression literal. V8 words a backslash right after
//! them (`/a/g`) as invalid flags, not as the identifier that follows
//! (`--node` only, issue #1523).

use super::{LexError, Lexer};

impl Lexer<'_> {
    pub(super) fn regex_flags(&mut self) -> Result<String, LexError> {
        let mut flags = String::new();
        while let Some(c) = self.peek().filter(char::is_ascii_alphabetic) {
            flags.push(c);
            self.advance();
        }
        if self.peek() == Some('\\') && crate::parser::inferred_names::enabled() {
            return Err(LexError {
                message: "Invalid regular expression flags".to_string(),
                location: self.location(),
            });
        }
        Ok(flags)
    }
}
