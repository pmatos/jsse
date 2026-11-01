use super::*;

impl Parser<'_> {
    /// `?.` and the member accesses and calls that follow it, up to the end of
    /// the chain; the current token is the `?.`.
    pub(super) fn parse_optional_chain(
        &mut self,
        expr: Expression,
    ) -> Result<Expression, ParseError> {
        let optional = v8_position::at(self.current_token_start);
        self.advance()?;
        let mut prop = if self.current == Token::LeftParen {
            let site = CallSiteId::at(self.current_token_start);
            let args = self.parse_arguments()?;
            v8_position::expression(site.1);
            Expression::Call(ExprBox::new(Expression::Identifier("".into())), args, site)
        } else if self.current == Token::LeftBracket {
            let bracket = v8_position::at(self.current_token_start);
            self.advance()?;
            let p = self.parse_expression()?;
            self.eat(&Token::RightBracket)?;
            v8_position::expression(bracket);
            Expression::Member(
                ExprBox::new(Expression::Identifier("".into())),
                MemberProperty::Computed(ExprBox::new(p)),
                PropSiteId::UNASSIGNED,
            )
        } else if let Token::PrivateName(name) = &self.current {
            let name = name.clone();
            self.use_private_name(&name)?;
            self.advance()?;
            v8_position::expression(optional);
            Expression::Member(
                ExprBox::new(Expression::Identifier("".into())),
                MemberProperty::Private(name),
                PropSiteId::UNASSIGNED,
            )
        } else {
            let name = match &self.current {
                Token::Identifier(n) | Token::IdentifierWithEscape(n) => n.clone(),
                Token::Keyword(kw) => kw.to_string(),
                Token::BooleanLiteral(value) => if *value { "true" } else { "false" }.to_string(),
                Token::NullLiteral => "null".to_string(),
                _ => return Err(self.error("Expected property after '?.'")),
            };
            self.advance()?;
            v8_position::expression(optional);
            Expression::Identifier(name)
        };
        // Keep consuming .prop, [expr], () in the same optional chain
        let mut called = matches!(prop, Expression::Call(..));
        loop {
            match &self.current {
                Token::Dot => {
                    let at = self.current_token_start;
                    self.advance()?;
                    let name = self.current_token_start;
                    let mp = self.parse_dot_member_property()?;
                    v8_position::expression(v8_position::at(if called { at } else { name }));
                    prop = Expression::Member(ExprBox::new(prop), mp, PropSiteId::at(at));
                }
                Token::LeftBracket => {
                    let at = self.current_token_start;
                    self.advance()?;
                    let p = self.parse_expression()?;
                    self.eat(&Token::RightBracket)?;
                    v8_position::expression(v8_position::at(at));
                    prop = Expression::Member(
                        ExprBox::new(prop),
                        MemberProperty::Computed(ExprBox::new(p)),
                        PropSiteId::at(at),
                    );
                }
                Token::LeftParen => {
                    let site = self.call_site(&prop);
                    let args = self.parse_arguments()?;
                    v8_position::expression(site.1);
                    called = true;
                    prop = Expression::Call(ExprBox::new(prop), args, site);
                }
                Token::NoSubstitutionTemplate(_, _) | Token::TemplateHead(_, _) => {
                    return Err(self.error("Tagged template cannot be used in an optional chain"));
                }
                _ => break,
            }
        }
        Ok(Expression::OptionalChain(
            ExprBox::new(expr),
            ExprBox::new(prop),
        ))
    }
}
