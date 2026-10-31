//! Arrow parameters (ECMAScript 13.2.1.1, 15.3.1): a parenthesised list is
//! parsed as a CoverParenthesizedExpressionAndArrowParameterList and, once
//! `=>` follows, reinterpreted as ArrowFormalParameters. The reinterpretation
//! is not a mechanical rewrite of the expressions: a MemberExpression, a
//! parenthesised binding, an object or array literal under an object rest are
//! all valid expressions but not binding elements. The list is therefore
//! parsed a second time as FormalParameters, which is the refinement the
//! specification describes.

use super::*;

/// Token-stream position, enough to resume parsing from a token boundary.
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

    fn rewind_to(&mut self, mark: TokenMark<'a>) {
        self.lexer = mark.lexer;
        self.current = mark.current;
        self.current_token_start = mark.current_token_start;
        self.current_token_end = mark.current_token_end;
        self.prev_token_end = mark.prev_token_end;
        self.prev_line_terminator = mark.prev_line_terminator;
    }

    /// Converts the elements of a cover list into the parameters of an arrow
    /// function. `list_start` is the mark taken right after the opening
    /// parenthesis; the current token is the one after `=>`.
    pub(super) fn arrow_params_from_cover(
        &mut self,
        exprs: Vec<Expression>,
        list_start: TokenMark<'a>,
        is_async: bool,
    ) -> Result<Vec<Pattern>, ParseError> {
        if is_async {
            for e in &exprs {
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
        let params: Vec<Pattern> = exprs
            .into_iter()
            .map(expr_to_pattern)
            .collect::<Result<_, _>>()?;
        if self.strict {
            self.check_strict_params(&params)?;
        }
        self.check_duplicate_params_strict(&params)?;
        self.reparse_as_formal_parameters(list_start)?;
        Ok(params)
    }

    /// Parses the tokens from `list_start` up to the closing parenthesis as
    /// FormalParameters, then resumes at the current token. Nested arrows
    /// inside the list were refined when the list was first parsed, so they
    /// are not refined again.
    fn reparse_as_formal_parameters(
        &mut self,
        list_start: TokenMark<'a>,
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
        self.rewind_to(list_start);
        self.refining_arrow_params = true;
        self.no_in = false;
        let result = self.parse_parameter_list_tail();
        self.refining_arrow_params = false;
        (
            self.last_expr_parenthesized,
            self.parenthesized_arrow,
            self.last_obj_had_proto_dup,
            self.no_in,
        ) = saved_flags;
        self.rewind_to(resume);
        result.map_err(|_| self.error("Invalid destructuring assignment target"))
    }

    fn parse_parameter_list_tail(&mut self) -> Result<(), ParseError> {
        while self.current != Token::RightParen {
            if self.current == Token::Ellipsis {
                self.advance()?;
                self.parse_binding_pattern()?;
                break;
            }
            self.parse_binding_pattern()?;
            if self.current == Token::Assign {
                self.advance()?;
                self.parse_assignment_expression()?;
            }
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
