use super::*;

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
        let declarations = self.parse_variable_declaration_list()?;
        // "let" cannot be a bound name in let/const declarations (§13.3.1.1)
        for d in &declarations {
            let mut names = Vec::new();
            Self::collect_bound_names(&d.pattern, &mut names);
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

    pub(super) fn parse_variable_declaration_list(
        &mut self,
    ) -> Result<Vec<VariableDeclarator>, ParseError> {
        let mut decls = Vec::new();
        loop {
            let pattern = self.parse_binding_pattern()?;
            let init = if self.current == Token::Assign {
                self.advance()?;
                Some(self.parse_assignment_expression()?)
            } else {
                None
            };
            decls.push(VariableDeclarator { pattern, init });
            if self.current == Token::Comma {
                self.advance()?;
            } else {
                break;
            }
        }
        Ok(decls)
    }

    pub(super) fn parse_binding_pattern(&mut self) -> Result<Pattern, ParseError> {
        if let Some(name) = self.current_identifier_name() {
            self.check_strict_binding_identifier(&name)?;
            self.advance()?;
            return Ok(Pattern::Identifier(name));
        }
        match &self.current {
            Token::LeftBracket => self.parse_array_pattern(),
            Token::LeftBrace => self.parse_object_pattern(),
            _ => Err(self.error(format!("Expected binding pattern, got {:?}", self.current))),
        }
    }

    fn parse_array_pattern(&mut self) -> Result<Pattern, ParseError> {
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
                let rest = self.parse_binding_pattern()?;
                elements.push(Some(ArrayPatternElement::Rest(rest)));
                break;
            }
            let pat = self.parse_binding_pattern()?;
            let pat = if self.current == Token::Assign {
                self.advance()?;
                let default = self.parse_assignment_expression()?;
                Pattern::Assign(Box::new(pat), Box::new(default))
            } else {
                pat
            };
            elements.push(Some(ArrayPatternElement::Pattern(pat)));
            if self.current == Token::Comma {
                self.advance()?;
            }
        }
        self.eat(&Token::RightBracket)?;
        Ok(Pattern::Array(elements))
    }

    fn parse_object_pattern(&mut self) -> Result<Pattern, ParseError> {
        self.eat(&Token::LeftBrace)?;
        let mut props = Vec::new();
        while self.current != Token::RightBrace {
            if self.current == Token::Ellipsis {
                self.advance()?;
                let rest = self.parse_binding_pattern()?;
                props.push(ObjectPatternProperty::Rest(rest));
                break;
            }
            let key = self.parse_property_key_for_pattern()?;
            if self.current == Token::Colon {
                self.advance()?;
                let mut pat = self.parse_binding_pattern()?;
                if self.current == Token::Assign {
                    self.advance()?;
                    let default = self.parse_assignment_expression()?;
                    pat = Pattern::Assign(Box::new(pat), Box::new(default));
                }
                props.push(ObjectPatternProperty::KeyValue(key, pat));
            } else {
                // Shorthand: { x } or { x = default }
                let name = match &key {
                    PropertyKey::Identifier(n) => n.clone(),
                    _ => return Err(self.error("Expected identifier for shorthand pattern")),
                };
                if self.current == Token::Assign {
                    self.advance()?;
                    let default = self.parse_assignment_expression()?;
                    let pat =
                        Pattern::Assign(Box::new(Pattern::Identifier(name)), Box::new(default));
                    props.push(ObjectPatternProperty::KeyValue(key, pat));
                } else {
                    props.push(ObjectPatternProperty::Shorthand(name));
                }
            }
            if self.current == Token::Comma {
                self.advance()?;
            }
        }
        self.eat(&Token::RightBrace)?;
        Ok(Pattern::Object(props))
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
            Token::NumericLiteral(n) => {
                let n = *n;
                self.advance()?;
                Ok(PropertyKey::Number(n))
            }
            Token::LeftBracket => {
                self.advance()?;
                let expr = self.parse_assignment_expression()?;
                self.eat(&Token::RightBracket)?;
                Ok(PropertyKey::Computed(Box::new(expr)))
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
            _ => Err(self.error("Expected property name in object pattern")),
        }
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
        if is_generator {
            self.in_generator = true;
        }
        if is_async {
            self.in_async = true;
        }
        let params = self.parse_formal_parameters()?;
        self.in_generator = prev_generator;
        self.in_async = prev_async;
        let (body, body_strict) = self.parse_function_body_with_context(is_generator, is_async)?;
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
        let source_text = Some(self.source_since(source_start));
        Ok(Statement::FunctionDeclaration(FunctionDecl {
            name,
            params,
            body,
            is_async,
            is_generator,
            source_text,
        }))
    }

    pub(super) fn parse_class_declaration(&mut self) -> Result<Statement, ParseError> {
        let source_start = self.current_token_start;
        self.advance()?; // class
        // Class definitions are strict mode code — set strict before parsing name
        let prev_strict = self.strict;
        self.set_strict(true);
        let name = match self.current_identifier_name() {
            Some(n) => {
                self.advance()?;
                n
            }
            None => {
                self.set_strict(prev_strict);
                return Err(self.error("Expected class name"));
            }
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
        self.set_strict(true); // class bodies are always strict
        self.push_private_scope();
        let mut elements = Vec::new();
        // Track private names: value is (has_getter, has_setter, has_other)
        let mut private_names: std::collections::HashMap<String, (bool, bool, bool)> =
            std::collections::HashMap::new();
        while self.current != Token::RightBrace {
            if self.current == Token::Semicolon {
                self.advance()?;
                continue;
            }
            let element = self.parse_class_element()?;
            // Check for duplicate private names and register declarations
            if let Some((name, kind)) = Self::get_private_name_info(&element) {
                if name == "constructor" {
                    return Err(self.error("Class fields and methods cannot be named '#constructor'"));
                }
                let entry = private_names
                    .entry(name.clone())
                    .or_insert((false, false, false));
                let (has_getter, has_setter, has_other) = *entry;
                match kind {
                    PrivateNameKind::Getter => {
                        if has_getter || has_other {
                            return Err(self
                                .error(format!("Identifier '#{name}' has already been declared")));
                        }
                        entry.0 = true;
                    }
                    PrivateNameKind::Setter => {
                        if has_setter || has_other {
                            return Err(self
                                .error(format!("Identifier '#{name}' has already been declared")));
                        }
                        entry.1 = true;
                    }
                    PrivateNameKind::Other => {
                        if has_getter || has_setter || has_other {
                            return Err(self
                                .error(format!("Identifier '#{name}' has already been declared")));
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
        Ok(elements)
    }

    fn get_private_name_info(element: &ClassElement) -> Option<(String, PrivateNameKind)> {
        match element {
            ClassElement::Method(m) => {
                if let PropertyKey::Private(name) = &m.key {
                    let kind = match m.kind {
                        ClassMethodKind::Get => PrivateNameKind::Getter,
                        ClassMethodKind::Set => PrivateNameKind::Setter,
                        _ => PrivateNameKind::Other,
                    };
                    Some((name.clone(), kind))
                } else {
                    None
                }
            }
            ClassElement::Property(p) => {
                if let PropertyKey::Private(name) = &p.key {
                    Some((name.clone(), PrivateNameKind::Other))
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
            if self.current == Token::LeftBrace {
                self.eat(&Token::LeftBrace)?;
                let prev_super_property = self.allow_super_property;
                self.allow_super_property = true;
                let mut stmts = Vec::new();
                while self.current != Token::RightBrace {
                    stmts.push(self.parse_statement_or_declaration()?);
                }
                self.allow_super_property = prev_super_property;
                self.eat(&Token::RightBrace)?;
                return Ok(ClassElement::StaticBlock(stmts));
            }
        }

        let method_source_start = self.current_token_start;

        // Check for async method
        let is_async_method = matches!(&self.current, Token::Identifier(n) | Token::IdentifierWithEscape(n) if n == "async")
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
            {
                // field named 'async': class { async = value; }
                let value = if self.current == Token::Assign {
                    self.advance()?;
                    Some(self.parse_assignment_expression()?)
                } else {
                    None
                };
                self.eat_semicolon()?;
                return Ok(ClassElement::Property(ClassProperty {
                    key: PropertyKey::Identifier("async".to_string()),
                    value,
                    is_static,
                    computed: false,
                }));
            }
            // It's an async method: async [*] name() {}
            let is_generator = self.eat_star()?;
            let (key, computed) = self.parse_property_name()?;
            if !is_static
                && !computed
                && matches!(&key, PropertyKey::Identifier(n) if n == "constructor")
            {
                return Err(self.error("Class constructor may not be an async method"));
            }
            if is_static
                && !computed
                && matches!(&key, PropertyKey::Identifier(n) if n == "prototype")
            {
                return Err(self.error("Classes may not have a static property named 'prototype'"));
            }
            if self.current == Token::LeftParen {
                let func = self.parse_class_method_function(
                    true,
                    is_generator,
                    false,
                    method_source_start,
                )?;
                return Ok(ClassElement::Method(ClassMethod {
                    key,
                    kind: ClassMethodKind::Method,
                    value: func,
                    is_static,
                    computed,
                }));
            }
            let value = if self.current == Token::Assign {
                self.advance()?;
                Some(self.parse_assignment_expression()?)
            } else {
                None
            };
            self.eat_semicolon()?;
            return Ok(ClassElement::Property(ClassProperty {
                key,
                value,
                is_static,
                computed,
            }));
        }

        let kind = match &self.current {
            Token::Identifier(n) | Token::IdentifierWithEscape(n) if n == "get" => {
                self.advance()?;
                if self.current == Token::LeftParen {
                    let key = PropertyKey::Identifier("get".to_string());
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
                ClassMethodKind::Get
            }
            Token::Identifier(n) | Token::IdentifierWithEscape(n) if n == "set" => {
                self.advance()?;
                if self.current == Token::LeftParen {
                    let key = PropertyKey::Identifier("set".to_string());
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
                ClassMethodKind::Set
            }
            _ => ClassMethodKind::Method,
        };

        let is_generator = self.eat_star()?;

        let (key, computed) = self.parse_property_name()?;
        let is_constructor = !is_static
            && kind == ClassMethodKind::Method
            && matches!(&key, PropertyKey::Identifier(n) if n == "constructor");

        if is_static && !computed && matches!(&key, PropertyKey::Identifier(n) if n == "prototype")
        {
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
            let value = if self.current == Token::Assign {
                self.advance()?;
                let expr = self.parse_assignment_expression()?;
                // Class field initializers cannot contain 'arguments'
                if Self::contains_arguments(&expr) {
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
    }

    pub(super) fn parse_property_name(&mut self) -> Result<(PropertyKey, bool), ParseError> {
        if self.current == Token::LeftBracket {
            self.advance()?;
            let expr = self.parse_assignment_expression()?;
            self.eat(&Token::RightBracket)?;
            Ok((PropertyKey::Computed(Box::new(expr)), true))
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
        } else if let Token::NumericLiteral(n) | Token::LegacyOctalLiteral(n) = &self.current {
            if matches!(&self.current, Token::LegacyOctalLiteral(_)) && self.strict {
                return Err(self.error("Octal literals are not allowed in strict mode"));
            }
            let n = *n;
            self.advance()?;
            Ok((PropertyKey::Number(n), false))
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
            Err(self.error(format!("Expected property name, got {:?}", self.current)))
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
        if is_generator {
            self.in_generator = true;
        }
        if is_async {
            self.in_async = true;
        }
        let params = self.parse_formal_parameters()?;
        self.in_generator = prev_generator;
        self.in_async = prev_async;
        let (body, body_strict) =
            self.parse_function_body_inner(is_generator, is_async, true, is_constructor)?;
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
        let source_text = Some(self.source_since(method_source_start));
        Ok(FunctionExpr {
            name: None,
            params,
            body,
            is_async,
            is_generator,
            source_text,
        })
    }

    pub(super) fn parse_formal_parameters(&mut self) -> Result<Vec<Pattern>, ParseError> {
        self.eat(&Token::LeftParen)?;
        let prev_formal = self.in_formal_parameters;
        self.in_formal_parameters = true;
        let mut params = Vec::new();
        while self.current != Token::RightParen {
            if self.current == Token::Ellipsis {
                self.advance()?;
                let pat = self.parse_binding_pattern()?;
                params.push(Pattern::Rest(Box::new(pat)));
                break;
            }
            let pat = self.parse_binding_pattern()?;
            let pat = if self.current == Token::Assign {
                self.advance()?;
                let default = self.parse_assignment_expression()?;
                Pattern::Assign(Box::new(pat), Box::new(default))
            } else {
                pat
            };
            params.push(pat);
            if self.current == Token::Comma {
                self.advance()?;
            }
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
        self.parse_function_body_inner(is_generator, is_async, false, false)
    }

    pub(super) fn parse_function_body_inner(
        &mut self,
        is_generator: bool,
        is_async: bool,
        super_property: bool,
        super_call: bool,
    ) -> Result<(Vec<Statement>, bool), ParseError> {
        self.eat(&Token::LeftBrace)?;
        let prev_strict = self.strict;
        let prev_generator = self.in_generator;
        let prev_async = self.in_async;
        let prev_iteration = self.in_iteration;
        let prev_switch = self.in_switch;
        let prev_labels = std::mem::take(&mut self.labels);
        let prev_super_property = self.allow_super_property;
        let prev_super_call = self.allow_super_call;
        self.in_generator = is_generator;
        self.in_async = is_async;
        self.in_iteration = 0;
        self.in_switch = 0;
        self.in_function += 1;
        self.allow_super_property = super_property;
        self.allow_super_call = super_call;
        let prev_block = self.in_block_or_function;
        let prev_sc = self.in_switch_case;
        self.in_block_or_function = true;
        self.in_switch_case = false;
        let mut stmts = Vec::new();
        let mut in_directive_prologue = true;
        let mut has_use_strict_directive = false;

        while self.current != Token::RightBrace {
            let stmt = self.parse_statement_or_declaration()?;

            if in_directive_prologue {
                if let Some(directive) = self.is_directive_prologue(&stmt) {
                    if directive == "use strict" {
                        self.set_strict(true);
                        has_use_strict_directive = true;
                    }
                } else {
                    in_directive_prologue = false;
                }
            }

            stmts.push(stmt);
        }

        let was_strict = has_use_strict_directive;
        self.in_function -= 1;
        self.in_generator = prev_generator;
        self.in_async = prev_async;
        self.in_iteration = prev_iteration;
        self.in_switch = prev_switch;
        self.labels = prev_labels;
        self.allow_super_property = prev_super_property;
        self.allow_super_call = prev_super_call;
        self.in_block_or_function = prev_block;
        self.in_switch_case = prev_sc;
        self.eat(&Token::RightBrace)?;
        self.set_strict(prev_strict);
        Ok((stmts, was_strict))
    }

    #[allow(dead_code)]
    fn parse_function_body(&mut self) -> Result<(Vec<Statement>, bool), ParseError> {
        self.parse_function_body_with_context(false, false)
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
}
