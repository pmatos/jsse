//! The long tail of #1523: where `--node` words an early error like V8, or
//! orders two of them like V8. Default mode keeps the engine's own wording.

use super::super::*;

const NON_STRICT_FN: &str = "In non-strict mode code, functions can only be declared at top level, inside a block, or as the body of an if statement";

impl<'a> Parser<'a> {
    pub(in crate::parser) fn non_strict_fn_error(&self) -> ParseError {
        self.error(NON_STRICT_FN)
    }

    /// A function declaration where a statement stands.
    pub(in crate::parser) fn function_in_statement_position(&self) -> &'static str {
        if self.strict {
            "In strict mode code, functions can only be declared at top level or inside a block."
        } else {
            "In non-strict mode code, functions can only be declared at top level, inside a block, or as the body of an if statement."
        }
    }

    /// `async * constructor() {}` is a generator before it is an async method.
    pub(in crate::parser) fn async_ctor_message(is_generator: bool) -> &'static str {
        if is_generator {
            "Class constructor may not be a generator"
        } else {
            "Class constructor may not be an async method"
        }
    }

    /// `await` in the parameters of a function that is not async: a module
    /// reserves the word, only an async function words it as an expression.
    pub(in crate::parser) fn await_in_params(&self) -> ParseError {
        if inferred_names::enabled() && !self.in_async {
            return self.error("Unexpected reserved word");
        }
        self.error("Await expression is not allowed in formal parameters of an async function")
    }

    /// `break` / `continue` without a target. V8 reads the label before it
    /// looks for the loop, so a token that cannot be one is the error.
    pub(in crate::parser) fn jump_error(&self, ours: &str) -> ParseError {
        let ends = matches!(
            self.current,
            Token::Semicolon | Token::RightBrace | Token::Eof
        );
        if inferred_names::enabled() && !self.prev_line_terminator && !ends {
            return self.error(self.v8_unexpected());
        }
        self.error(ours)
    }

    /// A keyword used as a shorthand property (`{ var }`). V8 stops at the
    /// token after the name unless that token ends the property.
    pub(in crate::parser) fn shorthand_keyword(&self, name: &str) -> ParseError {
        let ends = matches!(self.current, Token::Comma | Token::RightBrace);
        if inferred_names::enabled() && !ends {
            return self.error(self.v8_unexpected());
        }
        self.error(format!("Unexpected token '{name}'"))
    }

    /// An escaped word that is reserved where an identifier reference stands.
    /// V8 words `yield` by the token, except where a generator's statement
    /// starts with it (a label or an expression statement).
    pub(in crate::parser) fn escaped_reserved_error(&self, name: &str) -> ParseError {
        if name == "yield" && !(self.in_generator && self.starts_statement()) {
            return self.v8_error("Keyword must not contain escaped characters");
        }
        self.error("Keyword must not contain escaped characters")
    }

    /// The current token starts a statement, as far as the text before it
    /// tells (a label also does).
    fn starts_statement(&self) -> bool {
        let before = self.source_text_source[..self.current_token_start].trim_end();
        before.is_empty()
            || before.ends_with(['{', ';', '}'])
            || self.lookahead(|t| t.advance().is_ok() && t.current() == &Token::Colon)
    }

    pub(in crate::parser) fn duplicate_private_name(&self, name: &str) -> ParseError {
        self.error(format!("Identifier '#{name}' has already been declared"))
    }

    /// A using declaration that goes on with a pattern, which V8 reads as a
    /// binding that lacks its initializer.
    pub(in crate::parser) fn using_binding_error(&self) -> ParseError {
        if inferred_names::enabled()
            && matches!(self.current, Token::LeftBracket | Token::LeftBrace)
        {
            return self.error("Missing initializer in using declaration");
        }
        self.error("Expected identifier in using declaration")
    }

    /// `{ eval = 1 }` as a pattern in strict code.
    pub(in crate::parser) fn cover_eval_error(&self) -> ParseError {
        if inferred_names::enabled() {
            return self.error("Assignment to 'eval' in strict mode");
        }
        self.error("Invalid destructuring assignment target")
    }

    /// An export specifier with a string literal local name and a lone
    /// surrogate in its exported name: V8 reports the local name first.
    pub(in crate::parser) fn string_local_surrogate(
        &self,
        error: ParseError,
        string_local: bool,
    ) -> ParseError {
        if string_local && inferred_names::enabled() && error.message.contains("surrogate") {
            return self
                .error("A string literal cannot be used as an exported local name without 'from'");
        }
        error
    }

    /// `delete this.#m`. V8 finds the covered form `delete (this.#m)` out only
    /// once it knows the name, so a name that no enclosing class has declared
    /// yet is reported as undeclared.
    pub(in crate::parser) fn private_delete_error(&self, name: &str) -> ParseError {
        let covered = self
            .source_text_source
            .get(..self.prev_token_end)
            .is_some_and(|text| text.ends_with(')'));
        let declared = self
            .private_name_scopes
            .iter()
            .any(|(names, _)| names.contains(name));
        if inferred_names::enabled() && covered && !declared {
            return self.error(format!(
                "Reference to undeclared private field or method '#{name}'"
            ));
        }
        self.error("Applying the 'delete' operator to a private name is not allowed")
    }

    /// `class C extends () => {} {}`: V8 stops at the `=>`, before the arrow's
    /// body or parameters are read.
    pub(in crate::parser) fn heritage_arrow(&mut self) -> Result<(), ParseError> {
        if !inferred_names::enabled() {
            return Ok(());
        }
        let skip = self.lookahead(|tokens| {
            let mut skip = 0;
            if tokens.current() == &Token::Keyword(Keyword::Async) {
                tokens.advance().ok()?;
                if tokens.line_terminator_before() {
                    return None;
                }
                skip += 1;
            }
            if tokens.current() == &Token::LeftParen {
                let mut depth = 0usize;
                loop {
                    match tokens.current() {
                        Token::LeftParen | Token::LeftBracket | Token::LeftBrace => depth += 1,
                        Token::RightParen | Token::RightBracket | Token::RightBrace => {
                            depth = depth.checked_sub(1)?;
                        }
                        Token::Eof => return None,
                        _ => {}
                    }
                    tokens.advance().ok()?;
                    skip += 1;
                    if depth == 0 {
                        break;
                    }
                }
            } else if matches!(
                tokens.current(),
                Token::Identifier(_) | Token::Keyword(Keyword::Async | Keyword::Await)
            ) {
                tokens.advance().ok()?;
                skip += 1;
            } else {
                return None;
            }
            (tokens.current() == &Token::Arrow && !tokens.line_terminator_before()).then_some(skip)
        });
        for _ in 0..skip.unwrap_or(0) {
            self.advance()?;
        }
        if skip.is_some() {
            return Err(self.error(self.v8_unexpected()));
        }
        Ok(())
    }

    pub(in crate::parser) fn empty_parens_error(&self) -> ParseError {
        if inferred_names::enabled() && self.current == Token::Arrow {
            return self.error(self.v8_unexpected());
        }
        self.error("Unexpected token )")
    }

    /// An escaped `of` where a for head wants the word: V8 words it as the
    /// keyword in `for await` and as a plain token otherwise.
    pub(in crate::parser) fn escaped_of_text(&self) -> Option<&'static str> {
        let src = &self.source_text_source[..self.current_token_start];
        let mut depth = 0usize;
        let open = src.bytes().rposition(|c| match c {
            b')' => {
                depth += 1;
                false
            }
            b'(' if depth == 0 => true,
            b'(' => {
                depth -= 1;
                false
            }
            _ => false,
        })?;
        let words: Vec<&str> = src[..open].split_whitespace().rev().take(2).collect();
        match words.as_slice() {
            ["await", "for"] => Some("Keyword must not contain escaped characters"),
            ["for", ..] => Some("Unexpected token 'of'"),
            _ => None,
        }
    }

    /// `\u0061sync function f() {}` at the start of a statement.
    pub(in crate::parser) fn escaped_async_statement(&self) -> Result<(), ParseError> {
        let escaped = matches!(&self.current, Token::IdentifierWithEscape(n) if n == "async");
        if inferred_names::enabled()
            && escaped
            && self.lookahead(|tokens| {
                tokens.advance().is_ok()
                    && !tokens.line_terminator_before()
                    && tokens.current() == &Token::Keyword(Keyword::Function)
            })
        {
            return Err(self.error("Keyword must not contain escaped characters"));
        }
        Ok(())
    }

    /// `class C { \u0061sync m() {} }`, with or without the `*`.
    pub(in crate::parser) fn escaped_async_member(&self) -> Result<(), ParseError> {
        let escaped = matches!(&self.current, Token::IdentifierWithEscape(n) if n == "async");
        if inferred_names::enabled()
            && escaped
            && self.lookahead(|tokens| {
                tokens.advance().is_ok()
                    && !tokens.line_terminator_before()
                    && (tokens.current() == &Token::Star
                        || Self::is_method_name_start(tokens.current()))
            })
        {
            return Err(self.error("Keyword must not contain escaped characters"));
        }
        Ok(())
    }

    /// An escaped contextual word (`new.target`, `import.meta`, `as`, `from`)
    /// where V8 names the whole construct.
    pub(in crate::parser) fn escaped_word_error(&self, whole: &str) -> ParseError {
        if inferred_names::enabled() {
            return self.error(format!("'{whole}' must not contain escaped characters"));
        }
        self.error("Keyword must not contain escaped characters")
    }

    /// `import * as ns from`: the word that was expected is there, spelled
    /// with an escape.
    pub(in crate::parser) fn expected_contextual(&self, word: &str) -> ParseError {
        match &self.current {
            Token::IdentifierWithEscape(n) if n == word && inferred_names::enabled() => {
                self.escaped_word_error(word)
            }
            _ => self.error(format!("Expected '{word}'")),
        }
    }
}
