use super::*;

mod body_early_errors;
mod declarator_list;
mod param_names;
mod patterns;

fn bigint_literal_to_decimal(s: &str) -> String {
    crate::interpreter::parse_bigint_literal(s).to_string()
}

impl<'a> Parser<'a> {
    pub(super) fn parse_variable_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // var
        let declarations = self.parse_variable_declaration_list()?;
        self.eat_semicolon()?;
        Ok(Statement::Variable(VariableDeclaration {
            kind: VarKind::Var,
            declarations,
        }))
    }

    pub(super) fn parse_lexical_declaration(&mut self) -> Result<Statement, ParseError> {
        let kind = match &self.current {
            Token::Keyword(Keyword::Let) => VarKind::Let,
            Token::Keyword(Keyword::Const) => VarKind::Const,
            _ => return Err(self.error("Expected let or const")),
        };
        self.advance()?;
        let declarations = self.lexical_declarators(kind)?;
        for d in &declarations {
            // const declarations must have an initializer (§14.3.1.1)
            if kind == VarKind::Const && d.init.is_none() {
                return Err(self.error("Missing initializer in const declaration"));
            }
            // "let" cannot be a bound name in let/const declarations (§13.3.1.1)
            let mut names = Vec::new();
            d.pattern.bound_names(&mut names);
            for name in &names {
                if name == "let" {
                    return Err(self
                        .error("'let' is not allowed as a variable name in lexical declarations"));
                }
            }
        }
        self.eat_semicolon()?;
        Ok(Statement::Variable(VariableDeclaration {
            kind,
            declarations,
        }))
    }

    pub(super) fn parse_function_declaration(&mut self) -> Result<Statement, ParseError> {
        let source_start = self.current_token_start;
        let is_async = self.current == Token::Keyword(Keyword::Async);
        if is_async {
            self.advance()?;
        }
        self.eat(&Token::Keyword(Keyword::Function))?;
        let is_generator = self.eat_star()?;
        let name = match self.current_identifier_name() {
            Some(n) => {
                self.check_strict_binding_identifier(&n)?;
                self.advance()?;
                n
            }
            None => return Err(self.error("Expected function name")),
        };
        let prev_generator = self.in_generator;
        let prev_async = self.in_async;
        self.in_generator = is_generator;
        self.in_async = is_async;
        self.in_non_arrow_function += 1;
        let params = self.parse_formal_parameters()?;
        self.in_generator = prev_generator;
        self.in_async = prev_async;
        self.set_function_param_names(&params);
        let (body, body_strict) =
            self.parse_function_body_inner(is_generator, is_async, false, false)?;
        self.in_non_arrow_function -= 1;
        if body_strict && !Self::is_simple_parameter_list(&params) {
            return Err(self.error(
                "Illegal 'use strict' directive in function with non-simple parameter list",
            ));
        }
        if body_strict {
            self.check_strict_function_name(&name)?;
            self.check_strict_params(&params)?;
        }
        if body_strict
            || self.strict
            || is_async
            || is_generator
            || !Self::is_simple_parameter_list(&params)
        {
            self.check_duplicate_params_strict(&params)?;
        }
        let source_text = self.source_since(source_start);
        Ok(Statement::FunctionDeclaration(FunctionDecl {
            name,
            params,
            body: Body::new(body),
            is_async,
            is_generator,
            source_text,
            body_is_strict: body_strict,
            info: Default::default(),
            phase: Default::default(),
        }))
    }

    /// Consume and discard a list of decorator expressions (`@expr` prefixes).
    /// They may appear before class declarations, class expressions and elements.
    /// We parse them for syntax validity but do not yet implement decorator semantics.
    pub(super) fn parse_decorator_list(&mut self) -> Result<(), ParseError> {
        while self.current == Token::At {
            self.eat_at()?;
            self.parse_decorator_expression()?;
        }
        Ok(())
    }

    /// Parse one decorator expression after `@`.
    /// Grammar:
    ///   DecoratorMemberExpression . IdentifierName | . PrivateIdentifier (chained)
    ///   DecoratorMemberExpression ( Arguments )  → DecoratorCallExpression
    ///   ( Expression )             → DecoratorParenthesizedExpression
    fn parse_decorator_expression(&mut self) -> Result<(), ParseError> {
        if self.current == Token::LeftParen {
            // @( Expression )
            self.advance()?;
            self.parse_assignment_expression()?;
            self.eat(&Token::RightParen)?;
            return Ok(());
        }
        // DecoratorMemberExpression: IdentifierReference then .name or .#name
        match &self.current {
            Token::Identifier(_)
            | Token::IdentifierWithEscape(_)
            | Token::Keyword(_)
            | Token::BooleanLiteral(_)
            | Token::NullLiteral => {
                self.advance()?;
            }
            _ => return Err(self.error("Expected decorator expression after '@'")),
        }
        loop {
            if self.current == Token::Dot {
                self.advance()?;
                match &self.current {
                    Token::Identifier(_)
                    | Token::IdentifierWithEscape(_)
                    | Token::Keyword(_)
                    | Token::PrivateName(_) => {
                        self.advance()?;
                    }
                    _ => return Err(self.error("Expected property name after '.'")),
                }
            } else if self.current == Token::LeftParen {
                // DecoratorCallExpression: consume argument list
                self.advance()?; // consume (
                while self.current != Token::RightParen {
                    if self.current == Token::Ellipsis {
                        self.advance()?;
                    }
                    self.parse_assignment_expression()?;
                    if self.current != Token::Comma {
                        break;
                    }
                    self.advance()?;
                }
                self.eat(&Token::RightParen)?;
                break;
            } else {
                break;
            }
        }
        Ok(())
    }

    pub(super) fn parse_class_declaration(&mut self) -> Result<Statement, ParseError> {
        let source_start = self.current_token_start;
        self.advance()?; // class
        // Class definitions are strict mode code; set strict before parsing name
        let prev_strict = self.strict;
        self.set_strict(true);
        let name = match self.current_identifier_name() {
            Some(n) => self.class_binding(n)?,
            None => {
                // Check if token is an escaped reserved word
                if let Token::IdentifierWithEscape(n) = &self.current {
                    let n = n.clone();
                    self.set_strict(prev_strict);
                    return Err(self.escaped_class_name(&n));
                }
                return Err(self.class_name_error(prev_strict));
            }
        };
        let super_class = if self.current == Token::Keyword(Keyword::Extends) {
            self.advance()?;
            let heritage = self.parse_heritage_expression()?;
            // §15.7.1: ClassHeritage : extends LeftHandSideExpression
            // ArrowFunction is not a LeftHandSideExpression
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
        Ok(Statement::ClassDeclaration(ClassDecl {
            name,
            super_class,
            body,
            source_text,
        }))
    }

    pub(super) fn parse_class_body(&mut self) -> Result<Vec<ClassElement>, ParseError> {
        self.eat(&Token::LeftBrace)?;
        let prev_strict = self.strict;
        let prev_super_property = self.allow_super_property;
        self.set_strict(true); // always strict
        self.allow_super_property = true;
        self.push_private_scope();
        let mut elements = Vec::new();
        let mut has_constructor = false;
        // Track private names: value is (getter_static, setter_static, has_other)
        // Option<bool> = None means no getter/setter, Some(is_static) means present with staticness
        let mut private_names: HashMap<String, (Option<bool>, Option<bool>, bool)> = HashMap::new();
        while self.current != Token::RightBrace {
            if self.current == Token::Semicolon {
                self.advance()?;
                continue;
            }
            // Consume any decorator list before each class element
            self.parse_decorator_list()?;
            if self.current == Token::RightBrace {
                break;
            }
            let element = self.parse_class_element()?;

            // Check for duplicate constructors
            if let ClassElement::Method(m) = &element {
                if m.kind == ClassMethodKind::Constructor {
                    if has_constructor {
                        return Err(self.error("A class may only have one constructor"));
                    }
                    has_constructor = true;
                }
                // Non-static, non-computed getter/setter named "constructor" is forbidden
                if !m.is_static
                    && !m.computed
                    && matches!(m.kind, ClassMethodKind::Get | ClassMethodKind::Set)
                    && Self::key_is_constructor(&m.key)
                {
                    return Err(self.error("Class constructor may not be an accessor"));
                }
            }

            // Non-computed field named "constructor" is forbidden
            if let ClassElement::Property(p) = &element {
                if !p.computed && Self::key_is_constructor(&p.key) {
                    return Err(self.error("Classes may not have a field named 'constructor'"));
                }
                // Static non-computed field named "prototype" is forbidden
                if p.is_static && !p.computed && Self::key_is_prototype(&p.key) {
                    return Err(
                        self.error("Classes may not have a static property named 'prototype'")
                    );
                }
            }

            // Check for duplicate private names and register declarations
            if let Some((name, kind, is_static)) = Self::get_private_name_info(&element) {
                if name == "constructor" {
                    return Err(self.private_constructor_error(&element));
                }
                let entry = private_names
                    .entry(name.clone())
                    .or_insert((None, None, false));
                let (getter_static, setter_static, has_other) = *entry;
                match kind {
                    PrivateNameKind::Getter => {
                        if getter_static.is_some() || has_other {
                            return Err(self.duplicate_private_name(&name));
                        }
                        // If there's a setter, staticness must match
                        if let Some(setter_is_static) = setter_static
                            && setter_is_static != is_static
                        {
                            return Err(self.duplicate_private_name(&name));
                        }
                        entry.0 = Some(is_static);
                    }
                    PrivateNameKind::Setter => {
                        if setter_static.is_some() || has_other {
                            return Err(self.duplicate_private_name(&name));
                        }
                        // If there's a getter, staticness must match
                        if let Some(getter_is_static) = getter_static
                            && getter_is_static != is_static
                        {
                            return Err(self.duplicate_private_name(&name));
                        }
                        entry.1 = Some(is_static);
                    }
                    PrivateNameKind::Other => {
                        if getter_static.is_some() || setter_static.is_some() || has_other {
                            return Err(self.duplicate_private_name(&name));
                        }
                        entry.2 = true;
                    }
                }
                self.declare_private_name(&name);
            }
            elements.push(element);
        }
        self.pop_private_scope()?;
        self.eat(&Token::RightBrace)?;
        self.set_strict(prev_strict);
        self.allow_super_property = prev_super_property;
        Ok(elements)
    }

    fn key_is_constructor(key: &PropertyKey) -> bool {
        key.matches_name("constructor")
    }

    fn key_is_prototype(key: &PropertyKey) -> bool {
        key.matches_name("prototype")
    }

    pub(super) fn check_no_direct_super_in_constructor(
        body: &[ClassElement],
    ) -> Result<(), ParseError> {
        for elem in body {
            if let ClassElement::Method(m) = elem
                && m.kind == ClassMethodKind::Constructor
                && Self::stmts_has_direct_super(m.value.body.as_slice())
            {
                return Err(ParseError {
                    message: "'super' keyword unexpected here".to_string(),
                });
            }
        }
        Ok(())
    }

    fn stmts_has_direct_super(stmts: &[Statement]) -> bool {
        stmts.iter().any(Self::stmt_has_direct_super)
    }

    fn stmt_has_direct_super(stmt: &Statement) -> bool {
        use crate::ast::Statement;
        match stmt {
            Statement::Expression(e, _) | Statement::Throw(e, _) => Self::expr_has_direct_super(e),
            Statement::Return(Some(e), _) => Self::expr_has_direct_super(e),
            Statement::Return(None, _) | Statement::Empty | Statement::Debugger => false,
            Statement::Block(stmts) => Self::stmts_has_direct_super(stmts),
            Statement::Variable(decl) => decl
                .declarations
                .iter()
                .any(|d| d.init.as_ref().is_some_and(Self::expr_has_direct_super)),
            Statement::If(i) => {
                Self::expr_has_direct_super(&i.test)
                    || Self::stmt_has_direct_super(&i.consequent)
                    || i.alternate
                        .as_ref()
                        .is_some_and(|a| Self::stmt_has_direct_super(a))
            }
            Statement::While(w) => {
                Self::expr_has_direct_super(&w.test) || Self::stmt_has_direct_super(&w.body)
            }
            Statement::DoWhile(d) => {
                Self::expr_has_direct_super(&d.test) || Self::stmt_has_direct_super(&d.body)
            }
            Statement::For(f) => {
                f.init.as_ref().is_some_and(|i| match i {
                    crate::ast::ForInit::Expression(e) => Self::expr_has_direct_super(e),
                    crate::ast::ForInit::Variable(d) => d
                        .declarations
                        .iter()
                        .any(|dd| dd.init.as_ref().is_some_and(Self::expr_has_direct_super)),
                }) || f.test.as_ref().is_some_and(Self::expr_has_direct_super)
                    || f.update.as_deref().is_some_and(Self::expr_has_direct_super)
                    || Self::stmt_has_direct_super(&f.body)
            }
            Statement::ForIn(f) => {
                Self::expr_has_direct_super(&f.right) || Self::stmt_has_direct_super(&f.body)
            }
            Statement::ForOf(f) => {
                Self::expr_has_direct_super(&f.right) || Self::stmt_has_direct_super(&f.body)
            }
            Statement::Try(t) => {
                Self::stmts_has_direct_super(&t.block)
                    || t.handler
                        .as_ref()
                        .is_some_and(|h| Self::stmts_has_direct_super(&h.body))
                    || t.finalizer
                        .as_ref()
                        .is_some_and(|f| Self::stmts_has_direct_super(f))
            }
            Statement::Switch(s) => {
                Self::expr_has_direct_super(&s.discriminant)
                    || s.cases.iter().any(|c| {
                        c.test.as_ref().is_some_and(Self::expr_has_direct_super)
                            || Self::stmts_has_direct_super(&c.consequent)
                    })
            }
            Statement::Labeled(_, s) => Self::stmt_has_direct_super(s),
            Statement::With(e, s) => {
                Self::expr_has_direct_super(e) || Self::stmt_has_direct_super(s)
            }
            Statement::Break(..) | Statement::Continue(_) => false,
            Statement::FunctionDeclaration(_) | Statement::ClassDeclaration(_) => false,
        }
    }

    fn expr_has_direct_super(expr: &Expression) -> bool {
        use crate::ast::Expression;
        match expr {
            Expression::Call(callee, args, _) => {
                matches!(callee.as_ref(), Expression::Super)
                    || Self::expr_has_direct_super(callee)
                    || args.iter().any(Self::expr_has_direct_super)
            }
            Expression::New(callee, args, _) => {
                Self::expr_has_direct_super(callee)
                    || args.iter().any(Self::expr_has_direct_super)
            }
            Expression::Array(elems, _) => elems
                .iter()
                .any(|e| e.as_ref().is_some_and(Self::expr_has_direct_super)),
            Expression::Object(props, _) => props.iter().any(|p| {
                Self::expr_has_direct_super(&p.value)
                    || matches!(&p.key, crate::ast::PropertyKey::Computed(e) if Self::expr_has_direct_super(e))
            }),
            Expression::Member(object, property, _) => {
                Self::expr_has_direct_super(object)
                    || matches!(property, crate::ast::MemberProperty::Computed(e) if Self::expr_has_direct_super(e))
            }
            Expression::Binary(_, left, right, _)
            | Expression::Logical(_, left, right)
            | Expression::Assign(_, left, right, _) => {
                Self::expr_has_direct_super(left) || Self::expr_has_direct_super(right)
            }
            Expression::Unary(_, operand) | Expression::Update(.., operand, _) => {
                Self::expr_has_direct_super(operand)
            }
            Expression::Conditional(test, consequent, alternate) => {
                Self::expr_has_direct_super(test)
                    || Self::expr_has_direct_super(consequent)
                    || Self::expr_has_direct_super(alternate)
            }
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                exprs.iter().any(Self::expr_has_direct_super)
            }
            Expression::Import(inner, opts, _)
            | Expression::ImportDefer(inner, opts)
            | Expression::ImportSource(inner, opts) => {
                Self::expr_has_direct_super(inner)
                    || opts.as_ref().is_some_and(|e| Self::expr_has_direct_super(e))
            }
            Expression::Spread(e, _)
            | Expression::Await(e, _)
            | Expression::Typeof(e)
            | Expression::Void(e)
            | Expression::Delete(e) => Self::expr_has_direct_super(e),
            Expression::Yield(opt_e, _) => {
                opt_e.as_ref().is_some_and(|e| Self::expr_has_direct_super(e))
            }
            Expression::Template(tl) => tl.expressions.iter().any(Self::expr_has_direct_super),
            Expression::TaggedTemplate(tag, tl) => {
                Self::expr_has_direct_super(tag)
                    || tl.expressions.iter().any(Self::expr_has_direct_super)
            }
            Expression::OptionalChain(object, chain) => {
                Self::expr_has_direct_super(object) || Self::expr_has_direct_super(chain)
            }
            // Don't cross function/class boundaries
            _ => false,
        }
    }

    fn get_private_name_info(element: &ClassElement) -> Option<(String, PrivateNameKind, bool)> {
        match element {
            ClassElement::Method(m) => {
                if let PropertyKey::Private(name) = &m.key {
                    let kind = match m.kind {
                        ClassMethodKind::Get => PrivateNameKind::Getter,
                        ClassMethodKind::Set => PrivateNameKind::Setter,
                        _ => PrivateNameKind::Other,
                    };
                    Some((name.clone(), kind, m.is_static))
                } else {
                    None
                }
            }
            ClassElement::Property(p) => {
                if let PropertyKey::Private(name) = &p.key {
                    Some((name.clone(), PrivateNameKind::Other, p.is_static))
                } else {
                    None
                }
            }
            ClassElement::AutoAccessor(p) => {
                if let PropertyKey::Private(name) = &p.key {
                    Some((name.clone(), PrivateNameKind::Other, p.is_static))
                } else {
                    None
                }
            }
            ClassElement::StaticBlock(_) => None,
        }
    }

    fn parse_class_element(&mut self) -> Result<ClassElement, ParseError> {
        let is_static = self.current == Token::Keyword(Keyword::Static);
        if is_static {
            self.advance()?;

            // `static` followed by `=`, `;`, or `}` means it's a field named "static"
            if self.current == Token::Assign
                || self.current == Token::Semicolon
                || self.current == Token::RightBrace
            {
                let value = if self.current == Token::Assign {
                    self.advance()?;
                    Some(self.parse_field_initializer_value()?)
                } else {
                    None
                };
                self.eat_semicolon()?;
                return Ok(ClassElement::Property(ClassProperty {
                    key: PropertyKey::Identifier("static".to_string()),
                    value,
                    is_static: false,
                    computed: false,
                }));
            }

            if self.current == Token::LeftBrace {
                self.eat(&Token::LeftBrace)?;
                let stmts = self.with_function_context(
                    |p| {
                        p.allow_super_property = true;
                        p.allow_super_call = false;
                        p.in_function = 0;
                        // A static block body is a method, so `new.target` is
                        // allowed in it and in the arrow functions it contains.
                        p.in_non_arrow_function = 1;
                        p.in_generator = false;
                        p.in_async = false;
                        p.in_iteration = 0;
                        p.in_switch = 0;
                        p.in_static_block = true;
                        p.in_block_or_function = true;
                        p.in_switch_case = false;
                    },
                    |p| p.parse_static_block_statements(),
                )?;
                if crate::ast::stmts_contain_matching(&stmts, &crate::ast::is_arguments_reference) {
                    return Err(self.error("'arguments' is not allowed in class static blocks"));
                }
                self.eat(&Token::RightBrace)?;
                return Ok(ClassElement::StaticBlock(stmts));
            }
        }

        let method_source_start = self.current_token_start;

        // Check for async method (escaped identifiers like \u0061sync don't count as async keyword)
        self.escaped_async_member()?;
        let is_async_method = matches!(&self.current, Token::Identifier(n) if n == "async")
            || matches!(&self.current, Token::Keyword(Keyword::Async));
        if is_async_method {
            self.advance()?;
            if self.current == Token::LeftParen {
                // method named 'async': class { async() {} }
                let key = PropertyKey::Identifier("async".to_string());
                let func =
                    self.parse_class_method_function(false, false, false, method_source_start)?;
                return Ok(ClassElement::Method(ClassMethod {
                    key,
                    kind: ClassMethodKind::Method,
                    value: func,
                    is_static,
                    computed: false,
                }));
            }
            if self.current == Token::Assign
                || self.current == Token::Semicolon
                || self.current == Token::RightBrace
                || self.prev_line_terminator
            {
                return self.finish_class_field(
                    PropertyKey::Identifier("async".to_string()),
                    is_static,
                    false,
                );
            }
            // It's an async method: async [*] name() {}
            let is_generator = self.eat_star()?;
            let (key, computed) = self.parse_property_name()?;
            if !is_static && !computed && Self::key_is_constructor(&key) {
                return Err(self.error(Self::async_ctor_message(is_generator)));
            }
            if is_static && !computed && Self::key_is_prototype(&key) {
                return Err(self.error("Classes may not have a static property named 'prototype'"));
            }
            if self.current != Token::LeftParen {
                return Err(self.unexpected_token("Unexpected token: "));
            }
            let func =
                self.parse_class_method_function(true, is_generator, false, method_source_start)?;
            return Ok(ClassElement::Method(ClassMethod {
                key,
                kind: ClassMethodKind::Method,
                value: func,
                is_static,
                computed,
            }));
        }

        // `accessor FieldName`: auto-accessor syntax (decorators proposal).
        // Spec: `accessor [no LineTerminator here] ClassElementName`
        // If "accessor" is followed by `;`, `=`, `}`, or a line terminator,
        // it is just a field named "accessor", not an auto-accessor modifier.
        if matches!(&self.current, Token::Identifier(n) if n == "accessor") {
            self.advance()?;
            let next_is_field_name = !self.prev_line_terminator
                && !matches!(
                    self.current,
                    Token::LeftParen | Token::Assign | Token::Semicolon | Token::RightBrace
                );
            if next_is_field_name {
                let (key, computed) = self.parse_property_name()?;
                let value = if self.current == Token::Assign {
                    self.advance()?;
                    let expr = self.parse_field_initializer_value()?;
                    if crate::ast::expr_contains_matching(
                        &expr,
                        &crate::ast::is_arguments_reference,
                    ) {
                        return Err(
                            self.error("Class field initializer cannot reference 'arguments'")
                        );
                    }
                    Some(expr)
                } else {
                    None
                };
                self.eat_semicolon()?;
                return Ok(ClassElement::AutoAccessor(ClassProperty {
                    key,
                    value,
                    is_static,
                    computed,
                }));
            }
            if self.current == Token::LeftParen {
                // method named "accessor"
                let key = PropertyKey::Identifier("accessor".to_string());
                let func =
                    self.parse_class_method_function(false, false, false, method_source_start)?;
                return Ok(ClassElement::Method(ClassMethod {
                    key,
                    kind: ClassMethodKind::Method,
                    value: func,
                    is_static,
                    computed: false,
                }));
            }
            // field named "accessor" (followed by =, ;, or })
            return self.finish_class_field(
                PropertyKey::Identifier("accessor".to_string()),
                is_static,
                false,
            );
        }

        let kind = match &self.current {
            Token::Identifier(n) if n == "get" || n == "set" => {
                let is_get = n == "get";
                let name = if is_get { "get" } else { "set" };
                self.advance()?;
                if self.current == Token::LeftParen {
                    let key = PropertyKey::Identifier(name.to_string());
                    let func =
                        self.parse_class_method_function(false, false, false, method_source_start)?;
                    return Ok(ClassElement::Method(ClassMethod {
                        key,
                        kind: ClassMethodKind::Method,
                        value: func,
                        is_static,
                        computed: false,
                    }));
                }
                // A field named `get`/`set`; before `*` it relies on ASI
                if matches!(
                    self.current,
                    Token::Assign | Token::Semicolon | Token::RightBrace | Token::Star
                ) {
                    return self.finish_class_field(
                        PropertyKey::Identifier(name.to_string()),
                        is_static,
                        false,
                    );
                }
                if is_get {
                    ClassMethodKind::Get
                } else {
                    ClassMethodKind::Set
                }
            }
            _ => ClassMethodKind::Method,
        };

        let is_generator = self.eat_star()?;

        let (key, computed) = self.parse_property_name()?;
        let is_constructor = !is_static
            && kind == ClassMethodKind::Method
            && !computed
            && Self::key_is_constructor(&key);

        if is_static && !computed && Self::key_is_prototype(&key) {
            return Err(self.error("Classes may not have a static property named 'prototype'"));
        }

        if is_constructor && is_generator {
            return Err(self.error("Class constructor may not be a generator"));
        }

        if self.current == Token::LeftParen {
            let func = self.parse_class_method_function(
                false,
                is_generator,
                is_constructor,
                method_source_start,
            )?;
            if kind == ClassMethodKind::Get && !func.params.is_empty() {
                return Err(self.error("Getter must not have any formal parameters"));
            }
            if kind == ClassMethodKind::Set {
                if func.params.len() != 1 {
                    return Err(self.error("Setter must have exactly one formal parameter"));
                }
                if matches!(func.params[0], Pattern::Rest(_)) {
                    return Err(self.error("Setter function argument must not be a rest parameter"));
                }
            }
            let method_kind = if is_constructor {
                ClassMethodKind::Constructor
            } else {
                kind
            };
            Ok(ClassElement::Method(ClassMethod {
                key,
                kind: method_kind,
                value: func,
                is_static,
                computed,
            }))
        } else {
            if kind != ClassMethodKind::Method {
                return Err(self.error("Expected '(' after the accessor name"));
            }
            if is_generator {
                return Err(self.unexpected_token("Unexpected token: "));
            }
            self.finish_class_field(key, is_static, computed)
        }
    }

    /// The initializer and terminator of a public class field whose name is parsed.
    fn finish_class_field(
        &mut self,
        key: PropertyKey,
        is_static: bool,
        computed: bool,
    ) -> Result<ClassElement, ParseError> {
        let value = if self.current == Token::Assign {
            self.advance()?;
            let expr = self.parse_field_initializer_value()?;
            if crate::ast::expr_contains_matching(&expr, &crate::ast::is_arguments_reference) {
                return Err(self.error("Class field initializer cannot reference 'arguments'"));
            }
            Some(expr)
        } else {
            None
        };
        self.eat_semicolon()?;
        Ok(ClassElement::Property(ClassProperty {
            key,
            value,
            is_static,
            computed,
        }))
    }

    /// Parse field initializer value with ~Await, ~Yield parameters per spec §15.7.
    fn parse_field_initializer_value(&mut self) -> Result<Expression, ParseError> {
        self.with_function_context(
            |p| {
                p.in_async = false;
                p.in_generator = false;
                // Increment in_function so module-level await does not
                // match (class field initializers use [~Await] per spec).
                // An initializer is a method, so `new.target` is allowed in it
                // (Contains does not look into class bodies, §8.5.1).
                p.in_function += 1;
                p.in_non_arrow_function += 1;
            },
            |p| p.parse_assignment_expression(),
        )
    }

    pub(super) fn parse_property_name(&mut self) -> Result<(PropertyKey, bool), ParseError> {
        if self.current == Token::LeftBracket {
            self.advance()?;
            let saved_no_in = self.no_in;
            self.no_in = false;
            let expr = self.parse_assignment_expression()?;
            self.no_in = saved_no_in;
            self.eat(&Token::RightBracket)?;
            Ok((PropertyKey::Computed(ExprBox::new(expr)), true))
        } else if let Token::PrivateName(name) = &self.current {
            let name = name.clone();
            self.advance()?;
            Ok((PropertyKey::Private(name), false))
        } else if let Token::Identifier(name) | Token::IdentifierWithEscape(name) = &self.current {
            let name = name.clone();
            self.advance()?;
            Ok((PropertyKey::Identifier(name), false))
        } else if let Token::StringLiteral(s) = &self.current {
            let s = s.clone();
            self.advance()?;
            Ok((PropertyKey::String(s), false))
        } else if let Token::NumericLiteral(n)
        | Token::LegacyOctalLiteral(n)
        | Token::NonOctalDecimalLiteral(n) = &self.current
        {
            if matches!(
                &self.current,
                Token::LegacyOctalLiteral(_) | Token::NonOctalDecimalLiteral(_)
            ) && self.strict
            {
                return Err(self.error("Octal literals are not allowed in strict mode"));
            }
            let n = *n;
            self.advance()?;
            Ok((PropertyKey::Number(n), false))
        } else if let Token::BigIntLiteral(ref s) = self.current {
            let name = bigint_literal_to_decimal(s);
            self.advance()?;
            Ok((PropertyKey::String(name.encode_utf16().collect()), false))
        } else if let Token::Keyword(kw) = &self.current {
            // Keywords can be property names
            let name = kw.to_string();
            self.advance()?;
            Ok((PropertyKey::Identifier(name), false))
        } else if let Token::BooleanLiteral(b) = &self.current {
            let name = if *b { "true" } else { "false" }.to_string();
            self.advance()?;
            Ok((PropertyKey::Identifier(name), false))
        } else if self.current == Token::NullLiteral {
            self.advance()?;
            Ok((PropertyKey::Identifier("null".to_string()), false))
        } else {
            Err(self.unexpected_token("Expected property name, got "))
        }
    }

    fn parse_class_method_function(
        &mut self,
        is_async: bool,
        is_generator: bool,
        is_constructor: bool,
        method_source_start: usize,
    ) -> Result<FunctionExpr, ParseError> {
        let prev_generator = self.in_generator;
        let prev_async = self.in_async;
        let prev_static_block = self.in_static_block;
        if is_generator {
            self.in_generator = true;
        }
        if is_async {
            self.in_async = true;
        }
        self.in_static_block = false;
        self.in_non_arrow_function += 1;
        let params = self.parse_formal_parameters()?;
        self.in_generator = prev_generator;
        self.in_async = prev_async;
        self.in_static_block = prev_static_block;
        self.set_function_param_names(&params);
        let (body, body_strict) =
            self.parse_function_body_inner(is_generator, is_async, true, is_constructor)?;
        self.in_non_arrow_function -= 1;
        if body_strict && !Self::is_simple_parameter_list(&params) {
            return Err(self.error(
                "Illegal 'use strict' directive in function with non-simple parameter list",
            ));
        }
        if body_strict {
            self.check_strict_params(&params)?;
        }
        if body_strict
            || self.strict
            || is_async
            || is_generator
            || !Self::is_simple_parameter_list(&params)
        {
            self.check_duplicate_params_strict(&params)?;
        }
        let source_text = self.source_since(method_source_start);
        Ok(FunctionExpr {
            name: None,
            params,
            body: Body::new(body),
            is_async,
            is_generator,
            source_text,
            body_is_strict: body_strict,
            info: Default::default(),
        })
    }

    pub(super) fn parse_param_list(&mut self) -> Result<Vec<Pattern>, ParseError> {
        let paren = self.op_pos();
        self.eat(&Token::LeftParen)?;
        let prev_formal = self.in_formal_parameters;
        self.in_formal_parameters = true;
        let mut params = Vec::new();
        while self.current != Token::RightParen {
            if self.current == Token::Ellipsis {
                self.advance()?;
                let pat = self.rest_parameter()?;
                params.push(Pattern::Rest(Box::new(pat)));
                break;
            }
            let mut pat = self.parse_binding_pattern()?;
            if let Pattern::Array(elems) = &mut pat {
                elems.at = paren;
            }
            let pat = if self.current == Token::Assign {
                self.advance()?;
                let default = self.parse_assignment_expression()?;
                Pattern::Assign(Box::new(pat), ExprBox::new(default))
            } else {
                pat
            };
            params.push(pat);
            if self.current != Token::Comma {
                break;
            }
            self.advance()?;
        }
        self.eat(&Token::RightParen)?;
        self.in_formal_parameters = prev_formal;
        Ok(params)
    }

    pub(super) fn parse_function_body_with_context(
        &mut self,
        is_generator: bool,
        is_async: bool,
    ) -> Result<(Vec<Statement>, bool), ParseError> {
        self.in_non_arrow_function += 1;
        let result = self.parse_function_body_inner(is_generator, is_async, false, false);
        self.in_non_arrow_function -= 1;
        result
    }

    fn parse_static_block_statements(&mut self) -> Result<Vec<Statement>, ParseError> {
        let mut stmts = Vec::new();
        let mut lexical_names: Vec<String> = Vec::new();
        while self.current != Token::RightBrace {
            let stmt = self.parse_statement_or_declaration()?;
            Self::collect_lexical_names(&stmt, &mut lexical_names, self.strict)?;
            stmts.push(stmt);
        }
        // VarDeclaredNames must not overlap LexicallyDeclaredNames
        if !lexical_names.is_empty() {
            let mut var_names = Vec::new();
            for stmt in &stmts {
                Self::collect_var_declared_names(stmt, &mut var_names);
            }
            for name in &var_names {
                if lexical_names.contains(name) {
                    return Err(
                        self.error(format!("Identifier '{name}' has already been declared"))
                    );
                }
            }
        }
        Ok(stmts)
    }

    pub(super) fn parse_function_body_inner(
        &mut self,
        is_generator: bool,
        is_async: bool,
        super_property: bool,
        super_call: bool,
    ) -> Result<(Vec<Statement>, bool), ParseError> {
        let saved_param_names = self.function_param_names.take();
        self.eat(&Token::LeftBrace)?;
        let prev_strict = self.strict;

        let result = self.with_function_context(
            |p| {
                p.in_generator = is_generator;
                p.in_async = is_async;
                p.in_iteration = 0;
                p.in_switch = 0;
                p.in_function += 1;
                p.allow_super_property = super_property;
                p.allow_super_call = super_call;
                p.in_formal_parameters = false;
                p.in_block_or_function = true;
                p.in_switch_case = false;
                p.in_static_block = false;
            },
            |p| {
                p.parse_function_body_statements(saved_param_names.as_ref())
                    .and_then(|body| {
                        p.eat(&Token::RightBrace)?;
                        Ok(body)
                    })
            },
        );

        self.function_param_names = None;
        self.set_strict(prev_strict);
        result
    }

    fn parse_function_body_statements(
        &mut self,
        saved_param_names: Option<&HashSet<String>>,
    ) -> Result<(Vec<Statement>, bool), ParseError> {
        let mut stmts = Vec::new();
        let mut in_directive_prologue = true;
        let mut has_use_strict_directive = false;
        let mut prologue_had_legacy_octal = false;

        while self.current != Token::RightBrace {
            let stmt = self.parse_statement_or_declaration()?;

            if in_directive_prologue {
                if Self::is_string_literal_statement(&stmt) {
                    if let Some(directive) = self.is_directive_prologue(&stmt)
                        && directive == "use strict"
                    {
                        if prologue_had_legacy_octal {
                            return Err(
                                self.error("Octal escape sequences are not allowed in strict mode")
                            );
                        }
                        self.set_strict(true);
                        has_use_strict_directive = true;
                    }
                    if self.last_string_literal_has_legacy_octal {
                        prologue_had_legacy_octal = true;
                    }
                } else {
                    in_directive_prologue = false;
                }
            }

            stmts.push(stmt);
        }

        self.check_function_body_declarations(&stmts, saved_param_names)?;

        Ok((stmts, has_use_strict_directive))
    }

    pub(super) fn parse_arrow_function_body(
        &mut self,
        is_async: bool,
    ) -> Result<(Vec<Statement>, bool), ParseError> {
        self.parse_function_body_inner(
            false,
            is_async,
            self.allow_super_property,
            self.allow_super_call,
        )
    }

    pub(super) fn parse_arrow_expression_body(&mut self) -> Result<Expression, ParseError> {
        let prev_static_block = self.in_static_block;
        self.in_static_block = false;
        let result = self.parse_arrow_concise_body();
        self.in_static_block = prev_static_block;
        result
    }

    pub(super) fn parse_arrow_body_checked(
        &mut self,
        is_async: bool,
        params: &[Pattern],
    ) -> Result<(Vec<Statement>, bool), ParseError> {
        let outer_param_names = self.function_param_names.take();
        self.set_function_param_names(params);
        let (stmts, body_strict) = self.parse_arrow_function_body(is_async)?;
        self.function_param_names = outer_param_names;
        if body_strict && !Self::is_simple_parameter_list(params) {
            return Err(self.error(
                "Illegal 'use strict' directive in function with non-simple parameter list",
            ));
        }
        Ok((stmts, body_strict))
    }
}
