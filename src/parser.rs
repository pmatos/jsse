use crate::ast::*;
use crate::lexer::{Keyword, LexError, Lexer, Token};
use std::fmt;

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SyntaxError: {}", self.message)
    }
}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        ParseError { message: e.message }
    }
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
    prev_line_terminator: bool,
    pushback: Option<(Token, bool)>, // (token, had_line_terminator_before)
    strict: bool,
    in_function: u32,
    in_generator: bool,
    in_async: bool,
    in_iteration: u32,
    in_switch: u32,
    labels: Vec<(String, bool)>, // (name, is_iteration)
    allow_super_property: bool,
    allow_super_call: bool,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Result<Self, ParseError> {
        let mut lexer = Lexer::new(source);
        let mut had_lt = false;
        let current = loop {
            let tok = lexer.next_token()?;
            if tok == Token::LineTerminator {
                had_lt = true;
                continue;
            }
            break tok;
        };
        Ok(Self {
            lexer,
            current,
            prev_line_terminator: had_lt,
            pushback: None,
            strict: false,
            in_function: 0,
            in_generator: false,
            in_async: false,
            in_iteration: 0,
            in_switch: 0,
            labels: Vec::new(),
            allow_super_property: false,
            allow_super_call: false,
        })
    }

    fn advance(&mut self) -> Result<Token, ParseError> {
        let old = std::mem::replace(&mut self.current, Token::Eof);
        if let Some((tok, lt)) = self.pushback.take() {
            self.current = tok;
            self.prev_line_terminator = lt;
        } else {
            self.prev_line_terminator = false;
            loop {
                let tok = self.lexer.next_token()?;
                if tok == Token::LineTerminator {
                    self.prev_line_terminator = true;
                    continue;
                }
                self.current = tok;
                break;
            }
        }
        Ok(old)
    }

    fn push_back(&mut self, token: Token, had_lt: bool) {
        let old_current = std::mem::replace(&mut self.current, token);
        let old_lt = std::mem::replace(&mut self.prev_line_terminator, had_lt);
        self.pushback = Some((old_current, old_lt));
    }

    fn peek(&self) -> &Token {
        &self.current
    }

    fn eat(&mut self, expected: &Token) -> Result<(), ParseError> {
        if &self.current == expected {
            self.advance()?;
            Ok(())
        } else {
            Err(self.error(format!("Expected {expected:?}, got {:?}", self.current)))
        }
    }

    fn eat_semicolon(&mut self) -> Result<(), ParseError> {
        if self.current == Token::Semicolon {
            self.advance()?;
            return Ok(());
        }
        // ASI
        if self.prev_line_terminator
            || self.current == Token::RightBrace
            || self.current == Token::Eof
        {
            return Ok(());
        }
        Err(self.error("Expected semicolon"))
    }

    fn error(&self, msg: impl Into<String>) -> ParseError {
        ParseError {
            message: msg.into(),
        }
    }

    fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
        self.lexer.strict = strict;
    }

    fn eat_star(&mut self) -> Result<bool, ParseError> {
        if self.current == Token::Star {
            self.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn parse_optional_label(&mut self) -> Result<Option<String>, ParseError> {
        if !self.prev_line_terminator {
            if let Some(name) = self.current_identifier_name() {
                self.advance()?;
                return Ok(Some(name));
            }
        }
        Ok(None)
    }

    fn is_reserved_identifier(name: &str, strict: bool) -> bool {
        matches!(
            name,
            "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "continue"
                | "debugger"
                | "default"
                | "delete"
                | "do"
                | "else"
                | "enum"
                | "export"
                | "extends"
                | "false"
                | "finally"
                | "for"
                | "function"
                | "if"
                | "import"
                | "in"
                | "instanceof"
                | "new"
                | "null"
                | "return"
                | "super"
                | "switch"
                | "this"
                | "throw"
                | "true"
                | "try"
                | "typeof"
                | "var"
                | "void"
                | "while"
                | "with"
        ) || (strict
            && matches!(
                name,
                "implements" | "interface" | "package" | "private" | "protected" | "public"
            ))
    }

    fn current_identifier_name(&self) -> Option<String> {
        match &self.current {
            Token::Identifier(name) => {
                if Self::is_reserved_identifier(name, self.strict) {
                    None
                } else {
                    Some(name.clone())
                }
            }
            Token::Keyword(Keyword::Yield) if !self.in_generator && !self.strict => {
                Some("yield".to_string())
            }
            Token::Keyword(Keyword::Await) if !self.in_async => Some("await".to_string()),
            _ => None,
        }
    }

    fn collect_bound_names(pattern: &Pattern, names: &mut Vec<String>) {
        match pattern {
            Pattern::Identifier(n) => names.push(n.clone()),
            Pattern::Array(elems) => {
                for elem in elems.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                            Self::collect_bound_names(p, names);
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                            Self::collect_bound_names(p, names);
                        }
                        ObjectPatternProperty::Shorthand(n) => names.push(n.clone()),
                    }
                }
            }
            Pattern::Assign(p, _) | Pattern::Rest(p) => Self::collect_bound_names(p, names),
        }
    }

    fn check_duplicate_params_strict(&self, params: &[Pattern]) -> Result<(), ParseError> {
        let mut seen = std::collections::HashSet::new();
        let mut names = Vec::new();
        for p in params {
            Self::collect_bound_names(p, &mut names);
        }
        for name in &names {
            if !seen.insert(name.as_str()) {
                return Err(self.error("Duplicate parameter name not allowed in this context"));
            }
        }
        Ok(())
    }

    fn is_strict_reserved_word(name: &str) -> bool {
        matches!(
            name,
            "implements" | "interface" | "package" | "private" | "protected" | "public"
        )
    }

    fn check_strict_identifier(&self, name: &str) -> Result<(), ParseError> {
        if self.strict && Self::is_strict_reserved_word(name) {
            return Err(self.error(format!("Unexpected strict mode reserved word '{name}'")));
        }
        Ok(())
    }

    fn is_directive_prologue(stmt: &Statement) -> Option<&str> {
        match stmt {
            Statement::Expression(Expression::Literal(Literal::String(s))) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut body = Vec::new();
        let mut in_directive_prologue = true;

        while self.current != Token::Eof {
            let stmt = self.parse_statement_or_declaration()?;

            if in_directive_prologue {
                if let Some(directive) = Self::is_directive_prologue(&stmt) {
                    if directive == "use strict" {
                        self.set_strict(true);
                    }
                } else {
                    in_directive_prologue = false;
                }
            }

            body.push(stmt);
        }

        Ok(Program { body })
    }

    fn parse_statement_or_declaration(&mut self) -> Result<Statement, ParseError> {
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

    fn is_async_function(&self) -> bool {
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
                self.labels.push((name.clone(), is_iteration));
                let stmt = self.parse_statement()?;
                self.labels.pop();
                return Ok(Statement::Labeled(name, Box::new(stmt)));
            }
            // Not a label — push back current and restore identifier
            let after_tok = std::mem::replace(&mut self.current, orig_token);
            let after_lt = std::mem::replace(&mut self.prev_line_terminator, ident_lt);
            self.pushback = Some((after_tok, after_lt));
        }
        self.parse_expression_statement()
    }

    fn parse_block_statement(&mut self) -> Result<Statement, ParseError> {
        self.eat(&Token::LeftBrace)?;
        let mut stmts = Vec::new();
        let mut lexical_names: Vec<String> = Vec::new();
        while self.current != Token::RightBrace && self.current != Token::Eof {
            let stmt = self.parse_statement_or_declaration()?;
            Self::collect_lexical_names(&stmt, &mut lexical_names, self.strict)?;
            stmts.push(stmt);
        }
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

    fn parse_variable_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // var
        let declarations = self.parse_variable_declaration_list()?;
        self.eat_semicolon()?;
        Ok(Statement::Variable(VariableDeclaration {
            kind: VarKind::Var,
            declarations,
        }))
    }

    fn parse_lexical_declaration(&mut self) -> Result<Statement, ParseError> {
        let kind = match &self.current {
            Token::Keyword(Keyword::Let) => VarKind::Let,
            Token::Keyword(Keyword::Const) => VarKind::Const,
            _ => return Err(self.error("Expected let or const")),
        };
        self.advance()?;
        let declarations = self.parse_variable_declaration_list()?;
        self.eat_semicolon()?;
        Ok(Statement::Variable(VariableDeclaration {
            kind,
            declarations,
        }))
    }

    fn parse_variable_declaration_list(&mut self) -> Result<Vec<VariableDeclarator>, ParseError> {
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

    fn parse_binding_pattern(&mut self) -> Result<Pattern, ParseError> {
        if let Some(name) = self.current_identifier_name() {
            self.check_strict_identifier(&name)?;
            if self.strict && (name == "eval" || name == "arguments") {
                return Err(self.error(format!(
                    "'{name}' can't be defined or assigned to in strict mode code"
                )));
            }
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
            Token::Identifier(name) => {
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
            _ => Err(self.error("Expected property name in object pattern")),
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
        self.eat(&Token::LeftParen)?;

        // for (init; test; update)
        // for (decl in expr)
        // for (decl of expr)
        let init = match &self.current {
            Token::Semicolon => None,
            Token::Keyword(Keyword::Var) => {
                self.advance()?;
                let decls = self.parse_variable_declaration_list()?;
                if self.current == Token::Keyword(Keyword::In) {
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
                        is_await: false,
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
                        is_await: false,
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
                        is_await: false,
                    }));
                }
                Some(ForInit::Expression(expr))
            }
        };

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
                return Err(self.error(&format!("Undefined label '{l}'")));
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
                None => return Err(self.error(&format!("Undefined label '{l}'"))),
                Some((_, false)) => {
                    return Err(self.error(&format!("Label '{l}' is not an iteration statement")));
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
            while self.current != Token::RightBrace
                && self.current != Token::Keyword(Keyword::Case)
                && self.current != Token::Keyword(Keyword::Default)
            {
                let stmt = self.parse_statement_or_declaration()?;
                Self::collect_lexical_names(&stmt, &mut lexical_names, self.strict)?;
                consequent.push(stmt);
            }
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

    fn parse_function_declaration(&mut self) -> Result<Statement, ParseError> {
        let is_async = self.current == Token::Keyword(Keyword::Async);
        if is_async {
            self.advance()?;
        }
        self.eat(&Token::Keyword(Keyword::Function))?;
        let is_generator = self.eat_star()?;
        let name = match self.current_identifier_name() {
            Some(n) => {
                self.check_strict_identifier(&n)?;
                self.advance()?;
                n
            }
            None => return Err(self.error("Expected function name")),
        };
        let params = self.parse_formal_parameters()?;
        let (body, body_strict) = self.parse_function_body_with_context(is_generator, is_async)?;
        if body_strict {
            self.check_duplicate_params_strict(&params)?;
        }
        Ok(Statement::FunctionDeclaration(FunctionDecl {
            name,
            params,
            body,
            is_async,
            is_generator,
        }))
    }

    fn parse_class_declaration(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // class
        let name = match self.current_identifier_name() {
            Some(n) => {
                self.advance()?;
                n
            }
            None => return Err(self.error("Expected class name")),
        };
        let super_class = if self.current == Token::Keyword(Keyword::Extends) {
            self.advance()?;
            Some(Box::new(self.parse_left_hand_side_expression()?))
        } else {
            None
        };
        let body = self.parse_class_body()?;
        Ok(Statement::ClassDeclaration(ClassDecl {
            name,
            super_class,
            body,
        }))
    }

    fn parse_class_body(&mut self) -> Result<Vec<ClassElement>, ParseError> {
        self.eat(&Token::LeftBrace)?;
        let prev_strict = self.strict;
        self.set_strict(true); // class bodies are always strict
        let mut elements = Vec::new();
        while self.current != Token::RightBrace {
            if self.current == Token::Semicolon {
                self.advance()?;
                continue;
            }
            elements.push(self.parse_class_element()?);
        }
        self.eat(&Token::RightBrace)?;
        self.set_strict(prev_strict);
        Ok(elements)
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

        let kind = match &self.current {
            Token::Identifier(n) if n == "get" => {
                self.advance()?;
                if self.current == Token::LeftParen {
                    let key = PropertyKey::Identifier("get".to_string());
                    let func = self.parse_class_method_function(false, false, false)?;
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
            Token::Identifier(n) if n == "set" => {
                self.advance()?;
                if self.current == Token::LeftParen {
                    let key = PropertyKey::Identifier("set".to_string());
                    let func = self.parse_class_method_function(false, false, false)?;
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

        if self.current == Token::LeftParen {
            let func = self.parse_class_method_function(false, is_generator, is_constructor)?;
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
                Some(self.parse_assignment_expression()?)
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

    fn parse_property_name(&mut self) -> Result<(PropertyKey, bool), ParseError> {
        if self.current == Token::LeftBracket {
            self.advance()?;
            let expr = self.parse_assignment_expression()?;
            self.eat(&Token::RightBracket)?;
            Ok((PropertyKey::Computed(Box::new(expr)), true))
        } else if let Token::Identifier(name) = &self.current {
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
        } else {
            Err(self.error(format!("Expected property name, got {:?}", self.current)))
        }
    }

    fn parse_class_method_function(
        &mut self,
        is_async: bool,
        is_generator: bool,
        is_constructor: bool,
    ) -> Result<FunctionExpr, ParseError> {
        let params = self.parse_formal_parameters()?;
        let (body, body_strict) =
            self.parse_function_body_inner(is_generator, is_async, true, is_constructor)?;
        if body_strict {
            self.check_duplicate_params_strict(&params)?;
        }
        Ok(FunctionExpr {
            name: None,
            params,
            body,
            is_async,
            is_generator,
        })
    }

    fn parse_formal_parameters(&mut self) -> Result<Vec<Pattern>, ParseError> {
        self.eat(&Token::LeftParen)?;
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
        Ok(params)
    }

    fn parse_function_body_with_context(
        &mut self,
        is_generator: bool,
        is_async: bool,
    ) -> Result<(Vec<Statement>, bool), ParseError> {
        self.parse_function_body_inner(is_generator, is_async, false, false)
    }

    fn parse_function_body_inner(
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
        let mut stmts = Vec::new();
        let mut in_directive_prologue = true;

        while self.current != Token::RightBrace {
            let stmt = self.parse_statement_or_declaration()?;

            if in_directive_prologue {
                if let Some(directive) = Self::is_directive_prologue(&stmt) {
                    if directive == "use strict" {
                        self.set_strict(true);
                    }
                } else {
                    in_directive_prologue = false;
                }
            }

            stmts.push(stmt);
        }

        let was_strict = self.strict;
        self.in_function -= 1;
        self.in_generator = prev_generator;
        self.in_async = prev_async;
        self.in_iteration = prev_iteration;
        self.in_switch = prev_switch;
        self.labels = prev_labels;
        self.allow_super_property = prev_super_property;
        self.allow_super_call = prev_super_call;
        self.eat(&Token::RightBrace)?;
        self.set_strict(prev_strict);
        Ok((stmts, was_strict))
    }

    fn parse_function_body(&mut self) -> Result<(Vec<Statement>, bool), ParseError> {
        self.parse_function_body_with_context(false, false)
    }

    fn parse_arrow_function_body(&mut self) -> Result<(Vec<Statement>, bool), ParseError> {
        self.parse_function_body_inner(
            false,
            false,
            self.allow_super_property,
            self.allow_super_call,
        )
    }

    fn parse_expression_statement(&mut self) -> Result<Statement, ParseError> {
        let expr = self.parse_expression()?;
        self.eat_semicolon()?;
        Ok(Statement::Expression(expr))
    }

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
            if self.strict {
                if let Expression::Identifier(name) = expr {
                    if name == "eval" || name == "arguments" {
                        return Err(
                            self.error("Assignment to 'eval' or 'arguments' in strict mode")
                        );
                    }
                }
            }
            return Ok(());
        }
        Err(self.error("Invalid left-hand side in assignment"))
    }

    fn parse_assignment_expression(&mut self) -> Result<Expression, ParseError> {
        // YieldExpression in generator context
        if self.in_generator && self.current == Token::Keyword(Keyword::Yield) {
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
            let consequent = self.parse_assignment_expression()?;
            self.eat(&Token::Colon)?;
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
        let mut left = self.parse_shift()?;
        loop {
            let op = match &self.current {
                Token::LessThan => BinaryOp::Lt,
                Token::GreaterThan => BinaryOp::Gt,
                Token::LessThanEqual => BinaryOp::LtEq,
                Token::GreaterThanEqual => BinaryOp::GtEq,
                Token::Keyword(Keyword::Instanceof) => BinaryOp::Instanceof,
                Token::Keyword(Keyword::In) => BinaryOp::In,
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
            Token::Keyword(Keyword::Await) if self.in_async => {
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

    fn parse_left_hand_side_expression(&mut self) -> Result<Expression, ParseError> {
        let mut expr = if self.current == Token::Keyword(Keyword::New) {
            self.parse_new_expression()?
        } else {
            self.parse_primary_expression()?
        };

        loop {
            match &self.current {
                Token::Dot => {
                    self.advance()?;
                    let name = match &self.current {
                        Token::Identifier(n) => n.clone(),
                        Token::Keyword(kw) => kw.to_string(),
                        _ => return Err(self.error("Expected identifier after '.'")),
                    };
                    self.advance()?;
                    expr = Expression::Member(Box::new(expr), MemberProperty::Dot(name));
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
                    let tmpl = self.parse_template_literal_expr()?;
                    expr = Expression::TaggedTemplate(Box::new(expr), tmpl);
                }
                Token::OptionalChain => {
                    self.advance()?;
                    let prop = if self.current == Token::LeftParen {
                        let args = self.parse_arguments()?;
                        Expression::Call(Box::new(Expression::Identifier("".into())), args)
                    } else if self.current == Token::LeftBracket {
                        self.advance()?;
                        let p = self.parse_expression()?;
                        self.eat(&Token::RightBracket)?;
                        p
                    } else {
                        let name = match &self.current {
                            Token::Identifier(n) => n.clone(),
                            _ => return Err(self.error("Expected property after '?.'")),
                        };
                        self.advance()?;
                        Expression::Identifier(name)
                    };
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
            if let Token::Identifier(ref name) = self.current {
                if name == "target" {
                    if self.in_function == 0 {
                        return Err(self.error("new.target expression is not allowed here"));
                    }
                    self.advance()?; // target
                    return Ok(Expression::NewTarget);
                }
            }
            return Err(self.error("Expected 'target' after 'new.'"));
        }
        if self.current == Token::Keyword(Keyword::New) {
            let inner = self.parse_new_expression()?;
            return Ok(Expression::New(Box::new(inner), Vec::new()));
        }
        let callee = self.parse_primary_expression()?;
        // handle member access on new target
        let mut callee = callee;
        loop {
            match &self.current {
                Token::Dot => {
                    self.advance()?;
                    let name = match &self.current {
                        Token::Identifier(n) => n.clone(),
                        Token::Keyword(kw) => kw.to_string(),
                        _ => return Err(self.error("Expected identifier after '.'")),
                    };
                    self.advance()?;
                    callee = Expression::Member(Box::new(callee), MemberProperty::Dot(name));
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
            Token::Identifier(name) => {
                let name = name.clone();
                if Self::is_reserved_identifier(&name, self.strict) {
                    return Err(self.error(&format!("Unexpected reserved word '{name}'")));
                }
                self.check_strict_identifier(&name)?;
                self.advance()?;
                // Arrow function: (ident) => or ident =>
                if self.current == Token::Arrow && !self.prev_line_terminator {
                    self.advance()?;
                    let body = if self.current == Token::LeftBrace {
                        let (stmts, _) = self.parse_arrow_function_body()?;
                        ArrowBody::Block(stmts)
                    } else {
                        ArrowBody::Expression(Box::new(self.parse_assignment_expression()?))
                    };
                    return Ok(Expression::ArrowFunction(ArrowFunction {
                        params: vec![Pattern::Identifier(name)],
                        body,
                        is_async: false,
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
                self.advance()?;
                // Could be: parenthesized expression, arrow params, or empty arrow ()=>
                if self.current == Token::RightParen {
                    // () => ...
                    self.advance()?;
                    if self.current == Token::Arrow {
                        self.advance()?;
                        let body = if self.current == Token::LeftBrace {
                            ArrowBody::Block(self.parse_arrow_function_body()?.0)
                        } else {
                            ArrowBody::Expression(Box::new(self.parse_assignment_expression()?))
                        };
                        return Ok(Expression::ArrowFunction(ArrowFunction {
                            params: Vec::new(),
                            body,
                            is_async: false,
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
                        ArrowBody::Block(self.parse_arrow_function_body()?.0)
                    } else {
                        ArrowBody::Expression(Box::new(self.parse_assignment_expression()?))
                    };
                    return Ok(Expression::ArrowFunction(ArrowFunction {
                        params,
                        body,
                        is_async: false,
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
                        let body = if self.current == Token::LeftBrace {
                            ArrowBody::Block(self.parse_arrow_function_body()?.0)
                        } else {
                            ArrowBody::Expression(Box::new(self.parse_assignment_expression()?))
                        };
                        return Ok(Expression::ArrowFunction(ArrowFunction {
                            params,
                            body,
                            is_async: false,
                        }));
                    }
                    // Just a parenthesized expression
                    if exprs.len() == 1 {
                        return Ok(exprs.into_iter().next().unwrap());
                    }
                    return Ok(Expression::Sequence(exprs));
                }
                self.eat(&Token::RightParen)?;
                Ok(expr)
            }
            Token::LeftBracket => self.parse_array_literal(),
            Token::LeftBrace => self.parse_object_literal(),
            Token::Keyword(Keyword::Function) => self.parse_function_expression(),
            Token::Keyword(Keyword::Class) => self.parse_class_expression(),
            Token::NoSubstitutionTemplate(_, _) | Token::TemplateHead(_, _) => {
                let tmpl = self.parse_template_literal_expr()?;
                Ok(Expression::Template(tmpl))
            }
            _ => Err(self.error(format!("Unexpected token: {:?}", self.current))),
        }
    }

    fn parse_array_literal(&mut self) -> Result<Expression, ParseError> {
        self.advance()?; // [
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
        Ok(Expression::Array(elements))
    }

    fn parse_object_literal(&mut self) -> Result<Expression, ParseError> {
        self.advance()?; // {
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
        Ok(Expression::Object(props))
    }

    fn parse_object_property(&mut self) -> Result<Property, ParseError> {
        // Check for get/set accessor
        if let Token::Identifier(n) = &self.current
            && (n == "get" || n == "set")
        {
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
                    | Token::StringLiteral(_)
                    | Token::NumericLiteral(_)
                    | Token::LegacyOctalLiteral(_)
                    | Token::LeftBracket
                    | Token::Keyword(_)
            );
            if is_accessor {
                let (key, computed) = self.parse_property_name()?;
                let params = self.parse_formal_parameters()?;
                let (body, _) = self.parse_function_body_inner(false, false, true, false)?;
                return Ok(Property {
                    key,
                    value: Expression::Function(FunctionExpr {
                        name: None,
                        params,
                        body,
                        is_async: false,
                        is_generator: false,
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
            return Ok(Property {
                value: Expression::Identifier(name.clone()),
                key,
                kind: PropertyKind::Init,
                computed: false,
                shorthand: true,
            });
        }

        // Method: { foo() {} }
        if self.current == Token::LeftParen {
            let params = self.parse_formal_parameters()?;
            let (body, _) = self.parse_function_body_inner(false, false, true, false)?;
            return Ok(Property {
                key,
                value: Expression::Function(FunctionExpr {
                    name: None,
                    params,
                    body,
                    is_async: false,
                    is_generator: false,
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
        self.advance()?;
        let is_generator = self.eat_star()?;
        let name = if let Some(n) = self.current_identifier_name() {
            self.advance()?;
            Some(n)
        } else {
            None
        };
        let params = self.parse_formal_parameters()?;
        let (body, body_strict) = self.parse_function_body_with_context(is_generator, false)?;
        if body_strict {
            self.check_duplicate_params_strict(&params)?;
        }
        Ok(Expression::Function(FunctionExpr {
            name,
            params,
            body,
            is_async: false,
            is_generator,
        }))
    }

    fn parse_class_expression(&mut self) -> Result<Expression, ParseError> {
        self.advance()?; // class
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
        let body = self.parse_class_body()?;
        Ok(Expression::Class(ClassExpr {
            name,
            super_class,
            body,
        }))
    }

    fn parse_template_literal_expr(&mut self) -> Result<TemplateLiteral, ParseError> {
        match &self.current {
            Token::NoSubstitutionTemplate(cooked, _raw) => {
                let cooked = cooked.clone();
                // Non-tagged templates require valid cooked values
                let s = cooked
                    .ok_or_else(|| self.error("Invalid escape sequence in template literal"))?;
                self.advance()?;
                Ok(TemplateLiteral {
                    quasis: vec![s],
                    expressions: Vec::new(),
                })
            }
            Token::TemplateHead(cooked, _raw) => {
                let cooked = cooked.clone();
                let s = cooked
                    .ok_or_else(|| self.error("Invalid escape sequence in template literal"))?;
                let mut quasis = vec![s];
                let mut expressions = Vec::new();
                self.advance()?;
                loop {
                    expressions.push(self.parse_expression()?);
                    let tok = self.lexer.read_template_continuation()?;
                    match tok {
                        Token::TemplateTail(cooked, _raw) => {
                            let s = cooked.ok_or_else(|| {
                                self.error("Invalid escape sequence in template literal")
                            })?;
                            quasis.push(s);
                            self.advance()?;
                            break;
                        }
                        Token::TemplateMiddle(cooked, _raw) => {
                            let s = cooked.ok_or_else(|| {
                                self.error("Invalid escape sequence in template literal")
                            })?;
                            quasis.push(s);
                            self.advance()?;
                        }
                        _ => return Err(self.error("Expected template continuation")),
                    }
                }
                Ok(TemplateLiteral {
                    quasis,
                    expressions,
                })
            }
            _ => Err(self.error("Expected template literal")),
        }
    }
}

fn expr_to_pattern(expr: Expression) -> Result<Pattern, ParseError> {
    match expr {
        Expression::Identifier(name) => Ok(Pattern::Identifier(name)),
        Expression::Assign(AssignOp::Assign, left, right) => {
            let pat = expr_to_pattern(*left)?;
            Ok(Pattern::Assign(Box::new(pat), right))
        }
        Expression::Array(elements) => {
            let pats = elements
                .into_iter()
                .map(|e| {
                    e.map(|e| {
                        if let Expression::Spread(inner) = e {
                            expr_to_pattern(*inner).map(ArrayPatternElement::Rest)
                        } else {
                            expr_to_pattern(e).map(ArrayPatternElement::Pattern)
                        }
                    })
                    .transpose()
                })
                .collect::<Result<_, _>>()?;
            Ok(Pattern::Array(pats))
        }
        Expression::Object(props) => {
            let mut pat_props = Vec::new();
            for prop in props {
                if let PropertyKind::Init = prop.kind {
                    if let Expression::Spread(inner) = prop.value {
                        let pat = expr_to_pattern(*inner)?;
                        pat_props.push(ObjectPatternProperty::Rest(pat));
                    } else if prop.shorthand {
                        if let PropertyKey::Identifier(ref name) = prop.key {
                            if let Expression::Assign(AssignOp::Assign, left, right) = prop.value {
                                let pat = expr_to_pattern(*left)?;
                                pat_props.push(ObjectPatternProperty::KeyValue(
                                    prop.key,
                                    Pattern::Assign(Box::new(pat), right),
                                ));
                            } else {
                                pat_props.push(ObjectPatternProperty::Shorthand(name.clone()));
                            }
                        } else {
                            return Err(ParseError {
                                message: "Invalid destructuring target".to_string(),
                            });
                        }
                    } else {
                        let val_pat = expr_to_pattern(prop.value)?;
                        pat_props.push(ObjectPatternProperty::KeyValue(prop.key, val_pat));
                    }
                } else {
                    return Err(ParseError {
                        message: "Invalid destructuring target".to_string(),
                    });
                }
            }
            Ok(Pattern::Object(pat_props))
        }
        Expression::Spread(inner) => {
            let pat = expr_to_pattern(*inner)?;
            Ok(Pattern::Rest(Box::new(pat)))
        }
        _ => Err(ParseError {
            message: "Invalid destructuring target".to_string(),
        }),
    }
}

fn pattern_to_expr(pat: Pattern) -> Expression {
    match pat {
        Pattern::Identifier(name) => Expression::Identifier(name),
        Pattern::Rest(inner) => Expression::Spread(Box::new(pattern_to_expr(*inner))),
        _ => Expression::Identifier("_".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Program {
        Parser::new(src).unwrap().parse_program().unwrap()
    }

    #[test]
    fn parse_empty() {
        let prog = parse("");
        assert!(prog.body.is_empty());
    }

    #[test]
    fn parse_var_declaration() {
        let prog = parse("var x = 42;");
        assert_eq!(prog.body.len(), 1);
        assert!(matches!(&prog.body[0], Statement::Variable(_)));
    }

    #[test]
    fn parse_if_statement() {
        let prog = parse("if (true) { x; } else { y; }");
        assert!(matches!(&prog.body[0], Statement::If(_)));
    }

    #[test]
    fn parse_function_declaration() {
        let prog = parse("function foo(a, b) { return a + b; }");
        assert!(matches!(&prog.body[0], Statement::FunctionDeclaration(_)));
    }

    #[test]
    fn parse_expression_statement() {
        let prog = parse("1 + 2 * 3;");
        assert!(matches!(&prog.body[0], Statement::Expression(_)));
    }

    #[test]
    fn parse_for_loop() {
        let prog = parse("for (var i = 0; i < 10; i++) { x; }");
        assert!(matches!(&prog.body[0], Statement::For(_)));
    }

    #[test]
    fn parse_arrow_function() {
        let prog = parse("var f = (a, b) => a + b;");
        assert!(matches!(&prog.body[0], Statement::Variable(_)));
    }

    #[test]
    fn parse_try_catch() {
        let prog = parse("try { x; } catch (e) { y; } finally { z; }");
        assert!(matches!(&prog.body[0], Statement::Try(_)));
    }

    #[test]
    fn parse_class() {
        let prog = parse("class Foo extends Bar { constructor() {} }");
        assert!(matches!(&prog.body[0], Statement::ClassDeclaration(_)));
    }
}
