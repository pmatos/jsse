use super::*;

mod arguments;
mod async_body;
mod binary_ops;
mod regexp;
mod targets;

use regexp::validate_regexp_literal;

mod logical_ops;
mod new_expr;
mod optional_chain;
mod pattern_pos;
mod template_literal;
mod yield_expr;

impl<'a> Parser<'a> {
    pub(crate) fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        let expr = self.parse_assignment_expression()?;
        if self.current == Token::Comma {
            let mut exprs = vec![expr];
            while self.current == Token::Comma {
                self.advance()?;
                exprs.push(self.parse_comma_operand()?);
            }
            Ok(Expression::Sequence(exprs))
        } else {
            Ok(expr)
        }
    }

    pub(super) fn parse_assignment_expression(&mut self) -> Result<Expression, ParseError> {
        // Depth guard.
        self.enter_recursion()?;
        let cover = self.begin_cover_scope();
        let mut r = self.parse_assignment_expression_inner();
        if let Ok(e) = &r
            && let Err(err) = self.end_cover_scope(cover, e)
        {
            r = Err(err);
        }
        self.exit_recursion();
        r
    }

    fn parse_assignment_expression_inner(&mut self) -> Result<Expression, ParseError> {
        let saved_proto_dup = self.last_obj_had_proto_dup;
        self.last_obj_had_proto_dup = false;
        // YieldExpression in generator context
        if self.in_generator && self.current == Token::Keyword(Keyword::Yield) {
            if self.in_formal_parameters {
                return Err(self.error(
                    "Yield expression is not allowed in formal parameters of a generator function",
                ));
            }
            return self.parse_yield_expression();
        }

        let left_start = self.current_token_start;
        let before = v8_position::snapshot();
        let left = self.parse_conditional_expression()?;
        let arrow_ended = match self.arrow_held_token.take() {
            Some(held) => {
                self.current = held;
                true
            }
            None => false,
        };

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
        // An unparenthesized ArrowFunction cannot be the target of an operator.
        let op = op.filter(|_| !arrow_ended);

        if let Some(op) = op {
            let simple_only = op != AssignOp::Assign;
            let is_logical = matches!(
                op,
                AssignOp::LogicalAndAssign | AssignOp::LogicalOrAssign | AssignOp::NullishAssign
            );
            let lhs_parenthesized = self.last_expr_parenthesized;
            // Assignment target: duplicate __proto__ is allowed (destructuring)
            self.last_obj_had_proto_dup = saved_proto_dup;
            self.validate_assignment_target(&left, simple_only, !is_logical)?;
            let mut left = left;
            self.retarget_store_site(&mut left, op);
            v8_position::store_target(&left, op, before);
            self.claim_cover_for_pattern(op, &left);
            let at = self.assign_pos(&left, op, left_start);
            self.advance()?;
            let right_start = self.current_token_start;
            let right = self.parse_assignment_expression()?;
            v8_position::expression(at);
            self.infer_assigned_name(op, &left, &right, right_start);
            // When LHS is parenthesized, wrap in Sequence to prevent IsIdentifierRef
            let left = if lhs_parenthesized && matches!(left, Expression::Identifier(_, _)) {
                Expression::Sequence(vec![left])
            } else {
                left
            };
            Ok(Expression::Assign(
                op,
                ExprBox::new(left),
                ExprBox::new(right),
                at,
            ))
        } else {
            // Not a destructuring assignment — deferred __proto__ dup error
            if self.last_obj_had_proto_dup {
                self.last_obj_had_proto_dup = saved_proto_dup;
                return Err(
                    self.error("Duplicate __proto__ fields are not allowed in object literals")
                );
            }
            self.last_obj_had_proto_dup = saved_proto_dup;
            Ok(left)
        }
    }

    fn parse_conditional_expression(&mut self) -> Result<Expression, ParseError> {
        let start = v8_position::at(self.current_token_start);
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
            v8_position::node(start);
            Ok(Expression::Conditional(
                ExprBox::new(expr),
                ExprBox::new(consequent),
                ExprBox::new(alternate),
            ))
        } else {
            Ok(expr)
        }
    }

    fn parse_exponentiation(&mut self) -> Result<Expression, ParseError> {
        // Depth guard: `**` is right-associative and recurses here, bypassing
        // parse_assignment_expression, so it needs its own bound.
        self.enter_recursion()?;
        let r = self.parse_exponentiation_inner();
        self.exit_recursion();
        r
    }

    fn parse_exponentiation_inner(&mut self) -> Result<Expression, ParseError> {
        let start = self.current_token_start;
        let base = self.parse_unary()?;
        if self.current == Token::Exponent {
            let is_unary = matches!(
                &base,
                Expression::Unary(_, _)
                    | Expression::Typeof(_)
                    | Expression::Void(_)
                    | Expression::Delete(_)
                    | Expression::Await(..)
            );
            let is_parenthesized = self.source.as_bytes().get(start) == Some(&b'(');
            if is_unary && !is_parenthesized {
                return Err(self.error(
                    "Unary operator used immediately before exponentiation expression. Parenthesization is required",
                ));
            }
            let at = self.op_pos();
            self.advance()?;
            let exp = self.parse_exponentiation()?; // right-associative
            v8_position::expression(at);
            Ok(Expression::Binary(
                BinaryOp::Exp,
                ExprBox::new(base),
                ExprBox::new(exp),
                at,
            ))
        } else {
            Ok(base)
        }
    }

    fn parse_unary(&mut self) -> Result<Expression, ParseError> {
        // Prefix operators recurse here, bypassing
        // parse_assignment_expression, so they need their own guard.
        self.enter_recursion()?;
        let r = self.parse_unary_inner();
        self.exit_recursion();
        r
    }

    fn parse_unary_inner(&mut self) -> Result<Expression, ParseError> {
        match &self.current {
            Token::Keyword(Keyword::Delete) => {
                self.advance()?;
                let expr = self.parse_unary()?;
                if self.strict && matches!(&expr, Expression::Identifier(_, _)) {
                    return Err(self.error("Delete of an unqualified identifier in strict mode"));
                }
                if matches!(&expr, Expression::Member(_, MemberProperty::Private(_), _)) {
                    return Err(self
                        .error("Applying the 'delete' operator to a private name is not allowed"));
                }
                Ok(Expression::Delete(ExprBox::new(expr)))
            }
            Token::Keyword(Keyword::Void) => {
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Void(ExprBox::new(expr)))
            }
            Token::Keyword(Keyword::Typeof) => {
                self.advance()?;
                let before = v8_position::snapshot();
                let expr = self.parse_unary()?;
                if matches!(expr, Expression::Identifier(..)) {
                    v8_position::restore(before);
                    v8_position::instruction();
                }
                Ok(Expression::Typeof(ExprBox::new(expr)))
            }
            Token::Plus => {
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Unary(UnaryOp::Plus, ExprBox::new(expr)))
            }
            Token::Minus => {
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Unary(UnaryOp::Minus, ExprBox::new(expr)))
            }
            Token::Tilde => {
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Unary(UnaryOp::BitNot, ExprBox::new(expr)))
            }
            Token::Bang => {
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Unary(UnaryOp::Not, ExprBox::new(expr)))
            }
            Token::Increment | Token::Decrement => {
                let op = if self.current == Token::Increment {
                    UpdateOp::Increment
                } else {
                    UpdateOp::Decrement
                };
                self.advance()?;
                let at = self.op_pos();
                let expr = self.parse_unary()?;
                self.validate_update_target(&expr, true)?;
                Ok(Expression::Update(op, true, ExprBox::new(expr), at))
            }
            Token::Keyword(Keyword::Await) if self.reserved_await() => {
                Err(self.error("Unexpected reserved word"))
            }
            Token::Keyword(Keyword::Await)
                if self.in_async || (self.is_module && self.in_function == 0) =>
            {
                if self.in_formal_parameters {
                    return Err(self.error(
                        "Await expression is not allowed in formal parameters of an async function",
                    ));
                }
                let at = CallSiteId::at(self.current_token_start).1;
                v8_position::early_expression(at);
                self.advance()?;
                let expr = self.parse_unary()?;
                v8_position::expression(at);
                Ok(Expression::Await(ExprBox::new(expr), at))
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
                self.validate_update_target(&expr, false)?;
                let at = self.op_pos();
                self.advance()?;
                v8_position::expression(at);
                return Ok(Expression::Update(op, false, ExprBox::new(expr), at));
            }
        }
        Ok(expr)
    }

    fn parse_dot_member_property(&mut self) -> Result<MemberProperty, ParseError> {
        match &self.current {
            Token::PrivateName(name) => {
                let name = name.clone();
                self.use_private_name(&name)?;
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

        // §13.3: an unparenthesized ArrowFunction is a whole
        // AssignmentExpression: hide the next token from the operator levels
        // above (parse_assignment_expression restores it), so `/` after
        // `() => {}` and a newline lexes as a RegExp (§12.9.4).
        if matches!(expr, Expression::ArrowFunction(_)) && !self.last_expr_parenthesized {
            self.arrow_held_token = Some(std::mem::replace(&mut self.current, Token::Eof));
            return Ok(expr);
        }

        let mut called = false;
        loop {
            match &self.current {
                Token::Dot => {
                    let dot = self.current_token_start;
                    self.advance()?;
                    let at = if called {
                        dot
                    } else {
                        self.current_token_start
                    };
                    let prop = self.parse_dot_member_property()?;
                    if matches!(expr, Expression::Super)
                        && matches!(prop, MemberProperty::Private(_))
                    {
                        return Err(self.error("Private fields are not accessible on 'super'"));
                    }
                    let private = matches!(prop, MemberProperty::Private(_));
                    v8_position::expression(v8_position::at(if private { dot } else { at }));
                    expr = Expression::Member(ExprBox::new(expr), prop, PropSiteId::at(at));
                }
                Token::LeftBracket => {
                    let at = self.current_token_start;
                    self.advance()?;
                    let prop = self.parse_expression()?;
                    self.eat(&Token::RightBracket)?;
                    v8_position::expression(v8_position::at(at));
                    expr = Expression::Member(
                        ExprBox::new(expr),
                        MemberProperty::Computed(ExprBox::new(prop)),
                        PropSiteId::at(at),
                    );
                }
                Token::LeftParen => {
                    let site = self.call_site(&expr);
                    let args = self.parse_arguments()?;
                    v8_position::expression(site.1);
                    called = true;
                    expr = Expression::Call(ExprBox::new(expr), args, site);
                }
                Token::NoSubstitutionTemplate(_, _) | Token::TemplateHead(_, _) => {
                    v8_position::early_expression(v8_position::at(self.current_token_start));
                    let tmpl = self.parse_template_literal_expr(true)?;
                    expr = Expression::TaggedTemplate(ExprBox::new(expr), tmpl);
                }
                Token::OptionalChain => expr = self.parse_optional_chain(expr)?,
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary_expression(&mut self) -> Result<Expression, ParseError> {
        let tracked = self.track_primary();
        let result = self.parse_primary_expression_inner();
        if let Ok(expr) = &result {
            self.track_primary_end(tracked, expr);
        }
        // Only a `( ArrowFunction )` primary is genuinely parenthesized.
        let parenthesized_arrow = std::mem::take(&mut self.parenthesized_arrow);
        if matches!(result, Ok(Expression::ArrowFunction(_))) {
            self.last_expr_parenthesized = parenthesized_arrow;
        }
        result
    }

    fn parse_primary_expression_inner(&mut self) -> Result<Expression, ParseError> {
        self.last_expr_parenthesized = false;
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
                let ident_start = self.current_token_start;
                self.advance()?;
                if self.current == Token::Arrow && !self.prev_line_terminator {
                    self.advance()?;
                    let (body, body_is_strict) = if self.current == Token::LeftBrace {
                        let (stmts, strict) = self.parse_arrow_function_body(false)?;
                        (ArrowBody::Block(Body::new(stmts)), strict)
                    } else {
                        (
                            ArrowBody::Expression(Body::new(vec![self.arrow_return()?])),
                            false,
                        )
                    };
                    let source_text = self.source_since(ident_start);
                    return Ok(Expression::ArrowFunction(ArrowFunction {
                        params: vec![Pattern::Identifier("yield".to_string())],
                        body,
                        is_async: false,
                        source_text,
                        body_is_strict,
                        info: Default::default(),
                    }));
                }
                Ok(Expression::Identifier("yield".to_string(), 0))
            }
            Token::Keyword(Keyword::Await)
                if !self.in_async && !self.in_static_block && !self.is_module =>
            {
                let ident_start = self.current_token_start;
                self.advance()?;
                if self.current == Token::Arrow && !self.prev_line_terminator {
                    self.advance()?;
                    let (body, body_is_strict) = if self.current == Token::LeftBrace {
                        let (stmts, strict) = self.parse_arrow_function_body(false)?;
                        (ArrowBody::Block(Body::new(stmts)), strict)
                    } else {
                        (
                            ArrowBody::Expression(Body::new(vec![self.arrow_return()?])),
                            false,
                        )
                    };
                    let source_text = self.source_since(ident_start);
                    return Ok(Expression::ArrowFunction(ArrowFunction {
                        params: vec![Pattern::Identifier("await".to_string())],
                        body,
                        is_async: false,
                        source_text,
                        body_is_strict,
                        info: Default::default(),
                    }));
                }
                Ok(Expression::Identifier("await".to_string(), 0))
            }
            Token::Keyword(Keyword::Let) if !self.strict => {
                let ident_start = self.current_token_start;
                self.advance()?;
                if self.current == Token::Arrow && !self.prev_line_terminator {
                    self.advance()?;
                    let (body, body_is_strict) = if self.current == Token::LeftBrace {
                        let (stmts, strict) = self.parse_arrow_function_body(false)?;
                        (ArrowBody::Block(Body::new(stmts)), strict)
                    } else {
                        (
                            ArrowBody::Expression(Body::new(vec![self.arrow_return()?])),
                            false,
                        )
                    };
                    let source_text = self.source_since(ident_start);
                    return Ok(Expression::ArrowFunction(ArrowFunction {
                        params: vec![Pattern::Identifier("let".to_string())],
                        body,
                        is_async: false,
                        source_text,
                        body_is_strict,
                        info: Default::default(),
                    }));
                }
                Ok(Expression::Identifier("let".to_string(), 0))
            }
            Token::Keyword(Keyword::Static) if !self.strict => {
                let ident_start = self.current_token_start;
                self.advance()?;
                if self.current == Token::Arrow && !self.prev_line_terminator {
                    self.advance()?;
                    let (body, body_is_strict) = if self.current == Token::LeftBrace {
                        let (stmts, strict) = self.parse_arrow_function_body(false)?;
                        (ArrowBody::Block(Body::new(stmts)), strict)
                    } else {
                        (
                            ArrowBody::Expression(Body::new(vec![self.arrow_return()?])),
                            false,
                        )
                    };
                    let source_text = self.source_since(ident_start);
                    return Ok(Expression::ArrowFunction(ArrowFunction {
                        params: vec![Pattern::Identifier("static".to_string())],
                        body,
                        is_async: false,
                        source_text,
                        body_is_strict,
                        info: Default::default(),
                    }));
                }
                Ok(Expression::Identifier("static".to_string(), 0))
            }
            Token::Keyword(Keyword::Import) => {
                let at = CallSiteId::at(self.current_token_start).1;
                self.advance()?;
                if self.current == Token::Dot {
                    self.advance()?;
                    match &self.current {
                        Token::Identifier(name) if name == "meta" => {
                            if !self.is_module {
                                return Err(self.error("import.meta is only valid in module code"));
                            }
                            self.advance()?;
                            Ok(Expression::ImportMeta)
                        }
                        Token::Identifier(name) if self.import_phase(name) => {
                            let is_defer = name == "defer";
                            self.advance()?;
                            self.eat_phase_paren()?;
                            let old_no_in = self.no_in;
                            self.no_in = false;
                            let source = self.import_specifier()?;
                            let options = self.parse_import_options()?;
                            self.no_in = old_no_in;
                            self.eat(&Token::RightParen)?;
                            if is_defer {
                                Ok(Expression::ImportDefer(ExprBox::new(source), options))
                            } else {
                                Ok(Expression::ImportSource(ExprBox::new(source), options))
                            }
                        }
                        _ => Err(self.import_dot_error()),
                    }
                } else if self.current == Token::LeftParen {
                    self.advance()?;
                    let old_no_in = self.no_in;
                    self.no_in = false;
                    let source = self.import_specifier()?;
                    let options = self.parse_import_options()?;
                    self.no_in = old_no_in;
                    self.eat(&Token::RightParen)?;
                    Ok(self.import_call(source, options, at))
                } else {
                    Err(self.error("Unexpected 'import'"))
                }
            }
            Token::Keyword(Keyword::Async) => {
                let source_start = self.current_token_start;
                let (is_function, is_single_param, is_parenthesized) =
                    self.lookahead(|tokens| -> Result<_, ParseError> {
                        tokens.advance()?;
                        if tokens.line_terminator_before() {
                            return Ok((false, false, false));
                        }
                        if tokens.current() == &Token::Keyword(Keyword::Function) {
                            return Ok((true, false, false));
                        }
                        if tokens.current() == &Token::LeftParen {
                            return Ok((false, false, true));
                        }
                        let is_identifier = self.identifier_name(tokens.current()).is_some();
                        if !is_identifier {
                            return Ok((false, false, false));
                        }
                        tokens.advance()?;
                        self.arrow_after_async_param(tokens)
                    })?;

                self.advance()?; // async
                if is_function {
                    return self.parse_async_function_expression(source_start);
                }
                if is_parenthesized {
                    return self.parse_async_arrow_params(source_start);
                }
                if is_single_param {
                    let name = self
                        .current_identifier_name()
                        .expect("lookahead classified an identifier");
                    self.advance()?; // parameter
                    self.check_strict_binding_identifier(&name)?;
                    if name == "await" {
                        return Err(self.error(
                            "'await' is not allowed as a parameter name in an async function",
                        ));
                    }
                    self.advance()?; // arrow
                    let prev_async = self.in_async;
                    self.in_async = true;
                    let (body, body_is_strict) = if self.current == Token::LeftBrace {
                        let (stmts, strict) = self.parse_arrow_function_body(true)?;
                        (ArrowBody::Block(Body::new(stmts)), strict)
                    } else {
                        (
                            ArrowBody::Expression(Body::new(vec![self.async_arrow_return()?])),
                            false,
                        )
                    };
                    self.in_async = prev_async;
                    let source_text = self.source_since(source_start);
                    return Ok(Expression::ArrowFunction(ArrowFunction {
                        params: vec![Pattern::Identifier(name)],
                        body,
                        is_async: true,
                        source_text,
                        body_is_strict,
                        info: Default::default(),
                    }));
                }
                Ok(Expression::Identifier("async".to_string(), 0))
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
                    let (body, body_is_strict) = if self.current == Token::LeftBrace {
                        let (stmts, strict) = self.parse_arrow_function_body(false)?;
                        (ArrowBody::Block(Body::new(stmts)), strict)
                    } else {
                        (
                            ArrowBody::Expression(Body::new(vec![self.arrow_return()?])),
                            false,
                        )
                    };
                    let source_text = self.source_since(ident_start);
                    return Ok(Expression::ArrowFunction(ArrowFunction {
                        params: vec![Pattern::Identifier(name)],
                        body,
                        is_async: false,
                        source_text,
                        body_is_strict,
                        info: Default::default(),
                    }));
                }
                Ok(Expression::Identifier(name, Self::ident_pos(ident_start)))
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
                    let (body, body_is_strict) = if self.current == Token::LeftBrace {
                        let (stmts, strict) = self.parse_arrow_function_body(false)?;
                        (ArrowBody::Block(Body::new(stmts)), strict)
                    } else {
                        (
                            ArrowBody::Expression(Body::new(vec![self.arrow_return()?])),
                            false,
                        )
                    };
                    let source_text = self.source_since(ident_start);
                    return Ok(Expression::ArrowFunction(ArrowFunction {
                        params: vec![Pattern::Identifier(name)],
                        body,
                        is_async: false,
                        source_text,
                        body_is_strict,
                        info: Default::default(),
                    }));
                }
                Ok(Expression::Identifier(name, Self::ident_pos(ident_start)))
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
            Token::LegacyOctalLiteral(n) | Token::NonOctalDecimalLiteral(n) => {
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
                self.last_string_literal_has_legacy_octal = self.lexer.last_string_has_legacy_octal;
                if self.strict && self.lexer.last_string_has_legacy_octal {
                    return Err(self.error("Octal escape sequences are not allowed in strict mode"));
                }
                self.advance()?;
                Ok(Expression::Literal(Literal::String(s)))
            }
            Token::BigIntLiteral(s) => {
                let value =
                    crate::types::JsBigInt::new(crate::interpreter::parse_bigint_literal(s));
                self.advance()?;
                Ok(Expression::Literal(Literal::BigInt(value)))
            }
            Token::RegExpLiteral { pattern, flags } => {
                let p = pattern.clone();
                let f = flags.clone();
                self.advance()?;
                validate_regexp_literal(&p, &f)?;
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
                    let mut full_pattern: Vec<u16> = prefix.encode_utf16().collect();
                    full_pattern.extend_from_slice(&pattern);
                    self.prev_line_terminator = false;
                    self.current = self.lexer.next_token()?;
                    while self.current == Token::LineTerminator {
                        self.prev_line_terminator = true;
                        self.current = self.lexer.next_token()?;
                    }
                    self.current_token_start = self.lexer.token_start();
                    self.current_token_end = self.lexer.offset();
                    validate_regexp_literal(&full_pattern, &flags)?;
                    Ok(Expression::Literal(Literal::RegExp(full_pattern, flags)))
                } else {
                    Err(ParseError {
                        message: "Expected regex literal".to_string(),
                    })
                }
            }
            Token::LeftParen => {
                let paren_start = self.current_token_start;
                let list_start = self.token_mark();
                self.advance()?;
                let saved_no_in = self.no_in;
                self.no_in = false;
                // Parenthesized expression, arrow params, or empty arrow ()=>
                if self.current == Token::RightParen {
                    // () => ...
                    self.advance()?;
                    if self.current == Token::Arrow && !self.prev_line_terminator {
                        self.advance()?;
                        let (body, body_is_strict) = if self.current == Token::LeftBrace {
                            let (stmts, strict) = self.parse_arrow_function_body(false)?;
                            (ArrowBody::Block(Body::new(stmts)), strict)
                        } else {
                            (
                                ArrowBody::Expression(Body::new(vec![self.arrow_return()?])),
                                false,
                            )
                        };
                        let source_text = self.source_since(paren_start);
                        return Ok(Expression::ArrowFunction(ArrowFunction {
                            params: Vec::new(),
                            body,
                            is_async: false,
                            source_text,
                            body_is_strict,
                            info: Default::default(),
                        }));
                    }
                    return Err(self.error("Unexpected token )"));
                }
                // Check for rest param: (...a) =>
                if self.current == Token::Ellipsis {
                    // Arrow function with rest
                    self.advance()?;
                    let pat = self.arrow_rest_parameter()?;
                    let params = vec![Pattern::Rest(Box::new(pat))];
                    self.eat(&Token::RightParen)?;
                    self.arrow_after_rest()?;
                    let (body, body_is_strict) = if self.current == Token::LeftBrace {
                        let (stmts, strict) = self.parse_arrow_body_checked(false, &params)?;
                        (ArrowBody::Block(Body::new(stmts)), strict)
                    } else {
                        (
                            ArrowBody::Expression(Body::new(vec![self.arrow_return()?])),
                            false,
                        )
                    };
                    let source_text = self.source_since(paren_start);
                    return Ok(Expression::ArrowFunction(ArrowFunction {
                        params,
                        body,
                        is_async: false,
                        source_text,
                        body_is_strict,
                        info: Default::default(),
                    }));
                }
                let outer_cover = self.begin_cover_list();
                let expr = self.parse_cover_element()?;
                if self.current == Token::Comma || self.current == Token::RightParen {
                    let mut exprs = vec![expr];
                    let mut arrow_only = false;
                    while self.current == Token::Comma {
                        self.advance()?;
                        if self.current == Token::Ellipsis {
                            self.advance()?;
                            let pat = self.rest_parameter()?;
                            exprs.push(Expression::Spread(ExprBox::new(pattern_to_expr(pat)), 0));
                            arrow_only = true;
                            break;
                        }
                        if self.current == Token::RightParen {
                            arrow_only = true;
                            break;
                        }
                        let depth = v8_position::begin_node();
                        exprs.push(self.parse_cover_element()?);
                        v8_position::end_node(depth);
                    }
                    self.eat(&Token::RightParen)?;
                    self.no_in = saved_no_in;
                    let arrow = self.current == Token::Arrow && !self.prev_line_terminator;
                    self.end_cover_list(outer_cover, arrow)?;
                    if arrow {
                        self.advance()?;
                        let params = self.arrow_params_from_cover(exprs, list_start, false)?;
                        let (body, body_is_strict) = if self.current == Token::LeftBrace {
                            let (stmts, strict) = self.parse_arrow_body_checked(false, &params)?;
                            (ArrowBody::Block(Body::new(stmts)), strict)
                        } else {
                            (
                                ArrowBody::Expression(Body::new(vec![self.arrow_return()?])),
                                false,
                            )
                        };
                        let source_text = self.source_since(paren_start);
                        return Ok(Expression::ArrowFunction(ArrowFunction {
                            params,
                            body,
                            is_async: false,
                            source_text,
                            body_is_strict,
                            info: Default::default(),
                        }));
                    }
                    if arrow_only {
                        return Err(self.arrow_only_error(&exprs));
                    }
                    // Just a parenthesized expression
                    if exprs.len() == 1 {
                        let e = exprs.into_iter().next().unwrap();
                        self.last_expr_parenthesized = true;
                        self.parenthesized_arrow = matches!(e, Expression::ArrowFunction(_));
                        // Sequence marker: validate_destructuring_target rejects it
                        if matches!(
                            &e,
                            Expression::Object(..)
                                | Expression::Array(..)
                                | Expression::Assign(AssignOp::Assign, ..)
                        ) {
                            return Ok(Expression::Sequence(vec![e]));
                        }
                        return Ok(e);
                    }
                    return Ok(Expression::Sequence(exprs));
                }
                self.eat(&Token::RightParen)?;
                self.no_in = saved_no_in;
                Ok(expr)
            }
            Token::LeftBracket => {
                let result = self.parse_array_literal();
                self.last_expr_parenthesized = false;
                result
            }
            Token::LeftBrace => {
                let result = self.parse_object_literal();
                self.last_expr_parenthesized = false;
                result
            }
            Token::Keyword(Keyword::Function) => self.parse_function_expression(),
            Token::Keyword(Keyword::Class) => self.parse_class_expression(),
            Token::At => {
                // Decorated class expression: @decorator class {}
                self.parse_decorator_list()?;
                if self.current == Token::Keyword(Keyword::Class) {
                    self.parse_class_expression()
                } else {
                    Err(self.error("Expected 'class' after decorator"))
                }
            }
            Token::NoSubstitutionTemplate(_, _) | Token::TemplateHead(_, _) => {
                let tmpl = self.parse_template_literal_expr(false)?;
                Ok(Expression::Template(tmpl))
            }
            Token::Keyword(Keyword::Of) => {
                self.advance()?;
                Ok(Expression::Identifier("of".to_string(), 0))
            }
            _ => Err(self.unexpected_token("Unexpected token: ")),
        }
    }

    fn parse_array_literal(&mut self) -> Result<Expression, ParseError> {
        self.advance()?; // [
        let saved_no_in = self.no_in;
        self.no_in = false;
        let mut elements = Vec::new();
        let mut trailing_comma_after_spread = false;
        while self.current != Token::RightBracket {
            if self.current == Token::Comma {
                elements.push(None);
                self.advance()?;
                continue;
            }
            let is_spread = self.current == Token::Ellipsis;
            if is_spread {
                self.advance()?;
                elements.push(Some(self.parse_cover_spread()?));
            } else {
                elements.push(Some(self.parse_cover_element()?));
            }
            if self.current == Token::Comma {
                self.advance()?;
                if is_spread && self.current == Token::RightBracket {
                    trailing_comma_after_spread = true;
                }
            } else if self.current != Token::RightBracket {
                return Err(self.v8_error("Expected ',' or ']' in array literal"));
            }
        }
        self.eat(&Token::RightBracket)?;
        self.no_in = saved_no_in;
        Ok(Expression::Array(elements, trailing_comma_after_spread))
    }

    fn parse_object_literal(&mut self) -> Result<Expression, ParseError> {
        self.advance()?; // {
        let saved_no_in = self.no_in;
        self.no_in = false;
        let mut props = Vec::new();
        let mut trailing_comma_after_spread = false;
        while self.current != Token::RightBrace {
            let is_spread = self.current == Token::Ellipsis;
            if is_spread {
                self.advance()?;
                let expr = self.parse_cover_element()?;
                props.push(Property {
                    key: PropertyKey::Identifier("".into()),
                    value: Expression::Spread(ExprBox::new(expr), 0),
                    kind: PropertyKind::Init,
                    computed: false,
                    shorthand: false,
                    method: false,
                });
            } else {
                props.push(self.parse_object_property()?);
            }
            if self.current == Token::Comma {
                self.advance()?;
                if is_spread && self.current == Token::RightBrace {
                    trailing_comma_after_spread = true;
                }
            } else if self.current != Token::RightBrace {
                return Err(self.v8_error("Expected ',' or '}' after property definition"));
            }
        }
        self.eat(&Token::RightBrace)?;
        let mut has_proto = false;
        let mut has_proto_dup = false;
        for prop in &props {
            if prop.computed || prop.shorthand || prop.kind != PropertyKind::Init {
                continue;
            }
            let is_proto = prop.key.matches_name("__proto__");
            if is_proto {
                if matches!(&prop.value, Expression::Function(_)) {
                    continue;
                }
                if has_proto {
                    has_proto_dup = true;
                }
                has_proto = true;
            }
        }
        self.last_obj_had_proto_dup = has_proto_dup;
        self.no_in = saved_no_in;
        Ok(Expression::Object(props, trailing_comma_after_spread))
    }

    fn is_method_name_start(token: &Token) -> bool {
        matches!(
            token,
            Token::Identifier(_)
                | Token::IdentifierWithEscape(_)
                | Token::StringLiteral(_)
                | Token::NumericLiteral(_)
                | Token::LegacyOctalLiteral(_)
                | Token::NonOctalDecimalLiteral(_)
                | Token::BigIntLiteral(_)
                | Token::LeftBracket
                | Token::Keyword(_)
                | Token::PrivateName(_)
        )
    }

    fn is_accessor_name_start(token: &Token) -> bool {
        Self::is_method_name_start(token)
            || matches!(token, Token::NullLiteral | Token::BooleanLiteral(_))
    }

    fn parse_object_property(&mut self) -> Result<Property, ParseError> {
        let method_source_start = self.current_token_start;
        // Check for async method: { async method() {} } or { async *method() {} }
        // Escaped identifiers like \u0061sync don't count as async keyword.
        let is_async_prop = matches!(&self.current, Token::Identifier(n) if n == "async")
            || matches!(&self.current, Token::Keyword(Keyword::Async));
        let async_method = if is_async_prop {
            self.lookahead(|tokens| -> Result<Option<bool>, ParseError> {
                tokens.advance()?;
                if tokens.line_terminator_before() {
                    return Ok(None);
                }
                if tokens.current() == &Token::Star {
                    return Ok(Some(true));
                }
                Ok(Self::is_method_name_start(tokens.current()).then_some(false))
            })?
        } else {
            None
        };
        if let Some(is_generator) = async_method {
            self.advance()?; // async
            if is_generator {
                self.eat(&Token::Star)?;
            }
            let (key, computed) = self.parse_property_name()?;
            if matches!(&key, PropertyKey::Private(_)) {
                return Err(self.object_private_key(&key));
            }
            let prev_async = self.in_async;
            let prev_generator = self.in_generator;
            let prev_static_block = self.in_static_block;
            let prev_super_property = self.allow_super_property;
            self.in_async = true;
            if is_generator {
                self.in_generator = true;
            }
            self.in_static_block = false;
            self.allow_super_property = true;
            self.in_non_arrow_function += 1;
            let params = self.parse_formal_parameters()?;
            self.in_async = prev_async;
            self.in_generator = prev_generator;
            self.in_static_block = prev_static_block;
            self.allow_super_property = prev_super_property;
            self.set_function_param_names(&params);
            let (body, body_strict) =
                self.parse_function_body_inner(is_generator, true, true, false)?;
            self.in_non_arrow_function -= 1;
            if body_strict && !Self::is_simple_parameter_list(&params) {
                return Err(self.error(
                    "Illegal 'use strict' directive in function with non-simple parameter list",
                ));
            }
            self.check_duplicate_params_strict(&params)?;
            let source_text = self.source_since(method_source_start);
            return Ok(Property {
                key,
                value: Expression::Function(FunctionExpr {
                    name: None,
                    params,
                    body: Body::new(body),
                    is_async: true,
                    is_generator,
                    source_text,
                    body_is_strict: body_strict,
                    info: Default::default(),
                }),
                kind: PropertyKind::Init,
                computed,
                shorthand: false,
                method: true,
            });
        }

        // Escaped \u0061sync followed by * or method name is a SyntaxError.
        if matches!(&self.current, Token::IdentifierWithEscape(n) if n == "async")
            && self.lookahead(|tokens| -> Result<bool, ParseError> {
                tokens.advance()?;
                Ok(!tokens.line_terminator_before()
                    && (tokens.current() == &Token::Star
                        || Self::is_method_name_start(tokens.current())))
            })?
        {
            self.advance()?;
            return Err(self.error("Keyword must not contain escaped characters"));
        }

        // Check for generator method: { *method() {} }
        if self.current == Token::Star {
            self.advance()?; // consume *
            let (key, computed) = self.parse_property_name()?;
            if matches!(&key, PropertyKey::Private(_)) {
                return Err(self.object_private_key(&key));
            }
            let prev_generator = self.in_generator;
            let prev_static_block = self.in_static_block;
            let prev_super_property = self.allow_super_property;
            self.in_generator = true;
            self.in_static_block = false;
            self.allow_super_property = true;
            self.in_non_arrow_function += 1;
            let params = self.parse_formal_parameters()?;
            self.in_generator = prev_generator;
            self.in_static_block = prev_static_block;
            self.allow_super_property = prev_super_property;
            self.set_function_param_names(&params);
            let (body, body_strict) = self.parse_function_body_inner(true, false, true, false)?;
            self.in_non_arrow_function -= 1;
            if body_strict && !Self::is_simple_parameter_list(&params) {
                return Err(self.error(
                    "Illegal 'use strict' directive in function with non-simple parameter list",
                ));
            }
            self.check_duplicate_params_strict(&params)?;
            let source_text = self.source_since(method_source_start);
            return Ok(Property {
                key,
                value: Expression::Function(FunctionExpr {
                    name: None,
                    params,
                    body: Body::new(body),
                    is_async: false,
                    is_generator: true,
                    source_text,
                    body_is_strict: body_strict,
                    info: Default::default(),
                }),
                kind: PropertyKind::Init,
                computed,
                shorthand: false,
                method: true,
            });
        }

        // Check for get/set accessor
        // Escaped g\u0065t is not get/set — error if used as accessor
        let escaped_get_or_set =
            matches!(&self.current, Token::IdentifierWithEscape(n) if n == "get" || n == "set");
        let get_or_set_name = match &self.current {
            Token::Identifier(n) if n == "get" || n == "set" => Some(n.clone()),
            _ => None,
        };
        if let Some(n) = get_or_set_name {
            let saved_kind = if n == "get" {
                PropertyKind::Get
            } else {
                PropertyKind::Set
            };
            let is_accessor = self.lookahead(|tokens| -> Result<bool, ParseError> {
                tokens.advance()?;
                Ok(Self::is_accessor_name_start(tokens.current()))
            })?;
            if is_accessor {
                self.advance()?; // consume get/set
                let (key, computed) = self.parse_property_name()?;
                if matches!(&key, PropertyKey::Private(_)) {
                    return Err(self.object_private_key(&key));
                }
                let prev_static_block = self.in_static_block;
                self.in_static_block = false;
                self.in_non_arrow_function += 1;
                let params = self.parse_formal_parameters()?;
                self.in_static_block = prev_static_block;
                if saved_kind == PropertyKind::Get && !params.is_empty() {
                    return Err(self.error("Getter must not have any formal parameters"));
                }
                if saved_kind == PropertyKind::Set {
                    if params.len() != 1 {
                        return Err(self.error("Setter must have exactly one formal parameter"));
                    }
                    if matches!(params[0], Pattern::Rest(_)) {
                        return Err(
                            self.error("Setter function argument must not be a rest parameter")
                        );
                    }
                }
                self.set_function_param_names(&params);
                let (body, body_strict) =
                    self.parse_function_body_inner(false, false, true, false)?;
                self.in_non_arrow_function -= 1;
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
                        body: Body::new(body),
                        is_async: false,
                        is_generator: false,
                        source_text: self.source_since(method_source_start),
                        body_is_strict: body_strict,
                        info: Default::default(),
                    }),
                    kind: saved_kind,
                    computed,
                    shorthand: false,
                    method: true,
                });
            }
        }

        // Escaped get/set: reject only when the following token makes it an accessor.
        if escaped_get_or_set
            && self.lookahead(|tokens| -> Result<bool, ParseError> {
                tokens.advance()?;
                Ok(Self::is_method_name_start(tokens.current()))
            })?
        {
            self.advance()?;
            return Err(self.error("Keyword must not contain escaped characters"));
        }

        let is_escaped_reserved = matches!(&self.current, Token::IdentifierWithEscape(name)
            if Self::is_reserved_identifier(name, self.strict)
                || name == "await" || name == "yield");
        let was_keyword = matches!(&self.current, Token::Keyword(_) | Token::BooleanLiteral(_))
            || self.current == Token::NullLiteral;

        let (key, computed) = self.parse_property_name()?;

        if matches!(&key, PropertyKey::Private(_)) {
            return Err(self.object_private_key(&key));
        }

        // Shorthand: { x }
        if !computed
            && let PropertyKey::Identifier(ref name) = key
            && self.current != Token::Colon
            && self.current != Token::LeftParen
            && self.current != Token::Assign
        {
            if is_escaped_reserved {
                return Err(self.error("Keyword must not contain escaped characters"));
            }
            // Reserved words cannot be used as shorthand IdentifierReferences
            if matches!(name.as_str(), "true" | "false" | "null") {
                return Err(self.error(format!("Unexpected token '{name}'")));
            }
            // Keywords that are not valid IdentifierReferences cannot be shorthand
            if was_keyword {
                let n = name.as_str();
                // These keywords can be identifiers in sloppy mode
                let is_contextual =
                    matches!(n, "let" | "static" | "yield" | "await" | "async" | "of");
                // These are strict-mode-only reserved words
                let is_strict_reserved = matches!(
                    n,
                    "implements" | "interface" | "package" | "private" | "protected" | "public"
                );
                if !is_contextual && !is_strict_reserved {
                    return Err(self.error(format!("Unexpected token '{n}'")));
                }
            }
            // await is not a shorthand IdentifierReference in static blocks, async, modules
            if (self.in_static_block || self.in_async || self.is_module) && name == "await" {
                return Err(self.error("'await' is not allowed as an identifier in this context"));
            }
            // yield is not a shorthand IdentifierReference in generators or strict mode
            if self.in_generator && name == "yield" {
                return Err(self.error("'yield' is not allowed as an identifier in this context"));
            }
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
                    return Err(self.error(format!("Unexpected strict mode reserved word '{n}'")));
                }
            }
            return Ok(Property {
                value: Expression::Identifier(name.clone(), 0),
                key,
                kind: PropertyKind::Init,
                computed: false,
                shorthand: true,
                method: false,
            });
        }

        // CoverInitializedName: { x = defaultValue } (destructuring default)
        if !computed
            && let PropertyKey::Identifier(ref name) = key
            && self.current == Token::Assign
        {
            if is_escaped_reserved {
                return Err(self.error("Keyword must not contain escaped characters"));
            }
            // Keywords cannot be CoverInitializedName targets
            if was_keyword && Self::is_reserved_identifier(name, self.strict) {
                return Err(self.error(format!("Unexpected token '{name}'")));
            }
            let ident = name.clone();
            if self.strict && (ident == "eval" || ident == "arguments") {
                return Err(self.error("Invalid destructuring assignment target"));
            }
            self.advance()?; // consume '='
            let default_value = self.parse_assignment_expression()?;
            self.note_cover_initialized_name();
            return Ok(Property {
                key,
                value: Expression::Assign(
                    AssignOp::Assign,
                    ExprBox::new(Expression::Identifier(ident, 0)),
                    ExprBox::new(default_value),
                    0,
                ),
                kind: PropertyKind::Init,
                computed: false,
                shorthand: true,
                method: false,
            });
        }

        // Method: { foo() {} }
        if self.current == Token::LeftParen {
            let prev_static_block = self.in_static_block;
            let prev_super_property = self.allow_super_property;
            self.in_static_block = false;
            self.allow_super_property = true;
            self.in_non_arrow_function += 1;
            let params = self.parse_formal_parameters()?;
            self.in_static_block = prev_static_block;
            self.allow_super_property = prev_super_property;
            self.set_function_param_names(&params);
            let (body, body_strict) = self.parse_function_body_inner(false, false, true, false)?;
            self.in_non_arrow_function -= 1;
            if body_strict && !Self::is_simple_parameter_list(&params) {
                return Err(self.error(
                    "Illegal 'use strict' directive in function with non-simple parameter list",
                ));
            }
            if body_strict {
                self.check_strict_params(&params)?;
            }
            // Methods always use UniqueFormalParameters
            self.check_duplicate_params_strict(&params)?;
            return Ok(Property {
                key,
                value: Expression::Function(FunctionExpr {
                    name: None,
                    params,
                    body: Body::new(body),
                    is_async: false,
                    is_generator: false,
                    source_text: self.source_since(method_source_start),
                    body_is_strict: body_strict,
                    info: Default::default(),
                }),
                kind: PropertyKind::Init,
                computed,
                shorthand: false,
                method: true,
            });
        }

        // Regular: { key: value }
        self.colon_after(&key)?;
        self.track_property_value();
        let value = self.parse_cover_element()?;
        Ok(Property {
            key,
            value,
            kind: PropertyKind::Init,
            computed,
            shorthand: false,
            method: false,
        })
    }

    fn parse_function_expression(&mut self) -> Result<Expression, ParseError> {
        let source_start = self.current_token_start;
        self.advance()?;
        let is_generator = self.eat_star()?;
        let prev_static_block = self.in_static_block;
        self.in_static_block = false;
        let prev_generator = self.in_generator;
        let prev_async = self.in_async;
        // FunctionExpression name is [~Yield]; GeneratorExpression name is [+Yield]
        self.in_generator = is_generator;
        self.in_async = false;
        let name = if let Some(n) = self.current_identifier_name() {
            self.check_strict_binding_identifier(&n)?;
            self.advance()?;
            Some(n)
        } else {
            None
        };
        self.in_generator = is_generator;
        self.in_async = false;
        self.in_non_arrow_function += 1;
        let params = self.parse_formal_parameters()?;
        self.in_generator = prev_generator;
        self.in_async = prev_async;
        self.in_static_block = prev_static_block;
        self.set_function_param_names(&params);
        let (body, body_strict) =
            self.parse_function_body_inner(is_generator, false, false, false)?;
        self.in_non_arrow_function -= 1;
        if body_strict && !Self::is_simple_parameter_list(&params) {
            return Err(self.error(
                "Illegal 'use strict' directive in function with non-simple parameter list",
            ));
        }
        if body_strict {
            if let Some(ref n) = name {
                self.check_strict_function_name(n)?;
            }
            self.check_strict_params(&params)?;
        }
        if body_strict || self.strict || is_generator || !Self::is_simple_parameter_list(&params) {
            self.check_duplicate_params_strict(&params)?;
        }
        let source_text = self.source_since(source_start);
        Ok(Expression::Function(FunctionExpr {
            name,
            params,
            body: Body::new(body),
            is_async: false,
            is_generator,
            source_text,
            body_is_strict: body_strict,
            info: Default::default(),
        }))
    }

    fn parse_async_function_expression(
        &mut self,
        source_start: usize,
    ) -> Result<Expression, ParseError> {
        self.advance()?; // consume 'function'
        let is_generator = self.eat_star()?;
        let prev_generator = self.in_generator;
        let prev_async = self.in_async;
        self.in_generator = is_generator;
        self.in_async = true;
        let name = if matches!(&self.current, Token::Keyword(Keyword::Await)) {
            return Err(self.error("'await' is not allowed as a function name in async functions"));
        } else if let Some(n) = self.current_identifier_name() {
            self.check_strict_binding_identifier(&n)?;
            self.advance()?;
            Some(n)
        } else {
            None
        };
        self.in_non_arrow_function += 1;
        let params = self.parse_formal_parameters()?;
        self.in_generator = prev_generator;
        self.in_async = prev_async;
        self.set_function_param_names(&params);
        let (body, body_strict) =
            self.parse_function_body_inner(is_generator, true, false, false)?;
        self.in_non_arrow_function -= 1;
        if body_strict && !Self::is_simple_parameter_list(&params) {
            return Err(self.error(
                "Illegal 'use strict' directive in function with non-simple parameter list",
            ));
        }
        if body_strict {
            if let Some(ref n) = name {
                self.check_strict_function_name(n)?;
            }
            self.check_strict_params(&params)?;
        }
        self.check_duplicate_params_strict(&params)?;
        let source_text = self.source_since(source_start);
        Ok(Expression::Function(FunctionExpr {
            name,
            params,
            body: Body::new(body),
            is_async: true,
            is_generator,
            source_text,
            body_is_strict: body_strict,
            info: Default::default(),
        }))
    }

    fn parse_async_arrow_params(&mut self, source_start: usize) -> Result<Expression, ParseError> {
        // Current token is '(' — parse params, check for '=>'
        let list_start = self.token_mark();
        self.advance()?; // consume '('
        if self.current == Token::RightParen {
            self.advance()?;
            if self.current == Token::Arrow && !self.prev_line_terminator {
                self.advance()?;
                let prev_async = self.in_async;
                self.in_async = true;
                let (body, body_is_strict) = if self.current == Token::LeftBrace {
                    let (stmts, strict) = self.parse_arrow_function_body(true)?;
                    (ArrowBody::Block(Body::new(stmts)), strict)
                } else {
                    (
                        ArrowBody::Expression(Body::new(vec![self.async_arrow_return()?])),
                        false,
                    )
                };
                self.in_async = prev_async;
                let source_text = self.source_since(source_start);
                return Ok(Expression::ArrowFunction(ArrowFunction {
                    params: Vec::new(),
                    body,
                    is_async: true,
                    source_text,
                    body_is_strict,
                    info: Default::default(),
                }));
            }
            // async() — function call on 'async' identifier
            return Ok(Expression::Call(
                ExprBox::new(Expression::Identifier("async".to_string(), 0)),
                Vec::new(),
                CallSiteId::UNASSIGNED,
            ));
        }
        if self.current == Token::Ellipsis {
            // async(...rest) =>
            self.advance()?;
            let pat = self.rest_parameter()?;
            if Self::pattern_contains_await_identifier(&pat) {
                return Err(self.error("'await' is not allowed in async arrow formal parameters"));
            }
            let params = vec![Pattern::Rest(Box::new(pat))];
            self.eat(&Token::RightParen)?;
            self.eat(&Token::Arrow)?;
            let prev_async = self.in_async;
            self.in_async = true;
            let (body, body_is_strict) = if self.current == Token::LeftBrace {
                let (stmts, strict) = self.parse_arrow_body_checked(true, &params)?;
                (ArrowBody::Block(Body::new(stmts)), strict)
            } else {
                (
                    ArrowBody::Expression(Body::new(vec![self.async_arrow_return()?])),
                    false,
                )
            };
            self.in_async = prev_async;
            let source_text = self.source_since(source_start);
            return Ok(Expression::ArrowFunction(ArrowFunction {
                params,
                body,
                is_async: true,
                source_text,
                body_is_strict,
                info: Default::default(),
            }));
        }
        let outer_cover = self.begin_cover_list();
        let expr = self.parse_cover_element()?;
        if self.current == Token::Comma || self.current == Token::RightParen {
            let mut exprs = vec![expr];
            while self.current == Token::Comma {
                self.advance()?;
                if self.current == Token::Ellipsis {
                    self.advance()?;
                    let pat = self.rest_parameter()?;
                    exprs.push(Expression::Spread(ExprBox::new(pattern_to_expr(pat)), 0));
                    break;
                }
                if self.current == Token::RightParen {
                    break;
                }
                exprs.push(self.parse_cover_element()?);
            }
            self.eat(&Token::RightParen)?;
            let arrow = self.current == Token::Arrow && !self.prev_line_terminator;
            self.end_cover_list(outer_cover, arrow)?;
            if arrow {
                self.advance()?;
                let params = self.arrow_params_from_cover(exprs, list_start, true)?;
                let prev_async = self.in_async;
                self.in_async = true;
                let (body, body_is_strict) = if self.current == Token::LeftBrace {
                    let (stmts, strict) = self.parse_arrow_body_checked(true, &params)?;
                    (ArrowBody::Block(Body::new(stmts)), strict)
                } else {
                    (
                        ArrowBody::Expression(Body::new(vec![self.async_arrow_return()?])),
                        false,
                    )
                };
                self.in_async = prev_async;
                let source_text = self.source_since(source_start);
                return Ok(Expression::ArrowFunction(ArrowFunction {
                    params,
                    body,
                    is_async: true,
                    source_text,
                    body_is_strict,
                    info: Default::default(),
                }));
            }
            // Not an arrow — it's async(args) function call
            return Ok(Expression::Call(
                ExprBox::new(Expression::Identifier("async".to_string(), 0)),
                exprs,
                CallSiteId::UNASSIGNED,
            ));
        }
        self.eat_args_end()?;
        // async(expr) — function call
        Ok(Expression::Call(
            ExprBox::new(Expression::Identifier("async".to_string(), 0)),
            vec![expr],
            CallSiteId::UNASSIGNED,
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
            match self.current_identifier_name() {
                Some(n) => Some(self.class_binding(n)?),
                None => self.class_expression_name(prev_strict)?,
            }
        } else {
            None
        };
        let super_class = if self.current == Token::Keyword(Keyword::Extends) {
            self.advance()?;
            let heritage = self.parse_heritage_expression()?;
            if matches!(heritage, Expression::ArrowFunction(_)) && !self.last_expr_parenthesized {
                return Err(self.error("Arrow function is not allowed in class heritage"));
            }
            Some(Box::new(heritage))
        } else {
            None
        };
        self.set_strict(prev_strict);
        let body = self.parse_class_body()?;
        if super_class.is_none() {
            Self::check_no_direct_super_in_constructor(&body)?;
        }
        let source_text = self.source_since(source_start);
        Ok(Expression::Class(ClassExpr {
            name,
            super_class,
            body,
            source_text,
        }))
    }
}
