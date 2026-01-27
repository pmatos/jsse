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

    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut body = Vec::new();
        while self.current != Token::Eof {
            body.push(self.parse_statement_or_declaration()?);
        }
        Ok(Program { body })
    }

    fn parse_statement_or_declaration(&mut self) -> Result<Statement, ParseError> {
        match &self.current {
            Token::Keyword(Keyword::Function) => self.parse_function_declaration(),
            Token::Keyword(Keyword::Class) => self.parse_class_declaration(),
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
        match &self.current {
            Token::LeftBrace => self.parse_block_statement(),
            Token::Semicolon => {
                self.advance()?;
                Ok(Statement::Empty)
            }
            Token::Keyword(Keyword::Var) => self.parse_variable_statement(),
            Token::Keyword(Keyword::Let) => self.parse_lexical_declaration(),
            Token::Keyword(Keyword::Const) => self.parse_lexical_declaration(),
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
        if let Token::Identifier(name) = &self.current {
            let name = name.clone();
            let ident_lt = self.prev_line_terminator;
            self.advance()?;
            if self.current == Token::Colon {
                self.advance()?;
                let stmt = self.parse_statement()?;
                return Ok(Statement::Labeled(name, Box::new(stmt)));
            }
            // Not a label — push back current and restore identifier
            let after_tok = std::mem::replace(&mut self.current, Token::Identifier(name));
            let after_lt = std::mem::replace(&mut self.prev_line_terminator, ident_lt);
            self.pushback = Some((after_tok, after_lt));
        }
        self.parse_expression_statement()
    }

    fn parse_block_statement(&mut self) -> Result<Statement, ParseError> {
        self.eat(&Token::LeftBrace)?;
        let mut stmts = Vec::new();
        while self.current != Token::RightBrace && self.current != Token::Eof {
            stmts.push(self.parse_statement_or_declaration()?);
        }
        self.eat(&Token::RightBrace)?;
        Ok(Statement::Block(stmts))
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
        match &self.current {
            Token::Identifier(name) => {
                let name = name.clone();
                self.advance()?;
                Ok(Pattern::Identifier(name))
            }
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
            if let Token::Identifier(name) = &self.current {
                let name = name.clone();
                self.advance()?;
                if self.current == Token::Colon {
                    self.advance()?;
                    let pat = self.parse_binding_pattern()?;
                    props.push(ObjectPatternProperty::KeyValue(
                        PropertyKey::Identifier(name),
                        pat,
                    ));
                } else {
                    props.push(ObjectPatternProperty::Shorthand(name));
                }
            } else {
                return Err(self.error("Expected property name in object pattern"));
            }
            if self.current == Token::Comma {
                self.advance()?;
            }
        }
        self.eat(&Token::RightBrace)?;
        Ok(Pattern::Object(props))
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

    fn parse_while_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // while
        self.eat(&Token::LeftParen)?;
        let test = self.parse_expression()?;
        self.eat(&Token::RightParen)?;
        let body = Box::new(self.parse_statement()?);
        Ok(Statement::While(WhileStatement { test, body }))
    }

    fn parse_do_while_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // do
        let body = Box::new(self.parse_statement()?);
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
                    let body = Box::new(self.parse_statement()?);
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
                    let body = Box::new(self.parse_statement()?);
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
                    let body = Box::new(self.parse_statement()?);
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
                    let body = Box::new(self.parse_statement()?);
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
                    let body = Box::new(self.parse_statement()?);
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
                    let body = Box::new(self.parse_statement()?);
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
        let body = Box::new(self.parse_statement()?);
        Ok(Statement::For(ForStatement {
            init,
            test,
            update,
            body,
        }))
    }

    fn parse_return_statement(&mut self) -> Result<Statement, ParseError> {
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
        self.advance()?; // break
        let label = if !self.prev_line_terminator {
            if let Token::Identifier(name) = &self.current {
                let name = name.clone();
                self.advance()?;
                Some(name)
            } else {
                None
            }
        } else {
            None
        };
        self.eat_semicolon()?;
        Ok(Statement::Break(label))
    }

    fn parse_continue_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?;
        let label = if !self.prev_line_terminator {
            if let Token::Identifier(name) = &self.current {
                let name = name.clone();
                self.advance()?;
                Some(name)
            } else {
                None
            }
        } else {
            None
        };
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
        let mut cases = Vec::new();
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
                consequent.push(self.parse_statement_or_declaration()?);
            }
            cases.push(SwitchCase { test, consequent });
        }
        self.eat(&Token::RightBrace)?;
        Ok(Statement::Switch(SwitchStatement {
            discriminant,
            cases,
        }))
    }

    fn parse_with_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance()?; // with
        self.eat(&Token::LeftParen)?;
        let expr = self.parse_expression()?;
        self.eat(&Token::RightParen)?;
        let body = self.parse_statement()?;
        Ok(Statement::With(expr, Box::new(body)))
    }

    fn parse_function_declaration(&mut self) -> Result<Statement, ParseError> {
        let is_async = if self.current == Token::Keyword(Keyword::Async) {
            self.advance()?;
            true
        } else {
            false
        };
        self.eat(&Token::Keyword(Keyword::Function))?;
        let is_generator = if self.current == Token::Star {
            self.advance()?;
            true
        } else {
            false
        };
        let name = match &self.current {
            Token::Identifier(n) => {
                let n = n.clone();
                self.advance()?;
                n
            }
            _ => return Err(self.error("Expected function name")),
        };
        let params = self.parse_formal_parameters()?;
        let body = self.parse_function_body()?;
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
        let name = match &self.current {
            Token::Identifier(n) => {
                let n = n.clone();
                self.advance()?;
                n
            }
            _ => return Err(self.error("Expected class name")),
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
        let mut elements = Vec::new();
        while self.current != Token::RightBrace {
            if self.current == Token::Semicolon {
                self.advance()?;
                continue;
            }
            elements.push(self.parse_class_element()?);
        }
        self.eat(&Token::RightBrace)?;
        Ok(elements)
    }

    fn parse_class_element(&mut self) -> Result<ClassElement, ParseError> {
        let is_static = if self.current == Token::Keyword(Keyword::Static) {
            self.advance()?;
            if self.current == Token::LeftBrace {
                // static block
                self.eat(&Token::LeftBrace)?;
                let mut stmts = Vec::new();
                while self.current != Token::RightBrace {
                    stmts.push(self.parse_statement_or_declaration()?);
                }
                self.eat(&Token::RightBrace)?;
                return Ok(ClassElement::StaticBlock(stmts));
            }
            true
        } else {
            false
        };

        let kind = match &self.current {
            Token::Identifier(n) if n == "get" => {
                self.advance()?;
                if self.current == Token::LeftParen {
                    // it's a method called "get"
                    let key = PropertyKey::Identifier("get".to_string());
                    let func = self.parse_method_function(false, false)?;
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
                    let func = self.parse_method_function(false, false)?;
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

        let is_generator = if self.current == Token::Star {
            self.advance()?;
            true
        } else {
            false
        };

        let (key, computed) = self.parse_property_name()?;
        let is_constructor = !is_static
            && kind == ClassMethodKind::Method
            && matches!(&key, PropertyKey::Identifier(n) if n == "constructor");

        if self.current == Token::LeftParen {
            let func = self.parse_method_function(false, is_generator)?;
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
        } else if let Token::NumericLiteral(n) = &self.current {
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

    fn parse_method_function(
        &mut self,
        is_async: bool,
        is_generator: bool,
    ) -> Result<FunctionExpr, ParseError> {
        let params = self.parse_formal_parameters()?;
        let body = self.parse_function_body()?;
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

    fn parse_function_body(&mut self) -> Result<Vec<Statement>, ParseError> {
        self.eat(&Token::LeftBrace)?;
        let mut stmts = Vec::new();
        while self.current != Token::RightBrace {
            stmts.push(self.parse_statement_or_declaration()?);
        }
        self.eat(&Token::RightBrace)?;
        Ok(stmts)
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

    fn parse_assignment_expression(&mut self) -> Result<Expression, ParseError> {
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
            Token::Increment => {
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Update(
                    UpdateOp::Increment,
                    true,
                    Box::new(expr),
                ))
            }
            Token::Decrement => {
                self.advance()?;
                let expr = self.parse_unary()?;
                Ok(Expression::Update(
                    UpdateOp::Decrement,
                    true,
                    Box::new(expr),
                ))
            }
            Token::Keyword(Keyword::Await) => {
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
            if self.current == Token::Increment {
                self.advance()?;
                return Ok(Expression::Update(
                    UpdateOp::Increment,
                    false,
                    Box::new(expr),
                ));
            }
            if self.current == Token::Decrement {
                self.advance()?;
                return Ok(Expression::Update(
                    UpdateOp::Decrement,
                    false,
                    Box::new(expr),
                ));
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
                Token::NoSubstitutionTemplate(_) | Token::TemplateHead(_) => {
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
            Token::Identifier(name) => {
                let name = name.clone();
                self.advance()?;
                // Arrow function: (ident) => or ident =>
                if self.current == Token::Arrow && !self.prev_line_terminator {
                    self.advance()?;
                    let body = if self.current == Token::LeftBrace {
                        let stmts = self.parse_function_body()?;
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
            Token::LeftParen => {
                self.advance()?;
                // Could be: parenthesized expression, arrow params, or empty arrow ()=>
                if self.current == Token::RightParen {
                    // () => ...
                    self.advance()?;
                    if self.current == Token::Arrow {
                        self.advance()?;
                        let body = if self.current == Token::LeftBrace {
                            ArrowBody::Block(self.parse_function_body()?)
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
                        ArrowBody::Block(self.parse_function_body()?)
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
                            ArrowBody::Block(self.parse_function_body()?)
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
            Token::NoSubstitutionTemplate(_) | Token::TemplateHead(_) => {
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
        // Check for get/set
        if let Token::Identifier(n) = &self.current
            && (n == "get" || n == "set")
            && self.peek_is_property_name()
        {
            let kind = if n == "get" {
                PropertyKind::Get
            } else {
                PropertyKind::Set
            };
            self.advance()?;
            let (key, computed) = self.parse_property_name()?;
            let params = self.parse_formal_parameters()?;
            let body = self.parse_function_body()?;
            return Ok(Property {
                key,
                value: Expression::Function(FunctionExpr {
                    name: None,
                    params,
                    body,
                    is_async: false,
                    is_generator: false,
                }),
                kind,
                computed,
                shorthand: false,
            });
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
            let body = self.parse_function_body()?;
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

    fn peek_is_property_name(&self) -> bool {
        matches!(
            &self.current,
            Token::Identifier(_)
                | Token::StringLiteral(_)
                | Token::NumericLiteral(_)
                | Token::LeftBracket
                | Token::Keyword(_)
        )
    }

    fn parse_function_expression(&mut self) -> Result<Expression, ParseError> {
        self.advance()?; // function
        let is_generator = if self.current == Token::Star {
            self.advance()?;
            true
        } else {
            false
        };
        let name = if let Token::Identifier(n) = &self.current {
            let n = n.clone();
            self.advance()?;
            Some(n)
        } else {
            None
        };
        let params = self.parse_formal_parameters()?;
        let body = self.parse_function_body()?;
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
        let name = if let Token::Identifier(n) = &self.current {
            let n = n.clone();
            self.advance()?;
            Some(n)
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
            Token::NoSubstitutionTemplate(s) => {
                let s = s.clone();
                self.advance()?;
                Ok(TemplateLiteral {
                    quasis: vec![s],
                    expressions: Vec::new(),
                })
            }
            Token::TemplateHead(s) => {
                let mut quasis = vec![s.clone()];
                let mut expressions = Vec::new();
                self.advance()?;
                loop {
                    expressions.push(self.parse_expression()?);
                    // Read template continuation
                    let tok = self.lexer.read_template_continuation()?;
                    match tok {
                        Token::TemplateTail(s) => {
                            quasis.push(s);
                            // Advance past the consumed template
                            self.prev_line_terminator = false;
                            loop {
                                let t = self.lexer.next_token()?;
                                if t == Token::LineTerminator {
                                    self.prev_line_terminator = true;
                                    continue;
                                }
                                self.current = t;
                                break;
                            }
                            break;
                        }
                        Token::TemplateMiddle(s) => {
                            quasis.push(s);
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
