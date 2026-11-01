//! Where `--node` parses or words a source like V8 (#1523). Default mode keeps
//! the engine's own behaviour and text.

use super::super::*;

impl<'a> Parser<'a> {
    /// A call as the target of `=`, `++`, `for (... of/in)`. Web compatibility
    /// lets sloppy code write one (a ReferenceError at run time); V8 does so
    /// in strict code as well, where ECMAScript has an early error.
    pub(in crate::parser) fn call_target(&self, expr: &Expression) -> bool {
        matches!(expr, Expression::Call(..)) && (!self.strict || inferred_names::enabled())
    }

    /// `using x = 1` where a using declaration may not stand. V8 reads `using`
    /// as a name, so the binding is the token it rejects.
    pub(in crate::parser) fn using_not_allowed(&mut self, await_using: bool) -> ParseError {
        if !inferred_names::enabled() {
            let kind = if await_using { "await using" } else { "using" };
            return self.error(format!(
                "{kind} declaration is not allowed in this position"
            ));
        }
        for _ in 0..1 + usize::from(await_using) {
            if let Err(e) = self.advance() {
                return e;
            }
        }
        self.error(self.v8_unexpected())
    }

    /// A using binding that is not followed by `=`.
    pub(in crate::parser) fn using_missing_init(&self, await_using: bool) -> ParseError {
        let kind = if await_using { "await using" } else { "using" };
        if !inferred_names::enabled() {
            return self.error(format!("{kind} declaration requires an initializer"));
        }
        if self.current == Token::Keyword(Keyword::In) {
            return self.error("Invalid 'using' in for-in loop");
        }
        self.error(format!("Missing initializer in {kind} declaration"))
    }

    /// `import defer` and `import.defer()` (and so `import.source`): V8 in
    /// Node 26 has the source phase but not the deferred phase.
    pub(in crate::parser) fn import_phase(&self, name: &str) -> bool {
        name == "source" || (name == "defer" && self.defer_imports())
    }

    pub(in crate::parser) fn defer_imports(&self) -> bool {
        !inferred_names::enabled()
    }

    /// What follows `import.` is neither `meta` nor a phase.
    pub(in crate::parser) fn import_dot_error(&self) -> ParseError {
        if inferred_names::enabled() {
            return self.error(self.v8_unexpected());
        }
        self.error("Expected 'meta', 'defer', or 'source' after 'import.'")
    }

    /// `import.source` is followed by its argument list.
    pub(in crate::parser) fn eat_phase_paren(&mut self) -> Result<(), ParseError> {
        if self.current != Token::LeftParen && inferred_names::enabled() && !self.is_module {
            return Err(self.error("Cannot use import statement outside a module"));
        }
        self.eat(&Token::LeftParen)
    }

    /// The name of a class expression, which is not an identifier here.
    pub(in crate::parser) fn class_expression_name(
        &mut self,
        outer_strict: bool,
    ) -> Result<Option<String>, ParseError> {
        if inferred_names::enabled() {
            return Err(self.class_name_error(outer_strict));
        }
        Ok(None)
    }

    /// The head of a `for await` whose declaration binds a pattern: V8 reads
    /// it twice and words any error of the second reading alike.
    pub(in crate::parser) fn for_await_pattern(&self) -> bool {
        inferred_names::enabled()
            && matches!(self.current, Token::LeftBracket | Token::LeftBrace)
            && self.in_for_await_head()
    }

    pub(in crate::parser) fn for_await_binding_error(&self) -> ParseError {
        self.error("Invalid left-hand side in for-await-of loop: Must have a single binding.")
    }

    /// A private element named `#constructor`: V8 words a method apart from a
    /// field.
    pub(in crate::parser) fn private_constructor_error(
        &self,
        element: &ClassElement,
    ) -> ParseError {
        if inferred_names::enabled() && matches!(element, ClassElement::Method(_)) {
            return self.error("Class constructor may not be a private method");
        }
        self.error("Class fields and methods cannot be named '#constructor'")
    }

    /// The token where a class name was expected. The class is strict, but V8
    /// words `yield` of a generator and an escaped strict word by their own
    /// rules there.
    pub(in crate::parser) fn class_name_text(&self) -> ParseError {
        if !inferred_names::enabled() {
            return self.error("Expected class name");
        }
        match &self.current {
            Token::IdentifierWithEscape(name)
                if name != "await"
                    && !(name == "yield" && self.in_generator)
                    && (name == "let"
                        || name == "yield"
                        || super::super::v8_messages::STRICT_WORDS.contains(&name.as_str())) =>
            {
                self.error("Unexpected strict mode reserved word")
            }
            Token::Keyword(Keyword::Yield) if self.in_generator => {
                self.error("Unexpected identifier 'yield'")
            }
            _ => self.error(self.v8_unexpected()),
        }
    }
}
