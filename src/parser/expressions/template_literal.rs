use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_template_literal_expr(
        &mut self,
        tagged: bool,
    ) -> Result<TemplateLiteral, ParseError> {
        let start = self.current_token_start;
        match &self.current {
            Token::NoSubstitutionTemplate(cooked, raw) => {
                let cooked = cooked.clone();
                let raw = raw.clone();
                self.check_cooked(&cooked, tagged)?;
                self.advance()?;
                Ok(TemplateLiteral {
                    id: self.template_id(tagged, start),
                    quasis: vec![cooked],
                    raw_quasis: vec![raw],
                    raw_stand_ins: self.lone_surrogate_stand_ins,
                    expressions: Vec::new(),
                })
            }
            Token::TemplateHead(cooked, raw) => {
                let cooked = cooked.clone();
                let raw = raw.clone();
                self.check_cooked(&cooked, tagged)?;
                let mut quasis = vec![cooked];
                let mut raw_quasis = vec![raw];
                let mut expressions = Vec::new();
                self.advance()?;
                loop {
                    expressions.push(self.parse_expression()?);
                    if self.current != Token::RightBrace {
                        return Err(self.error("Expected '}' after template substitution"));
                    }
                    let tok = self.lexer.read_template_continuation()?;
                    match tok {
                        Token::TemplateTail(cooked, raw) => {
                            if !tagged {
                                cooked.as_ref().ok_or_else(|| {
                                    self.error("Invalid escape sequence in template literal")
                                })?;
                            }
                            quasis.push(cooked);
                            raw_quasis.push(raw);
                            self.advance()?;
                            break;
                        }
                        Token::TemplateMiddle(cooked, raw) => {
                            if !tagged {
                                cooked.as_ref().ok_or_else(|| {
                                    self.error("Invalid escape sequence in template literal")
                                })?;
                            }
                            quasis.push(cooked);
                            raw_quasis.push(raw);
                            self.advance()?;
                        }
                        _ => return Err(self.error("Expected template continuation")),
                    }
                }
                Ok(TemplateLiteral {
                    id: self.template_id(tagged, start),
                    quasis,
                    raw_quasis,
                    raw_stand_ins: self.lone_surrogate_stand_ins,
                    expressions,
                })
            }
            _ => Err(self.error("Expected template literal")),
        }
    }
}
