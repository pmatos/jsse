//! Arrow parameters (ECMAScript 13.2.1.1, 15.3.1): a parenthesised list is
//! parsed as a CoverParenthesizedExpressionAndArrowParameterList and, once
//! `=>` follows, reinterpreted as ArrowFormalParameters. The reinterpretation
//! is not a mechanical rewrite of the expressions: a MemberExpression, a
//! parenthesised binding, an object or array literal under an object rest are
//! all valid expressions but not binding elements. The list is therefore
//! parsed a second time as FormalParameters, which is the refinement the
//! specification describes.

use super::v8_messages::patterns::Site;
use super::*;

/// Token-stream position, enough to resume parsing from a token boundary.
/// Taken at the opening parenthesis, so it holds no allocation.
pub(super) struct TokenMark<'a> {
    lexer: Lexer<'a>,
    current: Token,
    current_token_start: usize,
    current_token_end: usize,
    prev_token_end: usize,
    prev_line_terminator: bool,
}

impl<'a> Parser<'a> {
    pub(super) fn token_mark(&self) -> TokenMark<'a> {
        TokenMark {
            lexer: self.lexer.clone(),
            current: self.current.clone(),
            current_token_start: self.current_token_start,
            current_token_end: self.current_token_end,
            prev_token_end: self.prev_token_end,
            prev_line_terminator: self.prev_line_terminator,
        }
    }

    pub(super) fn rewind_to(&mut self, mark: TokenMark<'a>) {
        self.lexer = mark.lexer;
        self.current = mark.current;
        self.current_token_start = mark.current_token_start;
        self.current_token_end = mark.current_token_end;
        self.prev_token_end = mark.prev_token_end;
        self.prev_line_terminator = mark.prev_line_terminator;
    }

    /// Converts the elements of a cover list into the parameters of an arrow
    /// function. `open_paren` is the mark taken at the opening parenthesis;
    /// the current token is the one after `=>`.
    pub(super) fn arrow_params_from_cover(
        &mut self,
        exprs: Vec<Expression>,
        open_paren: TokenMark<'a>,
        is_async: bool,
    ) -> Result<Vec<Pattern>, ParseError> {
        if inferred_names::enabled()
            && exprs
                .iter()
                .any(|e| matches!(e, Expression::Yield(..) | Expression::Await(..)))
        {
            return Err(self.error("Invalid destructuring assignment target"));
        }
        if is_async {
            for e in &exprs {
                if self.in_async
                    && inferred_names::enabled()
                    && Self::expr_contains_await_expression(e)
                {
                    return Err(self.error(
                        "Await expression is not allowed in formal parameters of an async function",
                    ));
                }
                if Self::expr_contains_await_identifier(e)
                    || Self::expr_contains_await_expression(e)
                {
                    return Err(
                        self.error("'await' is not allowed in async arrow formal parameters")
                    );
                }
            }
        } else {
            if self.in_async {
                for e in &exprs {
                    if Self::expr_contains_await_expression(e) {
                        return Err(self.error("Await expression is not allowed in formal parameters of an async function"));
                    }
                }
            }
            if self.in_generator {
                for e in &exprs {
                    if Self::expr_contains_yield_expression(e) {
                        return Err(self.error("Yield expression is not allowed in formal parameters of a generator function"));
                    }
                }
            }
        }
        if !self.is_plain_identifier_list(&open_paren) {
            self.reparse_as_formal_parameters(open_paren)?;
        }
        let params: Vec<Pattern> = exprs
            .into_iter()
            .map(expr_to_pattern)
            .collect::<Result<_, _>>()?;
        if self.strict {
            self.check_strict_params(&params)?;
        }
        self.check_duplicate_params_strict(&params)?;
        Ok(params)
    }

    /// `(a, b)`: nothing but identifiers and commas up to the closing
    /// parenthesis, so there is no binding element to refine.
    fn is_plain_identifier_list(&self, open_paren: &TokenMark<'a>) -> bool {
        let rest = &self.source.as_bytes()[open_paren.current_token_end..];
        for &byte in rest {
            match byte {
                b')' => return true,
                b'a'..=b'z'
                | b'A'..=b'Z'
                | b'0'..=b'9'
                | b'_'
                | b'$'
                | b','
                | b' '
                | b'\t'
                | b'\n' => {}
                _ => return false,
            }
        }
        false
    }

    /// Parses the tokens after `open_paren` up to the closing parenthesis as
    /// FormalParameters, then resumes at the current token. Nested arrows
    /// inside the list were refined when the list was first parsed, so they
    /// are not refined again.
    fn reparse_as_formal_parameters(
        &mut self,
        open_paren: TokenMark<'a>,
    ) -> Result<(), ParseError> {
        if self.refining_arrow_params {
            return Ok(());
        }
        let resume = self.token_mark();
        let saved_flags = (
            self.last_expr_parenthesized,
            self.parenthesized_arrow,
            self.last_obj_had_proto_dup,
            self.no_in,
        );
        self.rewind_to(open_paren);
        self.refining_arrow_params = true;
        self.no_in = false;
        let result = self
            .advance()
            .and_then(|_| self.parse_parameter_list_tail());
        self.refining_arrow_params = false;
        (
            self.last_expr_parenthesized,
            self.parenthesized_arrow,
            self.last_obj_had_proto_dup,
            self.no_in,
        ) = saved_flags;
        self.rewind_to(resume);
        result
    }

    fn parse_parameter_list_tail(&mut self) -> Result<(), ParseError> {
        while self.current != Token::RightParen {
            if self.current == Token::Ellipsis {
                self.advance()?;
                self.arrow_rest_parameter()?;
                break;
            }
            self.pattern_element(Site::ArrowParam)?;
            if self.current != Token::Comma {
                break;
            }
            self.advance()?;
        }
        if self.current != Token::RightParen {
            return Err(self.error("Invalid destructuring assignment target"));
        }
        Ok(())
    }
}
