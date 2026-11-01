//! CoverInitializedName (ECMAScript 13.2.5): `{ a = 1 }` is only valid where
//! the enclosing literal is reinterpreted as an AssignmentPattern, an
//! arrow-parameter list or a for-in/of head; anywhere else it is an early
//! Syntax Error (13.2.5.1). The literal is parsed as an expression first, so
//! the parser records a pending initializer and the first construct that
//! settles what the literal is either consumes it or rejects it.

use super::*;

#[derive(Default, Clone, Copy)]
pub(super) struct CoverInit {
    /// A CoverInitializedName was parsed that no pattern has claimed yet.
    pending: bool,
    /// The next AssignmentExpression may leave its own literal pending,
    /// because its caller decides whether that literal becomes a pattern.
    defer: bool,
}

/// What `begin_cover_scope` set aside for `end_cover_scope`.
pub(super) struct CoverScope {
    deferred: bool,
    outer: bool,
}

const MESSAGE: &str = "Invalid shorthand property initializer";

impl<'a> Parser<'a> {
    pub(super) fn note_cover_initialized_name(&mut self) {
        self.cover_init.pending = true;
    }

    /// An element of an array or object literal, or of a parenthesised list:
    /// each may itself be a pattern.
    pub(super) fn parse_cover_element(&mut self) -> Result<Expression, ParseError> {
        self.cover_init.defer = true;
        self.parse_assignment_expression()
    }

    /// Start of an AssignmentExpression: pending state of the enclosing
    /// expression is set aside, and whether this one defers is read.
    pub(super) fn begin_cover_scope(&mut self) -> CoverScope {
        CoverScope {
            deferred: std::mem::take(&mut self.cover_init.defer),
            outer: std::mem::take(&mut self.cover_init.pending),
        }
    }

    /// `left = ...` with an ArrayLiteral or ObjectLiteral on the left makes
    /// that literal an AssignmentPattern, which claims its initializers.
    pub(super) fn claim_cover_for_pattern(&mut self, op: AssignOp, left: &Expression) {
        if op == AssignOp::Assign && matches!(left, Expression::Object(..) | Expression::Array(..))
        {
            self.cover_init.pending = false;
        }
    }

    pub(super) fn end_cover_scope(
        &mut self,
        CoverScope { deferred, outer }: CoverScope,
        expr: &Expression,
    ) -> Result<(), ParseError> {
        let pending = self.cover_init.pending;
        if pending && !(deferred && matches!(expr, Expression::Object(..) | Expression::Array(..)))
        {
            return Err(self.error(MESSAGE));
        }
        self.cover_init.pending = outer || pending;
        Ok(())
    }

    /// A parenthesised list that closes with `)`: arrow parameters claim the
    /// initializers of their elements, a plain expression cannot have any.
    pub(super) fn end_cover_list(&mut self, outer: bool, arrow: bool) -> Result<(), ParseError> {
        if self.cover_init.pending && !arrow {
            return Err(self.error(MESSAGE));
        }
        self.cover_init.pending = outer;
        Ok(())
    }

    pub(super) fn begin_cover_list(&mut self) -> bool {
        std::mem::take(&mut self.cover_init.pending)
    }

    /// `class C extends <LeftHandSideExpression>` is never a pattern.
    pub(super) fn parse_heritage_expression(&mut self) -> Result<Expression, ParseError> {
        let outer = self.begin_cover_list();
        let expr = self.parse_left_hand_side_expression()?;
        self.end_cover_list(outer, false)?;
        Ok(expr)
    }

    /// The expression of a `for (` head that is not a declaration: it is a
    /// pattern when `in` or `of` follows.
    pub(super) fn parse_for_head_expression(&mut self) -> Result<Expression, ParseError> {
        let outer = self.begin_cover_list();
        self.cover_init.defer = true;
        let expr = self.parse_expression()?;
        let pattern = matches!(self.current, Token::Keyword(Keyword::In | Keyword::Of));
        self.end_cover_list(outer, pattern)?;
        Ok(expr)
    }
}
