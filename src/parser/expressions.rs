use super::*;

impl<'a> Parser<'a> {
    pub fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        let expr = self.parse_assignment_expression()?;
        if self.current == Token::Comma {
            let mut exprs = vec![expr];
            while self.current == Token::Comma {
                self.advance()?;
                exprs.push(self.parse_assignment_expression()?);
            }
            Ok(Expression::Sequence(exprs))
        } else {
            Ok(expr)
        }
    }

    fn parse_yield_expression(&mut self) -> Result<Expression, ParseError> {
        self.advance()?;
        if self.prev_line_terminator
            || matches!(
                self.current,
                Token::RightBrace
                    | Token::Semicolon
                    | Token::RightParen
                    | Token::RightBracket
                    | Token::Colon
                    | Token::Comma
                    | Token::Eof
            )
        {
            return Ok(Expression::Yield(None, false));
        }
        let delegate = if self.current == Token::Star {
            self.advance()?;
            true
        } else {
            false
        };
        let expr = self.parse_assignment_expression()?;
        Ok(Expression::Yield(Some(Box::new(expr)), delegate))
    }

    fn is_simple_assignment_target(expr: &Expression) -> bool {
        matches!(expr, Expression::Identifier(_) | Expression::Member(_, _))
    }

    fn validate_assignment_target(
        &self,
        expr: &Expression,
        simple_only: bool,
    ) -> Result<(), ParseError> {
        if !simple_only && matches!(expr, Expression::Array(_) | Expression::Object(_)) {
            return Ok(());
        }
        if Self::is_simple_assignment_target(expr) {
            if self.strict
                && let Expression::Identifier(name) = expr
                && (name == "eval" || name == "arguments")
            {
                return Err(self.error("Assignment to 'eval' or 'arguments' in strict mode"));
            }
            return Ok(());
        }
        Err(self.error("Invalid left-hand side in assignment"))
    }

    pub(super) fn parse_assignment_expression(&mut self) -> Result<Expression, ParseError> {
        // YieldExpression in generator context
        if self.in_generator && self.current == Token::Keyword(Keyword::Yield) {
            if self.in_formal_parameters {
                return Err(self.error(
                    "Yield expression is not allowed in formal parameters of a generator function",
                ));
            }
            return self.parse_yield_expression();
        }

        let left = self.parse_conditional_expression()?;

        let op = match &self.current {
            Token::Assign => Some(AssignOp::Assign),
            Token::PlusAssign => Some(AssignOp::AddAssign),
            Token::MinusAssign => Some(AssignOp::SubAssign),
            Token::StarAssign => Some(AssignOp::MulAssign),
            Token::SlashAssign => Some(AssignOp::DivAssign),
            Token::PercentAssign => Some(AssignOp::ModAssign),
            Token::ExponentAssign => Some(AssignOp::ExpAssign),
            Token::LeftShiftAssign => Some(AssignOp::LShiftAssign),
            Token::RightShiftAssign => Some(AssignOp::RShiftAssign),
            Token::UnsignedRightShiftAssign => Some(AssignOp::URShiftAssign),
            Token::AmpersandAssign => Some(AssignOp::BitAndAssign),
            Token::PipeAssign => Some(AssignOp::BitOrAssign),
            Token::CaretAssign => Some(AssignOp::BitXorAssign),
            Token::LogicalAndAssign => Some(AssignOp::LogicalAndAssign),
            Token::LogicalOrAssign => Some(AssignOp::LogicalOrAssign),
            Token::NullishAssign => Some(AssignOp::NullishAssign),
            _ => None,
        };

        if let Some(op) = op {
            let simple_only = op != AssignOp::Assign;
            self.validate_assignment_target(&left, simple_only)?;
            self.advance()?;
            let right = self.parse_assignment_expression()?;
            Ok(Expression::Assign(op, Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }

    fn parse_conditional_expression(&mut self) -> Result<Expression, ParseError> {
        let expr = self.parse_nullish_coalescing()?;
        if self.current == Token::Question {
            self.advance()?;
            // ConditionalExpression[In]: consequent is AssignmentExpression[+In]
            let saved_no_in = self.no_in;
            self.no_in = false;
            let consequent = self.parse_assignment_expression()?;
            self.no_in = saved_no_in;
            self.eat(&Token::Colon)?;
            // alternate is AssignmentExpression[?In]
            let alternate = self.parse_assignment_expression()?;
            Ok(Expression::Conditional(
                Box::new(expr),
                Box::new(consequent),
                Box::new(alternate),
            ))
        } else {
            Ok(expr)
        }
    }

    fn parse_nullish_coalescing(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_logical_or()?;
        while self.current == Token::NullishCoalescing {
            self.advance()?;
            let right = self.parse_logical_or()?;
            left = Expression::Logical(
                LogicalOp::NullishCoalescing,
                Box::new(left),
                Box::new(right),
            );
        }
        Ok(left)
    }

    fn parse_logical_or(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_logical_and()?;
        while self.current == Token::LogicalOr {
            self.advance()?;
            let right = self.parse_logical_and()?;
            left = Expression::Logical(LogicalOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_bitwise_or()?;
        while self.current == Token::LogicalAnd {
            self.advance()?;
            let right = self.parse_bitwise_or()?;
            left = Expression::Logical(LogicalOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_bitwise_or(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_bitwise_xor()?;
        while self.current == Token::Pipe {
            self.advance()?;
            let right = self.parse_bitwise_xor()?;
            left = Expression::Binary(BinaryOp::BitOr, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_bitwise_xor(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_bitwise_and()?;
        while self.current == Token::Caret {
            self.advance()?;
            let right = self.parse_bitwise_and()?;
            left = Expression::Binary(BinaryOp::BitXor, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_bitwise_and(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_equality()?;
        while self.current == Token::Ampersand {
            self.advance()?;
            let right = self.parse_equality()?;
            left = Expression::Binary(BinaryOp::BitAnd, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_relational()?;
        loop {
            let op = match &self.current {
                Token::Equal => BinaryOp::Eq,
                Token::NotEqual => BinaryOp::NotEq,
                Token::StrictEqual => BinaryOp::StrictEq,
                Token::StrictNotEqual => BinaryOp::StrictNotEq,
                _ => break,
            };
            self.advance()?;
            let right = self.parse_relational()?;
            left = Expression::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<Expression, ParseError> {
        let mut left = if let Token::PrivateName(name) = &self.current {
            let name = name.clone();
            self.advance()?;
            if matches!(self.current, Token::Keyword(Keyword::In)) {
                Expression::PrivateIdentifier(name)
            } else {
                return Err(ParseError {
                    message: format!("Unexpected token #{name}"),
                });
            }
        } else {
            self.parse_shift()?
        };
        loop {
            let op = match &self.current {
                Token::LessThan => BinaryOp::Lt,
                Token::GreaterThan => BinaryOp::Gt,
                Token::LessThanEqual => BinaryOp::LtEq,
                Token::GreaterThanEqual => BinaryOp::GtEq,
                Token::Keyword(Keyword::Instanceof) => BinaryOp::Instanceof,
                Token::Keyword(Keyword::In) if !self.no_in => BinaryOp::In,
                _ => break,
            };
            self.advance()?;
            let right = self.parse_shift()?;
            left = Expression::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match &self.current {
                Token::LeftShift => BinaryOp::LShift,
                Token::RightShift => BinaryOp::RShift,
                Token::UnsignedRightShift => BinaryOp::URShift,
                _ => break,
            };
            self.advance()?;
            let right = self.parse_additive()?;
            left = Expression::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match &self.current {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance()?;
            let right = self.parse_multiplicative()?;
            left = Expression::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_exponentiation()?;
        loop {
            let op = match &self.current {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Mod,
                _ => break,
            };
            self.advance()?;
            let right = self.parse_exponentiation()?;
            left = Expression::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_exponentiation(&mut self) -> Result<Expression, ParseError> {
        let base = self.parse_unary()?;
        if self.current == Token::Exponent {
            self.advance()?;
            let exp = self.parse_exponentiation()?; // right-associative
            Ok(Expression::Binary(
                BinaryOp::Exp,
                Box::new(base),
                Box::new(exp),
            ))
        } else {
            Ok(base)
        }
    }

    fn parse_unary(&mut self) -> Result<Expression, ParseError> {
        match &self.current {
            Token::Keyword(Keyword::Delete) => {
                self.advance()?;
                let expr = self.parse_unary()?;
                if self.strict && matches!(&expr, Expression::Identifier(_)) {
                    return Err(self.error("Delete of an unqualified identifier in strict mode"));
                }
                if matches!(&expr, Expression::Member(_, MemberProperty::Private(_))) {
                    return Err(self
                        .error("Applying the 'delete' operator to a private name is not allowed"));
                }
                Ok(Expression::Delete(Box::new(expr)))
            }
            Token::Keyword(Keyword::Void) => {
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Void(Box::new(expr)))
            }
            Token::Keyword(Keyword::Typeof) => {
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Typeof(Box::new(expr)))
            }
            Token::Plus => {
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Unary(UnaryOp::Plus, Box::new(expr)))
            }
            Token::Minus => {
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Unary(UnaryOp::Minus, Box::new(expr)))
            }
            Token::Tilde => {
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Unary(UnaryOp::BitNot, Box::new(expr)))
            }
            Token::Bang => {
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Unary(UnaryOp::Not, Box::new(expr)))
            }
            Token::Increment | Token::Decrement => {
                let op = if self.current == Token::Increment {
                    UpdateOp::Increment
                } else {
                    UpdateOp::Decrement
                };
                self.advance()?;
                let expr = self.parse_unary()?;
                self.validate_assignment_target(&expr, true)?;
                Ok(Expression::Update(op, true, Box::new(expr)))
            }
            Token::Keyword(Keyword::Await)
                if self.in_async || (self.is_module && self.in_function == 0) =>
            {
                if self.in_formal_parameters {
                    return Err(self.error(
                        "Await expression is not allowed in formal parameters of an async function",
                    ));
                }
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Await(Box::new(expr)))
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expression, ParseError> {
        let expr = self.parse_left_hand_side_expression()?;
        if !self.prev_line_terminator {
            let op = match self.current {
                Token::Increment => Some(UpdateOp::Increment),
                Token::Decrement => Some(UpdateOp::Decrement),
                _ => None,
            };
            if let Some(op) = op {
                self.validate_assignment_target(&expr, true)?;
                self.advance()?;
                return Ok(Expression::Update(op, false, Box::new(expr)));
            }
        }
        Ok(expr)
    }

    fn parse_dot_member_property(&mut self) -> Result<MemberProperty, ParseError> {
        match &self.current {
            Token::PrivateName(name) => {
                let name = name.clone();
                self.advance()?;
                Ok(MemberProperty::Private(name))
            }
            Token::Identifier(n) | Token::IdentifierWithEscape(n) => {
                let name = n.clone();
                self.advance()?;
                Ok(MemberProperty::Dot(name))
            }
            Token::Keyword(kw) => {
                let name = kw.to_string();
                self.advance()?;
                Ok(MemberProperty::Dot(name))
            }
            Token::BooleanLiteral(b) => {
                let name = if *b { "true" } else { "false" }.to_string();
                self.advance()?;
                Ok(MemberProperty::Dot(name))
            }
            Token::NullLiteral => {
                self.advance()?;
                Ok(MemberProperty::Dot("null".to_string()))
            }
            _ => Err(self.error("Expected identifier after '.'")),
        }
    }

    pub(super) fn parse_left_hand_side_expression(&mut self) -> Result<Expression, ParseError> {
        let mut expr = if self.current == Token::Keyword(Keyword::New) {
            self.parse_new_expression()?
        } else {
            self.parse_primary_expression()?
        };

        loop {
            match &self.current {
                Token::Dot => {
                    self.advance()?;
                    let prop = self.parse_dot_member_property()?;
                    expr = Expression::Member(Box::new(expr), prop);
                }
                Token::LeftBracket => {
                    self.advance()?;
                    let prop = self.parse_expression()?;
                    self.eat(&Token::RightBracket)?;
                    expr = Expression::Member(
                        Box::new(expr),
                        MemberProperty::Computed(Box::new(prop)),
                    );
                }
                Token::LeftParen => {
                    let args = self.parse_arguments()?;
                    expr = Expression::Call(Box::new(expr), args);
                }
                Token::NoSubstitutionTemplate(_, _) | Token::TemplateHead(_, _) => {
                    let tmpl = self.parse_template_literal_expr(true)?;
                    expr = Expression::TaggedTemplate(Box::new(expr), tmpl);
                }
                Token::OptionalChain => {
                    self.advance()?;
                    let mut prop = if self.current == Token::LeftParen {
                        let args = self.parse_arguments()?;
                        Expression::Call(Box::new(Expression::Identifier("".into())), args)
                    } else if self.current == Token::LeftBracket {
                        self.advance()?;
                        let p = self.parse_expression()?;
                        self.eat(&Token::RightBracket)?;
                        Expression::Member(
                            Box::new(Expression::Identifier("".into())),
                            MemberProperty::Computed(Box::new(p)),
                        )
                    } else if let Token::PrivateName(name) = &self.current {
                        let name = name.clone();
                        self.advance()?;
                        Expression::Member(
                            Box::new(Expression::Identifier("".into())),
                            MemberProperty::Private(name),
                        )
                    } else {
                        let name = match &self.current {
                            Token::Identifier(n) | Token::IdentifierWithEscape(n) => n.clone(),
                            _ => return Err(self.error("Expected property after '?.'")),
                        };
                        self.advance()?;
                        Expression::Identifier(name)
                    };
                    // Continue consuming .prop, [expr], () as part of the same optional chain
                    loop {
                        match &self.current {
                            Token::Dot => {
                                self.advance()?;
                                let mp = self.parse_dot_member_property()?;
                                prop = Expression::Member(Box::new(prop), mp);
                            }
                            Token::LeftBracket => {
                                self.advance()?;
                                let p = self.parse_expression()?;
                                self.eat(&Token::RightBracket)?;
                                prop = Expression::Member(
                                    Box::new(prop),
                                    MemberProperty::Computed(Box::new(p)),
                                );
                            }
                            Token::LeftParen => {
                                let args = self.parse_arguments()?;
                                prop = Expression::Call(Box::new(prop), args);
                            }
                            Token::NoSubstitutionTemplate(_, _)
                            | Token::TemplateHead(_, _) => {
                                let tmpl = self.parse_template_literal_expr(true)?;
                                prop = Expression::TaggedTemplate(Box::new(prop), tmpl);
                            }
                            _ => break,
                        }
                    }
                    expr = Expression::OptionalChain(Box::new(expr), Box::new(prop));
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_new_expression(&mut self) -> Result<Expression, ParseError> {
        self.advance()?; // new
        if self.current == Token::Dot {
            self.advance()?; // .
            let is_target = match &self.current {
                Token::Identifier(n) | Token::IdentifierWithEscape(n) => n == "target",
                _ => false,
            };
            if is_target {
                if self.in_function == 0 {
                    return Err(self.error("new.target expression is not allowed here"));
                }
                self.advance()?; // target
                return Ok(Expression::NewTarget);
            }
            return Err(self.error("Expected 'target' after 'new.'"));
        }
        if self.current == Token::Keyword(Keyword::New) {
            let inner = self.parse_new_expression()?;
            return Ok(Expression::New(Box::new(inner), Vec::new()));
        }
        // `new import(...)` is a syntax error - import() is a CallExpression, not constructable
        if self.current == Token::Keyword(Keyword::Import) {
            return Err(self.error("Cannot use 'new' with import()"));
        }
        let mut callee = self.parse_primary_expression()?;
        loop {
            match &self.current {
                Token::Dot => {
                    self.advance()?;
                    let prop = self.parse_dot_member_property()?;
                    callee = Expression::Member(Box::new(callee), prop);
                }
                Token::LeftBracket => {
                    self.advance()?;
                    let prop = self.parse_expression()?;
                    self.eat(&Token::RightBracket)?;
                    callee = Expression::Member(
                        Box::new(callee),
                        MemberProperty::Computed(Box::new(prop)),
                    );
                }
                _ => break,
            }
        }
        let args = if self.current == Token::LeftParen {
            self.parse_arguments()?
        } else {
            Vec::new()
        };
        Ok(Expression::New(Box::new(callee), args))
    }

    fn parse_arguments(&mut self) -> Result<Vec<Expression>, ParseError> {
        self.eat(&Token::LeftParen)?;
        let mut args = Vec::new();
        while self.current != Token::RightParen {
            if self.current == Token::Ellipsis {
                self.advance()?;
                let expr = self.parse_assignment_expression()?;
                args.push(Expression::Spread(Box::new(expr)));
            } else {
                args.push(self.parse_assignment_expression()?);
            }
            if self.current == Token::Comma {
                self.advance()?;
            }
        }
        self.eat(&Token::RightParen)?;
        Ok(args)
    }

    fn parse_primary_expression(&mut self) -> Result<Expression, ParseError> {
        match &self.current {
            Token::Keyword(Keyword::This) => {
                self.advance()?;
                Ok(Expression::This)
            }
            Token::Keyword(Keyword::Super) => {
                self.advance()?;
                let is_call = self.current == Token::LeftParen;
                let is_property = self.current == Token::Dot || self.current == Token::LeftBracket;
                let allowed = (is_call && self.allow_super_call)
                    || (is_property && self.allow_super_property);
                if !allowed {
                    return Err(self.error("'super' keyword unexpected here"));
                }
                Ok(Expression::Super)
            }
            Token::Keyword(Keyword::Yield) if !self.in_generator && !self.strict => {
                self.advance()?;
                Ok(Expression::Identifier("yield".to_string()))
            }
            Token::Keyword(Keyword::Await) if !self.in_async => {
                self.advance()?;
                Ok(Expression::Identifier("await".to_string()))
            }
            Token::Keyword(Keyword::Let) if !self.strict => {
                self.advance()?;
                Ok(Expression::Identifier("let".to_string()))
            }
            Token::Keyword(Keyword::Import) => {
                self.advance()?;
                if self.current == Token::Dot {
                    // import.meta
                    self.advance()?;
                    match &self.current {
                        Token::Identifier(name) if name == "meta" => {
                            if !self.is_module {
                                return Err(self.error("import.meta is only valid in module code"));
                            }
                            self.advance()?;
                            Ok(Expression::ImportMeta)
                        }
                        _ => Err(self.error("Expected 'meta' after 'import.'")),
                    }
                } else if self.current == Token::LeftParen {
                    // import(source) - dynamic import
                    self.advance()?;
                    let source = self.parse_assignment_expression()?;
                    self.eat(&Token::RightParen)?;
                    Ok(Expression::Import(Box::new(source)))
                } else {
                    Err(self.error("Unexpected 'import'"))
                }
            }
            Token::Keyword(Keyword::Async) => {
                let source_start = self.current_token_start;
                self.advance()?;
                if self.current == Token::Keyword(Keyword::Function) && !self.prev_line_terminator {
                    return self.parse_async_function_expression(source_start);
                }
                if !self.prev_line_terminator {
                    if let Some(name) = self.current_identifier_name() {
                        let name = name.clone();
                        self.advance()?;
                        if self.current == Token::Arrow && !self.prev_line_terminator {
                            self.check_strict_binding_identifier(&name)?;
                            self.advance()?;
                            let prev_async = self.in_async;
                            self.in_async = true;
                            let body = if self.current == Token::LeftBrace {
                                let (stmts, _) = self.parse_arrow_function_body(true)?;
                                ArrowBody::Block(stmts)
                            } else {
                                ArrowBody::Expression(Box::new(self.parse_assignment_expression()?))
                            };
                            self.in_async = prev_async;
                            let source_text = Some(self.source_since(source_start));
                            return Ok(Expression::ArrowFunction(ArrowFunction {
                                params: vec![Pattern::Identifier(name)],
                                body,
                                is_async: true,
                                source_text,
                            }));
                        }
                        // Not an arrow — push back and return "async" as identifier
                        self.push_back(self.current.clone(), self.prev_line_terminator);
                        self.current = Token::Identifier(name);
                        self.prev_line_terminator = false;
                        return Ok(Expression::Identifier("async".to_string()));
                    }
                    if self.current == Token::LeftParen {
                        return self.parse_async_arrow_params(source_start);
                    }
                }
                Ok(Expression::Identifier("async".to_string()))
            }
            Token::Identifier(name) => {
                let name = name.clone();
                let ident_start = self.current_token_start;
                if Self::is_reserved_identifier(&name, self.strict) {
                    return Err(self.error(format!("Unexpected reserved word '{name}'")));
                }
                self.check_strict_identifier(&name)?;
                self.advance()?;
                // Arrow function: (ident) => or ident =>
                if self.current == Token::Arrow && !self.prev_line_terminator {
                    self.check_strict_binding_identifier(&name)?;
                    self.advance()?;
                    let body = if self.current == Token::LeftBrace {
                        let (stmts, _) = self.parse_arrow_function_body(false)?;
                        ArrowBody::Block(stmts)
                    } else {
                        ArrowBody::Expression(Box::new(self.parse_assignment_expression()?))
                    };
                    let source_text = Some(self.source_since(ident_start));
                    return Ok(Expression::ArrowFunction(ArrowFunction {
                        params: vec![Pattern::Identifier(name)],
                        body,
                        is_async: false,
                        source_text,
                    }));
                }
                Ok(Expression::Identifier(name))
            }
            Token::IdentifierWithEscape(name) => {
                let name = name.clone();
                let ident_start = self.current_token_start;
                // Escaped reserved words are still reserved
                if Self::is_reserved_identifier(&name, self.strict) {
                    return Err(
                        self.error("Keyword must not contain escaped characters".to_string())
                    );
                }
                // Escaped "await" and "yield" cannot be used as identifiers
                if name == "await" || name == "yield" {
                    return Err(
                        self.error("Keyword must not contain escaped characters".to_string())
                    );
                }
                self.check_strict_identifier(&name)?;
                self.advance()?;
                // Arrow function with single escaped identifier
                if self.current == Token::Arrow && !self.prev_line_terminator {
                    self.advance()?;
                    let body = if self.current == Token::LeftBrace {
                        let (stmts, _) = self.parse_arrow_function_body(false)?;
                        ArrowBody::Block(stmts)
                    } else {
                        ArrowBody::Expression(Box::new(self.parse_assignment_expression()?))
                    };
                    let source_text = Some(self.source_since(ident_start));
                    return Ok(Expression::ArrowFunction(ArrowFunction {
                        params: vec![Pattern::Identifier(name)],
                        body,
                        is_async: false,
                        source_text,
                    }));
                }
                Ok(Expression::Identifier(name))
            }
            Token::NullLiteral => {
                self.advance()?;
                Ok(Expression::Literal(Literal::Null))
            }
            Token::BooleanLiteral(b) => {
                let b = *b;
                self.advance()?;
                Ok(Expression::Literal(Literal::Boolean(b)))
            }
            Token::NumericLiteral(n) => {
                let n = *n;
                self.advance()?;
                Ok(Expression::Literal(Literal::Number(n)))
            }
            Token::LegacyOctalLiteral(n) => {
                if self.strict {
                    return Err(self.error("Octal literals are not allowed in strict mode"));
                }
                let n = *n;
                self.advance()?;
                Ok(Expression::Literal(Literal::Number(n)))
            }
            Token::StringLiteral(s) => {
                let s = s.clone();
                self.last_string_literal_has_escape = self.lexer.last_string_has_escape;
                self.advance()?;
                Ok(Expression::Literal(Literal::String(s)))
            }
            Token::BigIntLiteral(s) => {
                let s = s.clone();
                self.advance()?;
                Ok(Expression::Literal(Literal::BigInt(s)))
            }
            Token::RegExpLiteral { pattern, flags } => {
                let p = pattern.clone();
                let f = flags.clone();
                self.advance()?;
                Ok(Expression::Literal(Literal::RegExp(p, f)))
            }
            Token::Slash | Token::SlashAssign => {
                // Re-lex as regex literal
                let prefix = if matches!(self.current, Token::SlashAssign) {
                    "="
                } else {
                    ""
                };
                let regex_tok = self.lexer.lex_regex()?;
                if let Token::RegExpLiteral { pattern, flags } = regex_tok {
                    let full_pattern = format!("{}{}", prefix, pattern);
                    self.current = self.lexer.next_token()?;
                    while self.current == Token::LineTerminator {
                        self.current = self.lexer.next_token()?;
                    }
                    Ok(Expression::Literal(Literal::RegExp(full_pattern, flags)))
                } else {
                    Err(ParseError {
                        message: "Expected regex literal".to_string(),
                    })
                }
            }
            Token::LeftParen => {
                let paren_start = self.current_token_start;
                self.advance()?;
                let saved_no_in = self.no_in;
                self.no_in = false;
                // Could be: parenthesized expression, arrow params, or empty arrow ()=>
                if self.current == Token::RightParen {
                    // () => ...
                    self.advance()?;
                    if self.current == Token::Arrow {
                        self.advance()?;
                        let body = if self.current == Token::LeftBrace {
                            ArrowBody::Block(self.parse_arrow_function_body(false)?.0)
                        } else {
                            ArrowBody::Expression(Box::new(self.parse_assignment_expression()?))
                        };
                        let source_text = Some(self.source_since(paren_start));
                        return Ok(Expression::ArrowFunction(ArrowFunction {
                            params: Vec::new(),
                            body,
                            is_async: false,
                            source_text,
                        }));
                    }
                    return Err(self.error("Unexpected token )"));
                }
                // Check for rest param: (...a) =>
                if self.current == Token::Ellipsis {
                    // Arrow function with rest
                    self.advance()?;
                    let pat = self.parse_binding_pattern()?;
                    let params = vec![Pattern::Rest(Box::new(pat))];
                    self.eat(&Token::RightParen)?;
                    self.eat(&Token::Arrow)?;
                    let body = if self.current == Token::LeftBrace {
                        ArrowBody::Block(self.parse_arrow_function_body(false)?.0)
                    } else {
                        ArrowBody::Expression(Box::new(self.parse_assignment_expression()?))
                    };
                    let source_text = Some(self.source_since(paren_start));
                    return Ok(Expression::ArrowFunction(ArrowFunction {
                        params,
                        body,
                        is_async: false,
                        source_text,
                    }));
                }
                let expr = self.parse_assignment_expression()?;
                if self.current == Token::Comma || self.current == Token::RightParen {
                    let mut exprs = vec![expr];
                    while self.current == Token::Comma {
                        self.advance()?;
                        if self.current == Token::Ellipsis {
                            self.advance()?;
                            let pat = self.parse_binding_pattern()?;
                            exprs.push(Expression::Spread(Box::new(pattern_to_expr(pat))));
                            break;
                        }
                        if self.current == Token::RightParen {
                            break;
                        }
                        exprs.push(self.parse_assignment_expression()?);
                    }
                    self.eat(&Token::RightParen)?;
                    self.no_in = saved_no_in;
                    if self.current == Token::Arrow && !self.prev_line_terminator {
                        self.advance()?;
                        let params: Vec<Pattern> = exprs
                            .into_iter()
                            .map(expr_to_pattern)
                            .collect::<Result<_, _>>()?;
                        if self.strict {
                            self.check_strict_params(&params)?;
                        }
                        self.check_duplicate_params_strict(&params)?;
                        let body = if self.current == Token::LeftBrace {
                            ArrowBody::Block(self.parse_arrow_function_body(false)?.0)
                        } else {
                            ArrowBody::Expression(Box::new(self.parse_assignment_expression()?))
                        };
                        let source_text = Some(self.source_since(paren_start));
                        return Ok(Expression::ArrowFunction(ArrowFunction {
                            params,
                            body,
                            is_async: false,
                            source_text,
                        }));
                    }
                    // Just a parenthesized expression
                    if exprs.len() == 1 {
                        let e = exprs.into_iter().next().unwrap();
                        if Self::has_cover_initialized_name(&e) {
                            return Err(self.error("Invalid shorthand property initializer"));
                        }
                        return Ok(e);
                    }
                    for e in &exprs {
                        if Self::has_cover_initialized_name(e) {
                            return Err(self.error("Invalid shorthand property initializer"));
                        }
                    }
                    return Ok(Expression::Sequence(exprs));
                }
                self.eat(&Token::RightParen)?;
                self.no_in = saved_no_in;
                if Self::has_cover_initialized_name(&expr) {
                    return Err(self.error("Invalid shorthand property initializer"));
                }
                Ok(expr)
            }
            Token::LeftBracket => self.parse_array_literal(),
            Token::LeftBrace => self.parse_object_literal(),
            Token::Keyword(Keyword::Function) => self.parse_function_expression(),
            Token::Keyword(Keyword::Class) => self.parse_class_expression(),
            Token::NoSubstitutionTemplate(_, _) | Token::TemplateHead(_, _) => {
                let tmpl = self.parse_template_literal_expr(false)?;
                Ok(Expression::Template(tmpl))
            }
            Token::Keyword(Keyword::Of) => {
                self.advance()?;
                Ok(Expression::Identifier("of".to_string()))
            }
            _ => Err(self.error(format!("Unexpected token: {:?}", self.current))),
        }
    }

    fn parse_array_literal(&mut self) -> Result<Expression, ParseError> {
        self.advance()?; // [
        let saved_no_in = self.no_in;
        self.no_in = false;
        let mut elements = Vec::new();
        while self.current != Token::RightBracket {
            if self.current == Token::Comma {
                elements.push(None);
                self.advance()?;
                continue;
            }
            if self.current == Token::Ellipsis {
                self.advance()?;
                let expr = self.parse_assignment_expression()?;
                elements.push(Some(Expression::Spread(Box::new(expr))));
            } else {
                elements.push(Some(self.parse_assignment_expression()?));
            }
            if self.current == Token::Comma {
                self.advance()?;
            }
        }
        self.eat(&Token::RightBracket)?;
        self.no_in = saved_no_in;
        Ok(Expression::Array(elements))
    }

    fn has_cover_initialized_name(expr: &Expression) -> bool {
        if let Expression::Object(props) = expr {
            props.iter().any(|p| {
                p.shorthand
                    && matches!(
                        &p.value,
                        Expression::Assign(AssignOp::Assign, left, _) if matches!(&**left, Expression::Identifier(_))
                    )
            })
        } else {
            false
        }
    }

    fn parse_object_literal(&mut self) -> Result<Expression, ParseError> {
        self.advance()?; // {
        let saved_no_in = self.no_in;
        self.no_in = false;
        let mut props = Vec::new();
        while self.current != Token::RightBrace {
            if self.current == Token::Ellipsis {
                self.advance()?;
                let expr = self.parse_assignment_expression()?;
                props.push(Property {
                    key: PropertyKey::Identifier("".into()),
                    value: Expression::Spread(Box::new(expr)),
                    kind: PropertyKind::Init,
                    computed: false,
                    shorthand: false,
                });
            } else {
                props.push(self.parse_object_property()?);
            }
            if self.current == Token::Comma {
                self.advance()?;
            }
        }
        self.eat(&Token::RightBrace)?;
        let mut has_proto = false;
        for prop in &props {
            if prop.computed || prop.shorthand || prop.kind != PropertyKind::Init {
                continue;
            }
            let is_proto = match &prop.key {
                PropertyKey::Identifier(n) => n == "__proto__",
                PropertyKey::String(s) => s == "__proto__",
                _ => false,
            };
            if is_proto {
                if matches!(&prop.value, Expression::Function(_)) {
                    continue;
                }
                if has_proto {
                    return Err(
                        self.error("Duplicate __proto__ fields are not allowed in object literals")
                    );
                }
                has_proto = true;
            }
        }
        self.no_in = saved_no_in;
        Ok(Expression::Object(props))
    }

    fn parse_object_property(&mut self) -> Result<Property, ParseError> {
        let method_source_start = self.current_token_start;
        // Check for async method: { async method() {} } or { async *method() {} }
        let is_async_prop = matches!(&self.current, Token::Identifier(n) | Token::IdentifierWithEscape(n) if n == "async")
            || matches!(&self.current, Token::Keyword(Keyword::Async));
        if is_async_prop {
            let saved_lt = self.prev_line_terminator;
            let saved = self.advance()?;
            if !self.prev_line_terminator {
                let is_generator = self.eat_star()?;
                let is_method = matches!(
                    &self.current,
                    Token::Identifier(_)
                        | Token::IdentifierWithEscape(_)
                        | Token::StringLiteral(_)
                        | Token::NumericLiteral(_)
                        | Token::LegacyOctalLiteral(_)
                        | Token::LeftBracket
                        | Token::Keyword(_)
                );
                if is_method || is_generator {
                    let (key, computed) = self.parse_property_name()?;
                    let prev_async = self.in_async;
                    let prev_generator = self.in_generator;
                    self.in_async = true;
                    if is_generator {
                        self.in_generator = true;
                    }
                    let params = self.parse_formal_parameters()?;
                    self.in_async = prev_async;
                    self.in_generator = prev_generator;
                    let (body, _) =
                        self.parse_function_body_inner(is_generator, true, true, false)?;
                    self.check_duplicate_params_strict(&params)?;
                    let source_text = Some(self.source_since(method_source_start));
                    return Ok(Property {
                        key,
                        value: Expression::Function(FunctionExpr {
                            name: None,
                            params,
                            body,
                            is_async: true,
                            is_generator,
                            source_text,
                        }),
                        kind: PropertyKind::Init,
                        computed,
                        shorthand: false,
                    });
                }
            }
            // Not an async method — push back and restore
            self.push_back(self.current.clone(), self.prev_line_terminator);
            self.current = saved;
            self.prev_line_terminator = saved_lt;
        }

        // Check for generator method: { *method() {} }
        if self.current == Token::Star {
            self.advance()?; // consume *
            let (key, computed) = self.parse_property_name()?;
            let prev_generator = self.in_generator;
            self.in_generator = true;
            let params = self.parse_formal_parameters()?;
            self.in_generator = prev_generator;
            let (body, _) = self.parse_function_body_inner(true, false, true, false)?;
            self.check_duplicate_params_strict(&params)?;
            let source_text = Some(self.source_since(method_source_start));
            return Ok(Property {
                key,
                value: Expression::Function(FunctionExpr {
                    name: None,
                    params,
                    body,
                    is_async: false,
                    is_generator: true,
                    source_text,
                }),
                kind: PropertyKind::Init,
                computed,
                shorthand: false,
            });
        }

        // Check for get/set accessor
        let get_or_set_name = match &self.current {
            Token::Identifier(n) | Token::IdentifierWithEscape(n) if n == "get" || n == "set" => {
                Some(n.clone())
            }
            _ => None,
        };
        if let Some(n) = get_or_set_name {
            let saved_kind = if n == "get" {
                PropertyKind::Get
            } else {
                PropertyKind::Set
            };
            let saved_lt = self.prev_line_terminator;
            let saved = self.advance()?; // consume get/set, current is now next token
            let is_accessor = matches!(
                &self.current,
                Token::Identifier(_)
                    | Token::IdentifierWithEscape(_)
                    | Token::StringLiteral(_)
                    | Token::NumericLiteral(_)
                    | Token::LegacyOctalLiteral(_)
                    | Token::LeftBracket
                    | Token::Keyword(_)
            );
            if is_accessor {
                let (key, computed) = self.parse_property_name()?;
                let params = self.parse_formal_parameters()?;
                let (body, body_strict) =
                    self.parse_function_body_inner(false, false, true, false)?;
                if body_strict && !Self::is_simple_parameter_list(&params) {
                    return Err(self.error(
                        "Illegal 'use strict' directive in function with non-simple parameter list",
                    ));
                }
                if body_strict || self.strict {
                    self.check_duplicate_params_strict(&params)?;
                }
                return Ok(Property {
                    key,
                    value: Expression::Function(FunctionExpr {
                        name: None,
                        params,
                        body,
                        is_async: false,
                        is_generator: false,
                        source_text: Some(self.source_since(method_source_start)),
                    }),
                    kind: saved_kind,
                    computed,
                    shorthand: false,
                });
            }
            // Not an accessor — push back current and restore get/set as current
            self.push_back(self.current.clone(), self.prev_line_terminator);
            self.current = saved;
            self.prev_line_terminator = saved_lt;
        }

        let (key, computed) = self.parse_property_name()?;

        // Shorthand: { x }
        if !computed
            && let PropertyKey::Identifier(ref name) = key
            && self.current != Token::Colon
            && self.current != Token::LeftParen
            && self.current != Token::Assign
        {
            // In strict mode, future reserved words cannot be IdentifierReferences
            if self.strict {
                let n = name.as_str();
                if matches!(
                    n,
                    "implements"
                        | "interface"
                        | "let"
                        | "package"
                        | "private"
                        | "protected"
                        | "public"
                        | "static"
                        | "yield"
                ) {
                    return Err(self.error(&format!("Unexpected strict mode reserved word '{n}'")));
                }
            }
            return Ok(Property {
                value: Expression::Identifier(name.clone()),
                key,
                kind: PropertyKind::Init,
                computed: false,
                shorthand: true,
            });
        }

        // CoverInitializedName: { x = defaultValue } (destructuring default)
        if !computed
            && let PropertyKey::Identifier(ref name) = key
            && self.current == Token::Assign
        {
            let ident = name.clone();
            if self.strict && (ident == "eval" || ident == "arguments") {
                return Err(self.error("Invalid destructuring assignment target"));
            }
            self.advance()?; // consume '='
            let default_value = self.parse_assignment_expression()?;
            return Ok(Property {
                key,
                value: Expression::Assign(
                    AssignOp::Assign,
                    Box::new(Expression::Identifier(ident)),
                    Box::new(default_value),
                ),
                kind: PropertyKind::Init,
                computed: false,
                shorthand: true,
            });
        }

        // Method: { foo() {} }
        if self.current == Token::LeftParen {
            let params = self.parse_formal_parameters()?;
            let (body, body_strict) = self.parse_function_body_inner(false, false, true, false)?;
            if body_strict && !Self::is_simple_parameter_list(&params) {
                return Err(self.error(
                    "Illegal 'use strict' directive in function with non-simple parameter list",
                ));
            }
            if body_strict {
                self.check_strict_params(&params)?;
            }
            if body_strict || self.strict {
                self.check_duplicate_params_strict(&params)?;
            }
            return Ok(Property {
                key,
                value: Expression::Function(FunctionExpr {
                    name: None,
                    params,
                    body,
                    is_async: false,
                    is_generator: false,
                    source_text: Some(self.source_since(method_source_start)),
                }),
                kind: PropertyKind::Init,
                computed,
                shorthand: false,
            });
        }

        // Regular: { key: value }
        self.eat(&Token::Colon)?;
        let value = self.parse_assignment_expression()?;
        Ok(Property {
            key,
            value,
            kind: PropertyKind::Init,
            computed,
            shorthand: false,
        })
    }

    fn parse_function_expression(&mut self) -> Result<Expression, ParseError> {
        let source_start = self.current_token_start;
        self.advance()?;
        let is_generator = self.eat_star()?;
        let name = if let Some(n) = self.current_identifier_name() {
            self.check_strict_binding_identifier(&n)?;
            self.advance()?;
            Some(n)
        } else {
            None
        };
        let prev_generator = self.in_generator;
        if is_generator {
            self.in_generator = true;
        }
        let params = self.parse_formal_parameters()?;
        self.in_generator = prev_generator;
        let (body, body_strict) = self.parse_function_body_with_context(is_generator, false)?;
        if body_strict && !Self::is_simple_parameter_list(&params) {
            return Err(self.error(
                "Illegal 'use strict' directive in function with non-simple parameter list",
            ));
        }
        if body_strict {
            self.check_strict_params(&params)?;
        }
        if body_strict || self.strict || is_generator || !Self::is_simple_parameter_list(&params) {
            self.check_duplicate_params_strict(&params)?;
        }
        let source_text = Some(self.source_since(source_start));
        Ok(Expression::Function(FunctionExpr {
            name,
            params,
            body,
            is_async: false,
            is_generator,
            source_text,
        }))
    }

    fn parse_async_function_expression(
        &mut self,
        source_start: usize,
    ) -> Result<Expression, ParseError> {
        self.advance()?; // consume 'function'
        let is_generator = self.eat_star()?;
        let name = if matches!(&self.current, Token::Keyword(Keyword::Await)) {
            return Err(self.error("'await' is not allowed as a function name in async functions"));
        } else if let Some(n) = self.current_identifier_name() {
            self.check_strict_binding_identifier(&n)?;
            self.advance()?;
            Some(n)
        } else {
            None
        };
        let prev_generator = self.in_generator;
        let prev_async = self.in_async;
        if is_generator {
            self.in_generator = true;
        }
        self.in_async = true;
        let params = self.parse_formal_parameters()?;
        self.in_generator = prev_generator;
        self.in_async = prev_async;
        let (body, body_strict) = self.parse_function_body_with_context(is_generator, true)?;
        if body_strict && !Self::is_simple_parameter_list(&params) {
            return Err(self.error(
                "Illegal 'use strict' directive in function with non-simple parameter list",
            ));
        }
        if body_strict {
            self.check_strict_params(&params)?;
        }
        self.check_duplicate_params_strict(&params)?;
        let source_text = Some(self.source_since(source_start));
        Ok(Expression::Function(FunctionExpr {
            name,
            params,
            body,
            is_async: true,
            is_generator,
            source_text,
        }))
    }

    fn parse_async_arrow_params(&mut self, source_start: usize) -> Result<Expression, ParseError> {
        // Current token is '(' — parse params, check for '=>'
        self.advance()?; // consume '('
        if self.current == Token::RightParen {
            self.advance()?;
            if self.current == Token::Arrow && !self.prev_line_terminator {
                self.advance()?;
                let prev_async = self.in_async;
                self.in_async = true;
                let body = if self.current == Token::LeftBrace {
                    ArrowBody::Block(self.parse_arrow_function_body(true)?.0)
                } else {
                    ArrowBody::Expression(Box::new(self.parse_assignment_expression()?))
                };
                self.in_async = prev_async;
                let source_text = Some(self.source_since(source_start));
                return Ok(Expression::ArrowFunction(ArrowFunction {
                    params: Vec::new(),
                    body,
                    is_async: true,
                    source_text,
                }));
            }
            // async() — function call on 'async' identifier
            return Ok(Expression::Call(
                Box::new(Expression::Identifier("async".to_string())),
                Vec::new(),
            ));
        }
        if self.current == Token::Ellipsis {
            // async(...rest) =>
            self.advance()?;
            let pat = self.parse_binding_pattern()?;
            let params = vec![Pattern::Rest(Box::new(pat))];
            self.eat(&Token::RightParen)?;
            self.eat(&Token::Arrow)?;
            let prev_async = self.in_async;
            self.in_async = true;
            let body = if self.current == Token::LeftBrace {
                ArrowBody::Block(self.parse_arrow_function_body(true)?.0)
            } else {
                ArrowBody::Expression(Box::new(self.parse_assignment_expression()?))
            };
            self.in_async = prev_async;
            let source_text = Some(self.source_since(source_start));
            return Ok(Expression::ArrowFunction(ArrowFunction {
                params,
                body,
                is_async: true,
                source_text,
            }));
        }
        let expr = self.parse_assignment_expression()?;
        if self.current == Token::Comma || self.current == Token::RightParen {
            let mut exprs = vec![expr];
            while self.current == Token::Comma {
                self.advance()?;
                if self.current == Token::Ellipsis {
                    self.advance()?;
                    let pat = self.parse_binding_pattern()?;
                    exprs.push(Expression::Spread(Box::new(pattern_to_expr(pat))));
                    break;
                }
                if self.current == Token::RightParen {
                    break;
                }
                exprs.push(self.parse_assignment_expression()?);
            }
            self.eat(&Token::RightParen)?;
            if self.current == Token::Arrow && !self.prev_line_terminator {
                self.advance()?;
                let params: Vec<Pattern> = exprs
                    .into_iter()
                    .map(expr_to_pattern)
                    .collect::<Result<_, _>>()?;
                if self.strict {
                    self.check_strict_params(&params)?;
                }
                self.check_duplicate_params_strict(&params)?;
                let prev_async = self.in_async;
                self.in_async = true;
                let body = if self.current == Token::LeftBrace {
                    ArrowBody::Block(self.parse_arrow_function_body(true)?.0)
                } else {
                    ArrowBody::Expression(Box::new(self.parse_assignment_expression()?))
                };
                self.in_async = prev_async;
                let source_text = Some(self.source_since(source_start));
                return Ok(Expression::ArrowFunction(ArrowFunction {
                    params,
                    body,
                    is_async: true,
                    source_text,
                }));
            }
            // Not an arrow — it's async(args) function call
            return Ok(Expression::Call(
                Box::new(Expression::Identifier("async".to_string())),
                exprs,
            ));
        }
        self.eat(&Token::RightParen)?;
        // async(expr) — function call
        Ok(Expression::Call(
            Box::new(Expression::Identifier("async".to_string())),
            vec![expr],
        ))
    }

    fn parse_class_expression(&mut self) -> Result<Expression, ParseError> {
        let source_start = self.current_token_start;
        self.advance()?; // class
        // Class definitions are strict mode code — set strict before parsing name
        let prev_strict = self.strict;
        self.set_strict(true);
        let name = if self.current != Token::Keyword(Keyword::Extends)
            && self.current != Token::LeftBrace
        {
            if let Some(n) = self.current_identifier_name() {
                self.advance()?;
                Some(n)
            } else {
                None
            }
        } else {
            None
        };
        let super_class = if self.current == Token::Keyword(Keyword::Extends) {
            self.advance()?;
            Some(Box::new(self.parse_left_hand_side_expression()?))
        } else {
            None
        };
        self.set_strict(prev_strict);
        let body = self.parse_class_body()?;
        let source_text = Some(self.source_since(source_start));
        Ok(Expression::Class(ClassExpr {
            name,
            super_class,
            body,
            source_text,
        }))
    }

    fn parse_template_literal_expr(&mut self, tagged: bool) -> Result<TemplateLiteral, ParseError> {
        match &self.current {
            Token::NoSubstitutionTemplate(cooked, raw) => {
                let cooked = cooked.clone();
                let raw = raw.clone();
                if !tagged {
                    cooked
                        .as_ref()
                        .ok_or_else(|| self.error("Invalid escape sequence in template literal"))?;
                }
                self.advance()?;
                Ok(TemplateLiteral {
                    quasis: vec![cooked],
                    raw_quasis: vec![raw],
                    expressions: Vec::new(),
                })
            }
            Token::TemplateHead(cooked, raw) => {
                let cooked = cooked.clone();
                let raw = raw.clone();
                if !tagged {
                    cooked
                        .as_ref()
                        .ok_or_else(|| self.error("Invalid escape sequence in template literal"))?;
                }
                let mut quasis = vec![cooked];
                let mut raw_quasis = vec![raw];
                let mut expressions = Vec::new();
                self.advance()?;
                loop {
                    expressions.push(self.parse_expression()?);
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
                    quasis,
                    raw_quasis,
                    expressions,
                })
            }
            _ => Err(self.error("Expected template literal")),
        }
    }
}
