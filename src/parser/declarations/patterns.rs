use super::v8_messages::patterns::Site;
use super::*;

impl<'a> Parser<'a> {
    pub(in crate::parser) fn parse_binding_pattern(&mut self) -> Result<Pattern, ParseError> {
        if let Some(name) = self.current_identifier_name() {
            self.check_strict_binding_identifier(&name)?;
            self.advance()?;
            return Ok(Pattern::Identifier(name));
        }
        match &self.current {
            Token::LeftBracket => {
                v8_position::instruction();
                self.parse_array_pattern()
            }
            Token::LeftBrace => {
                v8_position::instruction();
                self.parse_object_pattern()
            }
            _ => Err(self.unexpected_token("Expected binding pattern, got ")),
        }
    }

    fn parse_array_pattern(&mut self) -> Result<Pattern, ParseError> {
        let at = self.op_pos();
        self.eat(&Token::LeftBracket)?;
        let mut elements = Vec::new();
        while self.current != Token::RightBracket {
            if self.current == Token::Comma {
                elements.push(None);
                self.advance()?;
                continue;
            }
            if self.current == Token::Ellipsis {
                self.advance()?;
                let rest = self.rest_element(false)?;
                elements.push(Some(ArrayPatternElement::Rest(rest)));
                break;
            }
            let pat = self.pattern_element(Site::Element)?;
            elements.push(Some(ArrayPatternElement::Pattern(pat)));
            if self.current != Token::Comma {
                break;
            }
            self.advance()?;
        }
        self.eat(&Token::RightBracket)?;
        Ok(Pattern::Array(PatternElems::at(elements, at)))
    }

    fn parse_object_pattern(&mut self) -> Result<Pattern, ParseError> {
        let mut at = self.op_pos();
        self.eat(&Token::LeftBrace)?;
        let mut props = Vec::new();
        while self.current != Token::RightBrace {
            let first = props.is_empty();
            if self.current == Token::Ellipsis {
                self.advance()?;
                if first {
                    at = self.op_pos();
                }
                // BindingRestProperty : ... BindingIdentifier
                if matches!(self.current, Token::LeftBracket | Token::LeftBrace) {
                    return Err(self
                        .error("`...` must be followed by an identifier in declaration contexts"));
                }
                let rest = self.rest_element(true)?;
                props.push(ObjectPatternProperty::Rest(rest));
                break;
            }
            let key_text = inferred_names::enabled().then(|| self.v8_unexpected());
            let key_at = self.op_pos();
            let key = self.parse_property_key_for_pattern()?;
            if self.current == Token::Colon {
                self.advance()?;
                if first && !matches!(key, PropertyKey::Computed(_)) {
                    at = self.op_pos();
                }
                let pat = self.pattern_element(Site::Element)?;
                props.push(ObjectPatternProperty::KeyValue(key, pat));
            } else {
                // Shorthand: { x } or { x = default }
                if first {
                    at = key_at;
                }
                let name = match &key {
                    PropertyKey::Identifier(n) => n.clone(),
                    _ => {
                        return Err(self.error(
                            key_text.unwrap_or("Expected identifier for shorthand pattern".into()),
                        ));
                    }
                };
                // `async` followed by something that looks like a method name/modifier
                if name == "async"
                    && !matches!(
                        self.current,
                        Token::Colon | Token::Assign | Token::Comma | Token::RightBrace
                    )
                {
                    return Err(self.error("Invalid destructuring assignment target"));
                }
                if Self::is_reserved_identifier(&name, false) {
                    return Err(self.error(format!("Unexpected token '{name}'")));
                }
                if self.in_static_block && name == "await" {
                    return Err(self.error(
                        "'await' is not allowed as a binding identifier in class static blocks",
                    ));
                }
                if (self.in_generator || self.strict) && name == "yield" {
                    return Err(self
                        .error("'yield' is not allowed as a binding identifier in this context"));
                }
                self.check_strict_binding_identifier(&name)?;
                if self.current == Token::Assign {
                    self.advance()?;
                    let default = self.parse_assignment_expression()?;
                    let pat =
                        Pattern::Assign(Box::new(Pattern::Identifier(name)), ExprBox::new(default));
                    props.push(ObjectPatternProperty::KeyValue(key, pat));
                } else {
                    props.push(ObjectPatternProperty::Shorthand(name));
                }
            }
            if self.current != Token::Comma {
                break;
            }
            self.advance()?;
        }
        self.eat(&Token::RightBrace)?;
        Ok(Pattern::Object(PatternProps::at(props, at)))
    }

    fn parse_property_key_for_pattern(&mut self) -> Result<PropertyKey, ParseError> {
        match &self.current {
            Token::Identifier(name) | Token::IdentifierWithEscape(name) => {
                let name = name.clone();
                self.advance()?;
                Ok(PropertyKey::Identifier(name))
            }
            Token::StringLiteral(s) => {
                let s = s.clone();
                self.advance()?;
                Ok(PropertyKey::String(s))
            }
            Token::NumericLiteral(n)
            | Token::LegacyOctalLiteral(n)
            | Token::NonOctalDecimalLiteral(n) => {
                if matches!(
                    &self.current,
                    Token::LegacyOctalLiteral(_) | Token::NonOctalDecimalLiteral(_)
                ) && self.strict
                {
                    return Err(self.error("Octal literals are not allowed in strict mode"));
                }
                let n = *n;
                self.advance()?;
                Ok(PropertyKey::Number(n))
            }
            Token::LeftBracket => {
                self.advance()?;
                let expr = self.parse_assignment_expression()?;
                self.eat(&Token::RightBracket)?;
                Ok(PropertyKey::Computed(ExprBox::new(expr)))
            }
            Token::Keyword(kw) => {
                let name = kw.to_string();
                self.advance()?;
                Ok(PropertyKey::Identifier(name))
            }
            Token::BooleanLiteral(b) => {
                let name = if *b { "true" } else { "false" }.to_string();
                self.advance()?;
                Ok(PropertyKey::Identifier(name))
            }
            Token::NullLiteral => {
                self.advance()?;
                Ok(PropertyKey::Identifier("null".to_string()))
            }
            Token::BigIntLiteral(s) => {
                let name = super::bigint_literal_to_decimal(s);
                self.advance()?;
                Ok(PropertyKey::String(name.encode_utf16().collect()))
            }
            _ => Err(self.error("Expected property name in object pattern")),
        }
    }
}
