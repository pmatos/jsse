//! The long tail of #1523: where `--node` words an early error like V8, or
//! orders two of them like V8. Default mode keeps the engine's own wording.

use super::super::*;

impl<'a> Parser<'a> {
    /// `async * constructor() {}` is a generator before it is an async method.
    pub(in crate::parser) fn async_ctor_message(is_generator: bool) -> &'static str {
        if is_generator {
            "Class constructor may not be a generator"
        } else {
            "Class constructor may not be an async method"
        }
    }

    /// A module reserves `await`, so in the parameters of a function that is
    /// not async it is not an await expression.
    pub(in crate::parser) fn await_in_params(&self) -> ParseError {
        if inferred_names::enabled() && !self.in_async {
            return self.error("Unexpected reserved word");
        }
        self.error("Await expression is not allowed in formal parameters of an async function")
    }

    /// V8 reads the label of `break` / `continue` before it looks for the
    /// loop, so a token that cannot be a label is the error.
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

    /// V8 stops at the token after a keyword used as a shorthand property
    /// (`{ var x }`), unless that token ends the property.
    pub(in crate::parser) fn shorthand_keyword(&self, name: &str) -> ParseError {
        let ends = matches!(self.current, Token::Comma | Token::RightBrace);
        if inferred_names::enabled() && !ends {
            return self.error(self.v8_unexpected());
        }
        self.error(format!("Unexpected token '{name}'"))
    }

    /// An escaped reserved word where an identifier reference stands. V8 words
    /// `yield` by the token; where an assignment expression starts with it in a
    /// generator, `parse_assignment_expression` has already given the keyword text.
    pub(in crate::parser) fn escaped_reserved_error(&self, name: &str) -> ParseError {
        if name == "yield" {
            return self.v8_error("Keyword must not contain escaped characters");
        }
        self.error("Keyword must not contain escaped characters")
    }

    /// The escaped identifier at the current token is reserved where a
    /// shorthand property stands (`yield` and `await` only where they are).
    pub(in crate::parser) fn escaped_shorthand_reserved(&self) -> bool {
        match &self.current {
            Token::IdentifierWithEscape(name) => match name.as_str() {
                "yield" => self.strict || self.in_generator,
                "await" => self.in_async || self.in_static_block || self.is_module,
                _ => Self::is_reserved_identifier(name, self.strict),
            },
            _ => false,
        }
    }

    pub(in crate::parser) fn escaped_shorthand_error(&self, name: &str) -> ParseError {
        if inferred_names::enabled() {
            return self.error(self.escaped_identifier_text(name));
        }
        self.error("Keyword must not contain escaped characters")
    }

    /// An escaped `yield` where an assignment expression starts in a
    /// generator: V8 reads it as a YieldExpression and words the escape as the
    /// keyword. (Default mode words it alike.)
    pub(in crate::parser) fn escaped_yield_start(&self) -> Result<(), ParseError> {
        if self.in_generator
            && matches!(&self.current, Token::IdentifierWithEscape(n) if n == "yield")
        {
            return Err(self.error("Keyword must not contain escaped characters"));
        }
        Ok(())
    }

    pub(in crate::parser) fn duplicate_private_name(&self, name: &str) -> ParseError {
        self.error(format!("Identifier '#{name}' has already been declared"))
    }

    /// V8 reads a pattern after the first binding of a using declaration as a
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

    /// `export { "a" as "<lone surrogate>" }`: V8 reports the string literal
    /// local name before it looks at the exported one.
    pub(in crate::parser) fn string_local_surrogate(
        &self,
        string_local: bool,
    ) -> Result<(), ParseError> {
        let lone = matches!(&self.current, Token::StringLiteral(s) if Self::has_lone_surrogate(s));
        if string_local && lone && inferred_names::enabled() {
            return Err(self.error(
                "A string literal cannot be used as an exported local name without 'from'",
            ));
        }
        Ok(())
    }

    /// `delete this.#m`. V8 finds the covered form `delete (this.#m)` out only
    /// once it knows the name, so a name that no enclosing class has declared
    /// yet is reported as undeclared, even when a later element declares it.
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
    /// body or parameters are read. The tokens of the heritage are scanned
    /// ahead for an arrow, which the real parse would only find after its body.
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
    /// keyword in `for await` and as a plain token otherwise. The head is found
    /// by its `(` in the source text, as `in_for_await_head` does.
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

    /// An escaped `async` before `function` where a statement starts: V8 words
    /// it as the keyword.
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

    /// An escaped `async` before `*` or a method name in a class body.
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

    /// An escaped contextual word where V8 names the whole construct
    /// (`new.target`, `import.meta`, `as`, `from`).
    pub(in crate::parser) fn escaped_word_error(&self, whole: &str) -> ParseError {
        if inferred_names::enabled() {
            return self.error(format!("'{whole}' must not contain escaped characters"));
        }
        self.error("Keyword must not contain escaped characters")
    }

    /// The contextual word that was expected (`as`, `from`) is there, spelled
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
