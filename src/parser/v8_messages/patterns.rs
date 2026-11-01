//! V8's wording of destructuring errors under `--node` (issue #1523). V8 parses
//! the elements of a binding pattern as expressions and validates them as
//! patterns afterwards, so a bad element reads `Invalid destructuring
//! assignment target` or `Illegal property in declaration context` instead of
//! naming the token the pattern parser stopped at. The pattern parser keeps
//! its own errors; once an element fails, the element is parsed again as an
//! expression and classified the way V8 would. Default mode is untouched.

use crate::parser::arrow_params::TokenMark;
use crate::parser::{
    AssignOp, ExprBox, Expression, MemberProperty, ParseError, Parser, Pattern, PropertyKey, Token,
    inferred_names,
};

const INVALID_TARGET: &str = "Invalid destructuring assignment target";
const ILLEGAL_PROPERTY: &str = "Illegal property in declaration context";

#[derive(Clone, Copy, PartialEq)]
pub(in crate::parser) enum Site {
    Element,
    Rest,
    ArrowParam,
}

impl<'a> Parser<'a> {
    fn pattern_with_default(&mut self) -> Result<Pattern, ParseError> {
        let pat = self.parse_binding_pattern()?;
        if self.current != Token::Assign {
            return Ok(pat);
        }
        self.advance()?;
        let default = self.parse_assignment_expression()?;
        Ok(Pattern::Assign(Box::new(pat), ExprBox::new(default)))
    }

    fn element_end(&self) -> Result<(), ParseError> {
        match self.current {
            Token::Comma | Token::RightBracket | Token::RightBrace | Token::RightParen => Ok(()),
            _ => Err(self.error(self.v8_unexpected())),
        }
    }

    pub(in crate::parser) fn pattern_element(&mut self, site: Site) -> Result<Pattern, ParseError> {
        if !inferred_names::enabled() {
            return self.pattern_with_default();
        }
        let mark = self.token_mark();
        self.pattern_with_default()
            .and_then(|pat| self.element_end().map(|_| pat))
            .map_err(|e| self.reclassify(mark, site, e))
    }

    pub(in crate::parser) fn rest_element(&mut self, object: bool) -> Result<Pattern, ParseError> {
        if !inferred_names::enabled() {
            return self.parse_binding_pattern();
        }
        let mark = self.token_mark();
        let pat = self.parse_binding_pattern();
        match self.current {
            Token::Assign if pat.is_ok() && object => {
                return Err(
                    self.error("`...` must be followed by an identifier in declaration contexts")
                );
            }
            Token::Comma if pat.is_ok() => {
                return Err(self.error("Rest element must be last element"));
            }
            _ => {}
        }
        pat.and_then(|pat| self.element_end().map(|_| pat))
            .map_err(|e| self.reclassify(mark, Site::Rest, e))
    }

    pub(in crate::parser) fn rest_parameter(&mut self) -> Result<Pattern, ParseError> {
        let pat = self.parse_binding_pattern()?;
        if inferred_names::enabled() {
            match self.current {
                Token::Assign => {
                    return Err(self.error("Rest parameter may not have a default initializer"));
                }
                Token::Comma => {
                    return Err(self.error("Rest parameter must be last formal parameter"));
                }
                _ => {}
            }
        }
        Ok(pat)
    }

    pub(in crate::parser) fn arrow_after_rest(&mut self) -> Result<(), ParseError> {
        if self.current != Token::Arrow {
            return Err(if inferred_names::enabled() {
                self.error("Unexpected token '...'")
            } else {
                self.expected_error(&Token::Arrow)
            });
        }
        if self.prev_line_terminator {
            return Err(self.error("Unexpected token '=>'"));
        }
        self.advance().map(drop)
    }

    pub(in crate::parser) fn arrow_rest_parameter(&mut self) -> Result<Pattern, ParseError> {
        let pat = self.rest_parameter()?;
        if inferred_names::enabled() && self.current != Token::RightParen {
            return Err(self.error("Unexpected token '...'"));
        }
        Ok(pat)
    }

    pub(in crate::parser) fn arrow_only_error(&self, exprs: &[Expression]) -> ParseError {
        if inferred_names::enabled() && matches!(exprs.last(), Some(Expression::Spread(..))) {
            return self.error("Unexpected token '...'");
        }
        self.error("Unexpected token ')'")
    }

    pub(in crate::parser) fn colon_after(&mut self, key: &PropertyKey) -> Result<(), ParseError> {
        if self.current != Token::Colon && inferred_names::enabled() {
            match key {
                PropertyKey::Number(_) => return Err(self.error("Unexpected number")),
                PropertyKey::String(_) => return Err(self.error("Unexpected string")),
                PropertyKey::Computed(_) => return Err(self.error("Unexpected token '['")),
                _ => {}
            }
        }
        self.eat(&Token::Colon)
    }

    pub(in crate::parser) fn object_private_key(&self, key: &PropertyKey) -> ParseError {
        match key {
            PropertyKey::Private(name) if inferred_names::enabled() => {
                self.error(format!("Unexpected identifier '#{name}'"))
            }
            _ => self.error("Private fields are not allowed in object literals"),
        }
    }

    fn reclassify(&mut self, mark: TokenMark<'a>, site: Site, original: ParseError) -> ParseError {
        self.rewind_to(mark);
        let expr = match self.parse_assignment_expression() {
            Ok(expr) => expr,
            Err(read) if original.message.starts_with("Unexpected ") => return read,
            Err(_) => return original,
        };
        let parenthesized = self.last_expr_parenthesized;
        match target_error(&expr, parenthesized, site) {
            Some(message) => self.error(message),
            None => original,
        }
    }
}

fn target_error(expr: &Expression, parenthesized: bool, site: Site) -> Option<&'static str> {
    let property = if site == Site::ArrowParam {
        INVALID_TARGET
    } else {
        ILLEGAL_PROPERTY
    };
    match expr {
        Expression::Identifier(_) if parenthesized => Some(INVALID_TARGET),
        Expression::Identifier(_) | Expression::Array(..) | Expression::Object(..) => None,
        Expression::Member(..) if dotted(expr) => Some(property),
        Expression::Assign(AssignOp::Assign, target, ..) if site != Site::Rest => {
            let target: &Expression = target;
            match target {
                Expression::Member(..) if dotted(target) => Some(property),
                Expression::Identifier(_) | Expression::Array(..) | Expression::Object(..) => None,
                _ => Some(INVALID_TARGET),
            }
        }
        _ => Some(INVALID_TARGET),
    }
}

/// `a.b.c`, `f().x`: a property chain without a computed step, which V8 calls an
/// illegal property; `a[0]` and `a[0].b` are plain invalid targets.
fn dotted(expr: &Expression) -> bool {
    match expr {
        Expression::Member(object, MemberProperty::Dot(_), _) => dotted(object),
        Expression::Member(..) => false,
        _ => true,
    }
}
