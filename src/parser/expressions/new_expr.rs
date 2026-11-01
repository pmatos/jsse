use crate::ast::{CallSiteId, ExprBox, Expression, MemberProperty, PropSiteId};
use crate::lexer::{Keyword, Token};
use crate::parser::{ParseError, Parser, v8_position};

impl Parser<'_> {
    pub(super) fn parse_new_expression(&mut self) -> Result<Expression, ParseError> {
        // Depth guard: `new new … X` recurses here, bypassing
        // parse_assignment_expression, so it needs its own bound.
        self.enter_recursion()?;
        let r = self.parse_new_expression_inner();
        self.exit_recursion();
        r
    }

    pub(super) fn parse_new_expression_inner(&mut self) -> Result<Expression, ParseError> {
        let new_pos = self.current_token_start;
        self.advance()?; // new
        if self.current == Token::Dot {
            self.advance()?; // .
            let is_target = match &self.current {
                Token::Identifier(n) => n == "target",
                Token::IdentifierWithEscape(n) if n == "target" => {
                    return Err(self.error("Keyword must not contain escaped characters"));
                }
                _ => false,
            };
            if is_target {
                if self.in_non_arrow_function == 0 {
                    return Err(self.error("new.target expression is not allowed here"));
                }
                self.advance()?; // target
                v8_position::expression(v8_position::at(new_pos));
                return Ok(Expression::NewTarget);
            }
            return Err(self.error("Expected 'target' after 'new.'"));
        }
        let mut callee = if self.current == Token::Keyword(Keyword::New) {
            self.parse_new_expression()?
        } else {
            // `new import(...)` is an error; `new (import(...))` is valid.
            let is_direct_import = matches!(self.current, Token::Keyword(Keyword::Import));
            let primary = self.parse_primary_expression()?;
            if is_direct_import
                && matches!(
                    &primary,
                    Expression::Import(_, _, _)
                        | Expression::ImportDefer(_, _)
                        | Expression::ImportSource(_, _)
                )
            {
                return Err(self.error("Cannot use 'new' with import()"));
            }
            primary
        };
        loop {
            match &self.current {
                Token::OptionalChain => {
                    return Err(self.error("Invalid optional chain from new expression"));
                }
                Token::Dot => {
                    self.advance()?;
                    let at = self.current_token_start;
                    let prop = self.parse_dot_member_property()?;
                    v8_position::expression(v8_position::at(at));
                    callee = Expression::Member(ExprBox::new(callee), prop, PropSiteId::at(at));
                }
                Token::LeftBracket => {
                    let at = self.current_token_start;
                    self.advance()?;
                    let prop = self.parse_expression()?;
                    self.eat(&Token::RightBracket)?;
                    v8_position::expression(v8_position::at(at));
                    callee = Expression::Member(
                        ExprBox::new(callee),
                        MemberProperty::Computed(ExprBox::new(prop)),
                        PropSiteId::at(at),
                    );
                }
                Token::NoSubstitutionTemplate(_, _) | Token::TemplateHead(_, _) => {
                    v8_position::early_expression(v8_position::at(self.current_token_start));
                    let tmpl = self.parse_template_literal_expr(true)?;
                    callee = Expression::TaggedTemplate(ExprBox::new(callee), Box::new(tmpl));
                }
                _ => break,
            }
        }
        let args = if self.current == Token::LeftParen {
            self.parse_arguments()?
        } else {
            Vec::new()
        };
        v8_position::expression(v8_position::at(new_pos));
        Ok(Expression::New(
            ExprBox::new(callee),
            args,
            CallSiteId::at(new_pos),
        ))
    }
}
