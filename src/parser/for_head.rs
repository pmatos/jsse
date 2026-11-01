//! `for (let` heads (ECMAScript 14.7.5): `let` starts a ForDeclaration unless
//! it is an identifier, which only sloppy-mode source can write.

use super::*;

impl<'a> Parser<'a> {
    /// `for (let` followed by a token that makes `let` an identifier
    /// expression (`for (let in x)`, `for (let = 1;;)`). `for (let of of c)`
    /// and `for (let of in x)` declare a binding named `of`; `for (let of c)`
    /// is read as the identifier head the grammar forbids, for its error.
    pub(super) fn for_let_is_identifier(&self) -> bool {
        self.lookahead(|tokens| {
            if tokens.advance().is_err() {
                return false;
            }
            if tokens.current() == &Token::Keyword(Keyword::Of) {
                return tokens.advance().is_ok()
                    && !matches!(
                        tokens.current(),
                        Token::Keyword(Keyword::Of | Keyword::In)
                            | Token::Assign
                            | Token::Semicolon
                            | Token::Comma
                    );
            }
            matches!(
                tokens.current(),
                Token::Keyword(Keyword::In)
                    | Token::Semicolon
                    | Token::RightParen
                    | Token::Assign
                    | Token::Comma
                    | Token::Dot
                    | Token::Increment
                    | Token::Decrement
                    | Token::OptionalChain
            ) || matches!(tokens.current(), Token::Identifier(n) if n == "instanceof")
        })
    }
}
