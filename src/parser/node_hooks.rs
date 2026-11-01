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
            Expression::Identifier(_, _) => true,
            Expression::Member(object, MemberProperty::Dot(_), _) => Self::positions_itself(object),
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
            let (read, name) = match &spec {
                Expression::Member(_, MemberProperty::Dot(name), _) => {
                    (spec_end.saturating_sub(name.len()), name)
                }
                Expression::Identifier(name, _) => (spec_start, name),
                _ => unreachable!("positions_itself"),
            };
            if self.source_text_source.get(read..read + name.len()) != Some(name.as_str()) {
                return Expression::Import(ExprBox::new(spec), options, at);
            }
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

    /// The current token is the `<` of `<!--` or the `>` of `-->`, which the
    /// lexer leaves to the parser in module code.
    fn html_comment_in_module(&self) -> bool {
        let src = &self.source_text_source;
        self.is_module
            && (src
                .get(self.current_token_start..)
                .is_some_and(|s| s.starts_with("<!--"))
                || self
                    .current_token_start
                    .checked_sub(2)
                    .and_then(|from| src.get(from..))
                    .is_some_and(|s| s.starts_with("-->")))
    }

    /// `async a => ...` for the lookahead of `async`: (function, single
    /// parameter, parenthesized). V8 reads `async a` as the parameter of an
    /// arrow and rejects a line break before `=>`.
    pub(super) fn arrow_after_async_param(
        &self,
        tokens: &Lookahead<'_, '_>,
    ) -> Result<(bool, bool, bool), ParseError> {
        let arrow = tokens.current() == &Token::Arrow;
        if arrow && tokens.line_terminator_before() && inferred_names::enabled() {
            return Err(self.error("Unexpected token '=>'"));
        }
        Ok((false, arrow && !tokens.line_terminator_before(), false))
    }

    /// V8's message for the current token, which the grammar has no place for.
    pub(super) fn v8_unexpected(&self) -> String {
        let raw = self.raw_current();
        match &self.current {
            Token::Eof => "Unexpected end of input".to_string(),
            _ if self.html_comment_in_module() => {
                "HTML comments are not allowed in modules".to_string()
            }
            Token::Keyword(Keyword::Yield) if self.in_generator => {
                "Unexpected identifier 'yield'".to_string()
            }
            Token::Keyword(Keyword::Let | Keyword::Static | Keyword::Yield) if self.strict => {
                "Unexpected strict mode reserved word".to_string()
            }
            Token::Identifier(name)
                if self.strict && super::v8_messages::STRICT_WORDS.contains(&name.as_str()) =>
            {
                "Unexpected strict mode reserved word".to_string()
            }
            Token::Keyword(Keyword::Await | Keyword::Enum) => {
                "Unexpected reserved word".to_string()
            }
            Token::At => "Invalid or unexpected token".to_string(),
            Token::IdentifierWithEscape(name) => self.escaped_identifier_text(name),
            Token::Identifier(_)
            | Token::PrivateName(_)
            | Token::Keyword(Keyword::Let | Keyword::Static | Keyword::Yield) => {
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

    /// V8's text for an identifier written with a unicode escape (`await`).
    fn escaped_identifier_text(&self, name: &str) -> String {
        let text = match name {
            "let" => "Keyword must not contain escaped characters",
            "await" => "Unexpected reserved word",
            "yield" if self.strict => "Unexpected strict mode reserved word",
            _ if Self::is_reserved_identifier(name, false) => {
                "Keyword must not contain escaped characters"
            }
            _ if self.strict && super::v8_messages::STRICT_WORDS.contains(&name) => {
                "Unexpected strict mode reserved word"
            }
            _ => return format!("Unexpected identifier '{name}'"),
        };
        text.to_string()
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

    /// The source before the previous token, when that token is `keyword`.
    pub(super) fn source_before_prev_token(&self, keyword: &str) -> Option<&str> {
        self.source_text_source
            .get(..self.prev_token_end)?
            .strip_suffix(keyword)
    }

    /// `let` as a single statement that goes on with `[`, or, as V8 reads it,
    /// with an identifier or `{` on the same line: a lexical declaration.
    pub(super) fn let_starts_declaration(&self) -> bool {
        let node = inferred_names::enabled();
        self.lookahead(|tokens| {
            tokens.advance().is_ok()
                && match tokens.current() {
                    Token::LeftBracket => true,
                    Token::LeftBrace | Token::Identifier(_) => {
                        node && !tokens.line_terminator_before()
                    }
                    _ => false,
                }
        })
    }

    /// A declaration in a for-in / for-of head with an initializer or, under
    /// `--node`, several bindings, which V8 words as its own error.
    pub(super) fn for_binding_error(&self, decls: &[VariableDeclarator], kind: &str) -> ParseError {
        let kind = if kind == "of" && inferred_names::enabled() && self.in_for_await_head() {
            "await-of"
        } else {
            kind
        };
        if decls.len() != 1 && inferred_names::enabled() {
            return self.error(format!(
                "Invalid left-hand side in for-{kind} loop: Must have a single binding."
            ));
        }
        self.error(format!(
            "for-{kind} loop variable declaration may not have an initializer"
        ))
    }

    /// Whether the `(` that opens the head being parsed follows `for await`.
    fn in_for_await_head(&self) -> bool {
        let src = &self.source_text_source[..self.current_token_start];
        let mut depth = 0usize;
        let Some(open) = src.bytes().rposition(|c| match c {
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
        }) else {
            return false;
        };
        let words: Vec<&str> = src[..open].split_whitespace().rev().take(2).collect();
        words == ["await", "for"]
    }

    pub(super) fn var_for_in_error(&self, decls: &[VariableDeclarator]) -> ParseError {
        if inferred_names::enabled() {
            return self.for_binding_error(decls, "in");
        }
        self.error("Invalid left-hand side in for-in loop")
    }

    /// The head of a for-of loop starts with the token `let` (§14.7.5: the
    /// left-hand side cannot, so `for (let.x of y)` is rejected as well).
    pub(super) fn head_is_let(&self, head_start: usize) -> bool {
        self.source_text_source
            .get(head_start..)
            .and_then(|rest| rest.strip_prefix("let"))
            .is_some_and(|rest| {
                !rest.starts_with(|c: char| c.is_alphanumeric() || "_$\\".contains(c))
            })
    }

    /// `for (let of x)`: V8 reads `of` as the declared name and stops at `x`.
    pub(super) fn let_of_error(&mut self, head_start: usize) -> ParseError {
        if !inferred_names::enabled() {
            return self.error(
                "The identifier 'let' is disallowed as a left-hand side expression of a for-of loop",
            );
        }
        let head = self.source_text_source.get(head_start..self.prev_token_end);
        if head.is_some_and(|head| head.trim() != "let") {
            return self.error("The left-hand side of a for-of loop may not start with 'let'.");
        }
        let _ = self.advance();
        self.error(self.v8_unexpected())
    }

    pub(super) fn reserved_await(&self) -> bool {
        self.in_static_block && !self.in_async
    }

    pub(super) fn class_binding(&mut self, name: String) -> Result<String, ParseError> {
        if matches!(name.as_str(), "eval" | "arguments") {
            return Err(self.error("Assignment to 'eval' or 'arguments' in strict mode"));
        }
        self.advance()?;
        Ok(name)
    }

    pub(super) fn eat_named_imports(&mut self) -> Result<(), ParseError> {
        if self.current != Token::LeftBrace && inferred_names::enabled() {
            return Err(self.error("Unexpected token ','"));
        }
        self.eat(&Token::LeftBrace)
    }

    pub(super) fn eat_at(&mut self) -> Result<(), ParseError> {
        if inferred_names::enabled() {
            return Err(self.error("Expected decorator expression after '@'"));
        }
        self.advance().map(drop)
    }

    pub(super) fn nested_eval_error(&self) -> ParseError {
        if inferred_names::enabled() {
            return self.error("Invalid destructuring assignment target");
        }
        self.error("Assignment to 'eval' or 'arguments' in strict mode")
    }

    /// `class let {}`: the class is strict while its name is read, so V8
    /// words the token as a reserved word before the caller's mode returns.
    pub(super) fn class_name_error(&mut self, outer_strict: bool) -> ParseError {
        let error = self.v8_error("Expected class name");
        self.set_strict(outer_strict);
        error
    }

    pub(super) fn import_specifier(&mut self) -> Result<Expression, ParseError> {
        if self.current == Token::RightParen && inferred_names::enabled() {
            return Err(self.error("import() requires a specifier"));
        }
        self.parse_assignment_expression()
    }

    pub(super) fn eat_args_end(&mut self) -> Result<(), ParseError> {
        if self.current != Token::RightParen {
            return Err(self.unterminated_arguments());
        }
        self.advance().map(drop)
    }

    pub(super) fn expected_error(&self, expected: &Token) -> ParseError {
        self.v8_error(&format!("Expected {expected:?}, got {:?}", self.current))
    }

    pub(super) fn expected_semicolon(&self) -> ParseError {
        if inferred_names::enabled() && self.current == Token::Arrow && !self.prev_line_terminator {
            return self.error("Malformed arrow function parameter list");
        }
        let before = self.source_text_source[..self.prev_token_end].trim_end();
        if inferred_names::enabled()
            && before.ends_with("await")
            && !before[..before.len() - 5]
                .ends_with(|c: char| c.is_alphanumeric() || matches!(c, '_' | '$' | '#'))
        {
            return self.error(
                "await is only valid in async functions and the top level bodies of modules",
            );
        }
        self.v8_error("Expected semicolon")
    }
}
