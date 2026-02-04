use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_statement_or_declaration(&mut self) -> Result<Statement, ParseError> {
        if matches!(&self.current, Token::Identifier(n) if n == "using")
            && self.is_using_declaration()
        {
            if !self.in_block_or_function || self.in_switch_case {
                return Err(self.error("using declaration is not allowed in this position"));
            }
            return self.parse_using_declaration();
        }
        if matches!(&self.current, Token::Keyword(Keyword::Await))
            && self.in_async
            && self.is_await_using_declaration()
        {
            if !self.in_block_or_function || self.in_switch_case {
                return Err(self.error("await using declaration is not allowed in this position"));
            }
            return self.parse_await_using_declaration();
        }
        match &self.current {
            Token::Keyword(Keyword::Function) => self.parse_function_declaration(),
            Token::Keyword(Keyword::Class) => self.parse_class_declaration(),
            Token::Keyword(Keyword::Let) | Token::Keyword(Keyword::Const) => {
                self.parse_lexical_declaration()
            }
            Token::Keyword(Keyword::Async) if self.is_async_function() => {
                self.parse_function_declaration()
            }
            _ => self.parse_statement(),
        }
    }

    pub(super) fn is_async_function(&self) -> bool {
        // peek ahead: `async function` without line terminator
        // simplified: just check if current is async keyword
        matches!(&self.current, Token::Keyword(Keyword::Async))
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        if matches!(
            &self.current,
            Token::Keyword(Keyword::Let) | Token::Keyword(Keyword::Const)
        ) {
            return Err(
                self.error("Lexical declaration cannot appear in a single-statement context")
            );
        }
        if matches!(&self.current, Token::Keyword(Keyword::Async)) {
            let saved_lt = self.prev_line_terminator;
            let saved = self.advance()?;
            let is_async_fn =
                self.current == Token::Keyword(Keyword::Function) && !self.prev_line_terminator;
            self.push_back(self.current.clone(), self.prev_line_terminator);
            self.current = saved;
            self.prev_line_terminator = saved_lt;
            if is_async_fn {
                return Err(self.error(
                    "Async function declaration cannot appear in a single-statement context",
                ));
            }
        }
        match &self.current {
            Token::LeftBrace => self.parse_block_statement(),
            Token::Semicolon => {
                self.advance()?;
                Ok(Statement::Empty)
            }
            Token::Keyword(Keyword::Var) => self.parse_variable_statement(),
            Token::Keyword(Keyword::If) => self.parse_if_statement(),
            Token::Keyword(Keyword::While) => self.parse_while_statement(),
            Token::Keyword(Keyword::Do) => self.parse_do_while_statement(),
            Token::Keyword(Keyword::For) => self.parse_for_statement(),
            Token::Keyword(Keyword::Return) => self.parse_return_statement(),
            Token::Keyword(Keyword::Break) => self.parse_break_statement(),
            Token::Keyword(Keyword::Continue) => self.parse_continue_statement(),
            Token::Keyword(Keyword::Throw) => self.parse_throw_statement(),
            Token::Keyword(Keyword::Try) => self.parse_try_statement(),
            Token::Keyword(Keyword::Switch) => self.parse_switch_statement(),
            Token::Keyword(Keyword::With) => self.parse_with_statement(),
            Token::Keyword(Keyword::Debugger) => {
                self.advance()?;
                self.eat_semicolon()?;
                Ok(Statement::Debugger)
            }
            _ => self.parse_expression_statement_or_labeled(),
        }
    }

    fn parse_expression_statement_or_labeled(&mut self) -> Result<Statement, ParseError> {
        if let Some(name) = self.current_identifier_name() {
            let orig_token = self.current.clone();
            let ident_lt = self.prev_line_terminator;
            self.advance()?;
            if self.current == Token::Colon {
                self.advance()?;
                let is_iteration = matches!(
                    self.current,
                    Token::Keyword(Keyword::For)
                        | Token::Keyword(Keyword::While)
                        | Token::Keyword(Keyword::Do)
                );
                if self.strict && self.current == Token::Keyword(Keyword::Function) {
                    return Err(self.error("In strict mode code, functions can only be declared at top level or inside a block"));
                }
                self.labels.push((name.clone(), is_iteration));
                let stmt = self.parse_statement()?;
                self.labels.pop();
                return Ok(Statement::Labeled(name, Box::new(stmt)));
            }
            // Not a label — push back current and restore identifier
            let after_tok = std::mem::replace(&mut self.current, orig_token);
            let after_lt = std::mem::replace(&mut self.prev_line_terminator, ident_lt);
            let after_ts = self.current_token_start;
            let after_te = self.current_token_end;
            self.pushback = Some((after_tok, after_lt, after_ts, after_te));
        }
        self.parse_expression_statement()
    }

    fn parse_block_statement(&mut self) -> Result<Statement, ParseError> {
        self.eat(&Token::LeftBrace)?;
        let prev = self.in_block_or_function;
        let prev_sc = self.in_switch_case;
        self.in_block_or_function = true;
        self.in_switch_case = false;
        let mut stmts = Vec::new();
        let mut lexical_names: Vec<String> = Vec::new();
        while self.current != Token::RightBrace && self.current != Token::Eof {
            let stmt = self.parse_statement_or_declaration()?;
            Self::collect_lexical_names(&stmt, &mut lexical_names, self.strict)?;
            stmts.push(stmt);
        }
        self.in_block_or_function = prev;
        self.in_switch_case = prev_sc;
        self.eat(&Token::RightBrace)?;
        Ok(Statement::Block(stmts))
    }

    fn collect_lexical_names(
        stmt: &Statement,
        names: &mut Vec<String>,
        _strict: bool,
    ) -> Result<(), ParseError> {
        let new_names: Vec<String> = match stmt {
            Statement::Variable(decl) if decl.kind != VarKind::Var => {
                Self::bound_names_from_decl(decl)
            }
            Statement::ClassDeclaration(cls) => {
                vec![cls.name.clone()]
            }
            Statement::FunctionDeclaration(f) => {
                vec![f.name.clone()]
            }
            _ => vec![],
        };
        for name in &new_names {
            if names.contains(name) {
                return Err(ParseError {
                    message: format!("Identifier '{name}' has already been declared"),
                });
            }
        }
        names.extend(new_names);
        Ok(())
    }

    fn bound_names_from_decl(decl: &VariableDeclaration) -> Vec<String> {
        let mut names = Vec::new();
        for d in &decl.declarations {
            Self::bound_names_from_pattern(&d.pattern, &mut names);
        }
        names
    }

    fn bound_names_from_pattern(pat: &Pattern, names: &mut Vec<String>) {
        match pat {
            Pattern::Identifier(name) => names.push(name.clone()),
            Pattern::Array(elems) => {
                for elem in elems.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) => {
                            Self::bound_names_from_pattern(p, names);
                        }
                        ArrayPatternElement::Rest(p) => {
                            Self::bound_names_from_pattern(p, names);
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(_, p) => {
                            Self::bound_names_from_pattern(p, names);
                        }
                        ObjectPatternProperty::Shorthand(name) => {
                            names.push(name.clone());
                        }
                        ObjectPatternProperty::Rest(p) => {
                            Self::bound_names_from_pattern(p, names);
                        }
                    }
                }
            }
            Pattern::Assign(inner, _) => Self::bound_names_from_pattern(inner, names),
            Pattern::Rest(inner) => Self::bound_names_from_pattern(inner, names),
        }
    }

    fn parse_if_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // if
        self.eat(&Token::LeftParen)?;
        let test = self.parse_expression()?;
        self.eat(&Token::RightParen)?;
        let consequent = Box::new(self.parse_statement()?);
        let alternate = if self.current == Token::Keyword(Keyword::Else) {
            self.advance()?;
            Some(Box::new(self.parse_statement()?))
        } else {
            None
        };
        Ok(Statement::If(IfStatement {
            test,
            consequent,
            alternate,
        }))
    }

    fn parse_iteration_body(&mut self) -> Result<Box<Statement>, ParseError> {
        self.in_iteration += 1;
        let body = self.parse_statement();
        self.in_iteration -= 1;
        Ok(Box::new(body?))
    }

    fn parse_while_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // while
        self.eat(&Token::LeftParen)?;
        let test = self.parse_expression()?;
        self.eat(&Token::RightParen)?;
        let body = self.parse_iteration_body()?;
        Ok(Statement::While(WhileStatement { test, body }))
    }

    fn parse_do_while_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // do
        let body = self.parse_iteration_body()?;
        self.eat(&Token::Keyword(Keyword::While))?;
        self.eat(&Token::LeftParen)?;
        let test = self.parse_expression()?;
        self.eat(&Token::RightParen)?;
        self.eat_semicolon()?;
        Ok(Statement::DoWhile(DoWhileStatement { test, body }))
    }

    fn parse_for_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // for
        let is_await = if self.current == Token::Keyword(Keyword::Await) {
            if !self.in_async {
                return Err(self.error("for await...of is only valid in async functions"));
            }
            self.advance()?;
            true
        } else {
            false
        };
        self.eat(&Token::LeftParen)?;

        // for (using x of expr)
        if matches!(&self.current, Token::Identifier(n) if n == "using")
            && self.is_using_declaration()
        {
            self.advance()?; // using
            let ident = self
                .current_identifier_name()
                .ok_or_else(|| self.error("Expected identifier in using declaration"))?;
            self.advance()?;
            if self.current == Token::Keyword(Keyword::Of) {
                self.advance()?;
                let right = self.parse_assignment_expression()?;
                self.eat(&Token::RightParen)?;
                let body = self.parse_iteration_body()?;
                return Ok(Statement::ForOf(ForOfStatement {
                    left: ForInOfLeft::Variable(VariableDeclaration {
                        kind: VarKind::Using,
                        declarations: vec![VariableDeclarator {
                            pattern: Pattern::Identifier(ident),
                            init: None,
                        }],
                    }),
                    right,
                    body,
                    is_await,
                }));
            }
            return Err(self.error("using in for statement only valid with for-of"));
        }

        // for (init; test; update)
        // for (decl in expr)
        // for (decl of expr)
        let init = match &self.current {
            Token::Semicolon => None,
            Token::Keyword(Keyword::Var) => {
                self.advance()?;
                let decls = self.parse_variable_declaration_list()?;
                if self.current == Token::Keyword(Keyword::In) {
                    if is_await {
                        return Err(self.error("for await...in is not valid; use for await...of"));
                    }
                    self.advance()?;
                    let right = self.parse_expression()?;
                    self.eat(&Token::RightParen)?;
                    let body = self.parse_iteration_body()?;
                    return Ok(Statement::ForIn(ForInStatement {
                        left: ForInOfLeft::Variable(VariableDeclaration {
                            kind: VarKind::Var,
                            declarations: decls,
                        }),
                        right,
                        body,
                    }));
                }
                if self.current == Token::Keyword(Keyword::Of) {
                    self.advance()?;
                    let right = self.parse_assignment_expression()?;
                    self.eat(&Token::RightParen)?;
                    let body = self.parse_iteration_body()?;
                    return Ok(Statement::ForOf(ForOfStatement {
                        left: ForInOfLeft::Variable(VariableDeclaration {
                            kind: VarKind::Var,
                            declarations: decls,
                        }),
                        right,
                        body,
                        is_await,
                    }));
                }
                Some(ForInit::Variable(VariableDeclaration {
                    kind: VarKind::Var,
                    declarations: decls,
                }))
            }
            Token::Keyword(Keyword::Let) | Token::Keyword(Keyword::Const) => {
                let kind = if self.current == Token::Keyword(Keyword::Let) {
                    VarKind::Let
                } else {
                    VarKind::Const
                };
                self.advance()?;
                let decls = self.parse_variable_declaration_list()?;
                if self.current == Token::Keyword(Keyword::In) {
                    if is_await {
                        return Err(self.error("for await...in is not valid; use for await...of"));
                    }
                    self.advance()?;
                    let right = self.parse_expression()?;
                    self.eat(&Token::RightParen)?;
                    let body = self.parse_iteration_body()?;
                    return Ok(Statement::ForIn(ForInStatement {
                        left: ForInOfLeft::Variable(VariableDeclaration {
                            kind,
                            declarations: decls,
                        }),
                        right,
                        body,
                    }));
                }
                if self.current == Token::Keyword(Keyword::Of) {
                    self.advance()?;
                    let right = self.parse_assignment_expression()?;
                    self.eat(&Token::RightParen)?;
                    let body = self.parse_iteration_body()?;
                    return Ok(Statement::ForOf(ForOfStatement {
                        left: ForInOfLeft::Variable(VariableDeclaration {
                            kind,
                            declarations: decls,
                        }),
                        right,
                        body,
                        is_await,
                    }));
                }
                Some(ForInit::Variable(VariableDeclaration {
                    kind,
                    declarations: decls,
                }))
            }
            _ => {
                let expr = self.parse_expression()?;
                if self.current == Token::Keyword(Keyword::In) {
                    if is_await {
                        return Err(self.error("for await...in is not valid; use for await...of"));
                    }
                    self.advance()?;
                    let right = self.parse_expression()?;
                    self.eat(&Token::RightParen)?;
                    let body = self.parse_iteration_body()?;
                    return Ok(Statement::ForIn(ForInStatement {
                        left: ForInOfLeft::Pattern(expr_to_pattern(expr)?),
                        right,
                        body,
                    }));
                }
                if self.current == Token::Keyword(Keyword::Of) {
                    self.advance()?;
                    let right = self.parse_assignment_expression()?;
                    self.eat(&Token::RightParen)?;
                    let body = self.parse_iteration_body()?;
                    return Ok(Statement::ForOf(ForOfStatement {
                        left: ForInOfLeft::Pattern(expr_to_pattern(expr)?),
                        right,
                        body,
                        is_await,
                    }));
                }
                Some(ForInit::Expression(expr))
            }
        };

        if is_await {
            return Err(self.error("for await is only valid with for...of loops"));
        }

        self.eat(&Token::Semicolon)?;
        let test = if self.current != Token::Semicolon {
            Some(self.parse_expression()?)
        } else {
            None
        };
        self.eat(&Token::Semicolon)?;
        let update = if self.current != Token::RightParen {
            Some(self.parse_expression()?)
        } else {
            None
        };
        self.eat(&Token::RightParen)?;
        let body = self.parse_iteration_body()?;
        Ok(Statement::For(ForStatement {
            init,
            test,
            update,
            body,
        }))
    }

    fn parse_return_statement(&mut self) -> Result<Statement, ParseError> {
        if self.in_function == 0 {
            return Err(self.error("Illegal return statement"));
        }
        self.advance()?; // return
        let value = if self.current == Token::Semicolon
            || self.current == Token::RightBrace
            || self.current == Token::Eof
            || self.prev_line_terminator
        {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.eat_semicolon()?;
        Ok(Statement::Return(value))
    }

    fn parse_break_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?;
        let label = self.parse_optional_label()?;
        if let Some(ref l) = label {
            if !self.labels.iter().any(|(name, _)| name == l) {
                return Err(self.error(format!("Undefined label '{l}'")));
            }
        } else if self.in_iteration == 0 && self.in_switch == 0 {
            return Err(self.error("Illegal break statement"));
        }
        self.eat_semicolon()?;
        Ok(Statement::Break(label))
    }

    fn parse_continue_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?;
        let label = self.parse_optional_label()?;
        if let Some(ref l) = label {
            match self.labels.iter().find(|(name, _)| name == l) {
                None => return Err(self.error(format!("Undefined label '{l}'"))),
                Some((_, false)) => {
                    return Err(self.error(format!("Label '{l}' is not an iteration statement")));
                }
                _ => {}
            }
        } else if self.in_iteration == 0 {
            return Err(self.error("Illegal continue statement"));
        }
        self.eat_semicolon()?;
        Ok(Statement::Continue(label))
    }

    fn parse_throw_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // throw
        if self.prev_line_terminator {
            return Err(self.error("Illegal newline after throw"));
        }
        let expr = self.parse_expression()?;
        self.eat_semicolon()?;
        Ok(Statement::Throw(expr))
    }

    fn parse_try_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // try
        self.eat(&Token::LeftBrace)?;
        let mut block = Vec::new();
        while self.current != Token::RightBrace {
            block.push(self.parse_statement_or_declaration()?);
        }
        self.eat(&Token::RightBrace)?;

        let handler = if self.current == Token::Keyword(Keyword::Catch) {
            self.advance()?;
            let param = if self.current == Token::LeftParen {
                self.advance()?;
                let p = self.parse_binding_pattern()?;
                self.eat(&Token::RightParen)?;
                Some(p)
            } else {
                None
            };
            self.eat(&Token::LeftBrace)?;
            let mut body = Vec::new();
            while self.current != Token::RightBrace {
                body.push(self.parse_statement_or_declaration()?);
            }
            self.eat(&Token::RightBrace)?;
            Some(CatchClause { param, body })
        } else {
            None
        };

        let finalizer = if self.current == Token::Keyword(Keyword::Finally) {
            self.advance()?;
            self.eat(&Token::LeftBrace)?;
            let mut body = Vec::new();
            while self.current != Token::RightBrace {
                body.push(self.parse_statement_or_declaration()?);
            }
            self.eat(&Token::RightBrace)?;
            Some(body)
        } else {
            None
        };

        Ok(Statement::Try(TryStatement {
            block,
            handler,
            finalizer,
        }))
    }

    fn parse_switch_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // switch
        self.eat(&Token::LeftParen)?;
        let discriminant = self.parse_expression()?;
        self.eat(&Token::RightParen)?;
        self.eat(&Token::LeftBrace)?;
        self.in_switch += 1;
        let mut cases = Vec::new();
        let mut lexical_names: Vec<String> = Vec::new();
        while self.current != Token::RightBrace {
            let test = if self.current == Token::Keyword(Keyword::Case) {
                self.advance()?;
                let expr = self.parse_expression()?;
                self.eat(&Token::Colon)?;
                Some(expr)
            } else {
                self.eat(&Token::Keyword(Keyword::Default))?;
                self.eat(&Token::Colon)?;
                None
            };
            let mut consequent = Vec::new();
            let prev_sc = self.in_switch_case;
            self.in_switch_case = true;
            while self.current != Token::RightBrace
                && self.current != Token::Keyword(Keyword::Case)
                && self.current != Token::Keyword(Keyword::Default)
            {
                let stmt = self.parse_statement_or_declaration()?;
                Self::collect_lexical_names(&stmt, &mut lexical_names, self.strict)?;
                consequent.push(stmt);
            }
            self.in_switch_case = prev_sc;
            cases.push(SwitchCase { test, consequent });
        }
        self.in_switch -= 1;
        self.eat(&Token::RightBrace)?;
        Ok(Statement::Switch(SwitchStatement {
            discriminant,
            cases,
        }))
    }

    fn parse_with_statement(&mut self) -> Result<Statement, ParseError> {
        if self.strict {
            return Err(self.error("Strict mode code may not include a with statement"));
        }
        self.advance()?; // with
        self.eat(&Token::LeftParen)?;
        let expr = self.parse_expression()?;
        self.eat(&Token::RightParen)?;
        let body = self.parse_statement()?;
        Ok(Statement::With(expr, Box::new(body)))
    }
    fn parse_expression_statement(&mut self) -> Result<Statement, ParseError> {
        let expr = self.parse_expression()?;
        self.eat_semicolon()?;
        Ok(Statement::Expression(expr))
    }

    fn is_using_declaration(&mut self) -> bool {
        // `using` followed by an identifier on the same line (no line terminator)
        let saved_lt = self.prev_line_terminator;
        let saved = match self.advance() {
            Ok(t) => t,
            Err(_) => return false,
        };
        let lt = self.prev_line_terminator;
        let is_using = !lt && matches!(&self.current, Token::Identifier(_));
        self.push_back(self.current.clone(), self.prev_line_terminator);
        self.current = saved;
        self.prev_line_terminator = saved_lt;
        is_using
    }

    fn is_await_using_declaration(&mut self) -> bool {
        // `await` is current. Peek: next should be `using` (no line terminator),
        // then an identifier (no line terminator).
        // We can only push_back one token, so peek just the next token.
        let saved_lt = self.prev_line_terminator;
        let saved = match self.advance() {
            Ok(t) => t,
            Err(_) => return false,
        };
        let lt1 = self.prev_line_terminator;
        let is_using_kw = !lt1 && matches!(&self.current, Token::Identifier(n) if n == "using");
        // Restore
        self.push_back(self.current.clone(), self.prev_line_terminator);
        self.current = saved;
        self.prev_line_terminator = saved_lt;
        is_using_kw
    }

    fn parse_using_declaration(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // using
        let declarations = self.parse_using_binding_list()?;
        self.eat_semicolon()?;
        Ok(Statement::Variable(VariableDeclaration {
            kind: VarKind::Using,
            declarations,
        }))
    }

    fn parse_await_using_declaration(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // await
        self.advance()?; // using
        let declarations = self.parse_using_binding_list()?;
        self.eat_semicolon()?;
        Ok(Statement::Variable(VariableDeclaration {
            kind: VarKind::AwaitUsing,
            declarations,
        }))
    }

    fn parse_using_binding_list(&mut self) -> Result<Vec<VariableDeclarator>, ParseError> {
        let mut decls = Vec::new();
        loop {
            let name = self
                .current_identifier_name()
                .ok_or_else(|| self.error("Expected identifier in using declaration"))?;
            self.advance()?;
            if self.current != Token::Assign {
                return Err(self.error("using declaration requires an initializer"));
            }
            self.advance()?;
            let init = self.parse_assignment_expression()?;
            decls.push(VariableDeclarator {
                pattern: Pattern::Identifier(name),
                init: Some(init),
            });
            if self.current == Token::Comma {
                self.advance()?;
            } else {
                break;
            }
        }
        Ok(decls)
    }
}
