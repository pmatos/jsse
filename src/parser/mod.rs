use crate::ast::*;
use crate::lexer::{Keyword, LexError, Lexer, Token};
use std::fmt;

mod declarations;
mod expressions;
mod modules;
mod statements;

#[derive(Clone, Copy, PartialEq)]
enum PrivateNameKind {
    Getter,
    Setter,
    Other,
}

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
    source: &'a str,
    lexer: Lexer<'a>,
    current: Token,
    current_token_start: usize,
    current_token_end: usize,
    prev_token_end: usize,
    prev_line_terminator: bool,
    pushback: Option<(Token, bool, usize, usize)>, // (token, had_line_terminator_before, token_start, token_end)
    strict: bool,
    is_module: bool,
    in_function: u32,
    in_non_arrow_function: u32,
    in_generator: bool,
    in_async: bool,
    in_iteration: u32,
    in_switch: u32,
    labels: Vec<(String, bool)>, // (name, is_iteration)
    allow_super_property: bool,
    allow_super_call: bool,
    in_formal_parameters: bool,
    in_block_or_function: bool,
    in_switch_case: bool,
    no_in: bool,
    pub last_string_literal_has_escape: bool,
    pub last_string_literal_has_legacy_octal: bool,
    private_name_scopes: Vec<(std::collections::HashSet<String>, Vec<(String, usize)>)>,
    in_field_initializer_eval: bool,
    in_static_block: bool,
    function_param_names: Option<std::collections::HashSet<String>>,
    eval_new_target_allowed: bool,
    last_expr_parenthesized: bool,
    last_obj_had_proto_dup: bool,
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
        let token_start = lexer.token_start();
        let token_end = lexer.offset();
        Ok(Self {
            source,
            lexer,
            current,
            current_token_start: token_start,
            current_token_end: token_end,
            prev_token_end: 0,
            prev_line_terminator: had_lt,
            pushback: None,
            strict: false,
            is_module: false,
            in_function: 0,
            in_non_arrow_function: 0,
            in_generator: false,
            in_async: false,
            in_iteration: 0,
            in_switch: 0,
            labels: Vec::new(),
            allow_super_property: false,
            allow_super_call: false,
            in_formal_parameters: false,
            in_block_or_function: false,
            in_switch_case: false,
            no_in: false,
            last_string_literal_has_escape: false,
            last_string_literal_has_legacy_octal: false,
            private_name_scopes: Vec::new(),
            in_field_initializer_eval: false,
            in_static_block: false,
            function_param_names: None,
            eval_new_target_allowed: false,
            last_expr_parenthesized: false,
            last_obj_had_proto_dup: false,
        })
    }

    fn advance(&mut self) -> Result<Token, ParseError> {
        self.prev_token_end = self.current_token_end;
        let old = std::mem::replace(&mut self.current, Token::Eof);
        if let Some((tok, lt, ts, te)) = self.pushback.take() {
            self.current = tok;
            self.prev_line_terminator = lt;
            self.current_token_start = ts;
            self.current_token_end = te;
        } else {
            self.prev_line_terminator = false;
            loop {
                let tok = self.lexer.next_token()?;
                if tok == Token::LineTerminator {
                    self.prev_line_terminator = true;
                    continue;
                }
                self.current_token_start = self.lexer.token_start();
                self.current_token_end = self.lexer.offset();
                self.current = tok;
                break;
            }
        }
        Ok(old)
    }

    fn push_back(&mut self, token: Token, had_lt: bool) {
        let old_current = std::mem::replace(&mut self.current, token);
        let old_lt = std::mem::replace(&mut self.prev_line_terminator, had_lt);
        let old_ts = self.current_token_start;
        let old_te = self.current_token_end;
        self.pushback = Some((old_current, old_lt, old_ts, old_te));
    }

    #[allow(dead_code)]
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

    fn source_since(&self, start: usize) -> String {
        self.source[start..self.prev_token_end].to_string()
    }

    pub fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
        self.lexer.strict = strict;
    }

    pub fn set_eval_in_class_with_names(&mut self, names: std::collections::HashSet<String>) {
        self.private_name_scopes.push((names, Vec::new()));
    }

    pub fn set_eval_in_field_initializer(&mut self) {
        self.in_field_initializer_eval = true;
        self.allow_super_property = true;
        self.in_function += 1;
        self.in_non_arrow_function += 1;
    }

    pub fn set_eval_new_target_allowed(&mut self) {
        self.eval_new_target_allowed = true;
    }

    pub fn set_eval_allow_super_property(&mut self) {
        self.allow_super_property = true;
    }

    #[allow(dead_code)]
    pub fn set_eval_allow_super_call(&mut self) {
        self.allow_super_call = true;
    }

    fn push_private_scope(&mut self) {
        self.private_name_scopes
            .push((std::collections::HashSet::new(), Vec::new()));
    }

    fn declare_private_name(&mut self, name: &str) {
        if let Some((declared, _)) = self.private_name_scopes.last_mut() {
            declared.insert(name.to_string());
        }
    }

    pub(super) fn use_private_name(&mut self, name: &str) -> Result<(), ParseError> {
        if name == "constructor" {
            return Err(self.error("#constructor is not a valid private name"));
        }
        if self.private_name_scopes.is_empty() {
            return Err(self.error(format!(
                "Reference to undeclared private field or method '#{name}'"
            )));
        }
        if let Some((_, used)) = self.private_name_scopes.last_mut() {
            used.push((name.to_string(), self.current_token_start));
        }
        Ok(())
    }

    pub fn validate_eval_private_names(&mut self) -> Result<(), ParseError> {
        while !self.private_name_scopes.is_empty() {
            self.pop_private_scope()?;
        }
        Ok(())
    }

    fn pop_private_scope(&mut self) -> Result<(), ParseError> {
        if let Some((declared, used)) = self.private_name_scopes.pop() {
            for (name, _pos) in &used {
                let mut found = declared.contains(name);
                if !found {
                    for (parent_declared, _) in self.private_name_scopes.iter().rev() {
                        if parent_declared.contains(name) {
                            found = true;
                            break;
                        }
                    }
                }
                if !found {
                    return Err(self.error(format!(
                        "Reference to undeclared private field or method '#{name}'"
                    )));
                }
            }
            Ok(())
        } else {
            Ok(())
        }
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
        if !self.prev_line_terminator
            && let Some(name) = self.current_identifier_name()
        {
            self.advance()?;
            return Ok(Some(name));
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
                "implements"
                    | "interface"
                    | "let"
                    | "package"
                    | "private"
                    | "protected"
                    | "public"
                    | "static"
                    | "yield"
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
            Token::IdentifierWithEscape(name) => {
                if Self::is_reserved_identifier(name, self.strict)
                    || (name == "yield" && (self.in_generator || self.strict))
                    || (name == "await"
                        && (self.in_async || self.in_static_block || self.is_module))
                {
                    None
                } else {
                    Some(name.clone())
                }
            }
            Token::Keyword(Keyword::Yield) if !self.in_generator && !self.strict => {
                Some("yield".to_string())
            }
            Token::Keyword(Keyword::Await)
                if !self.in_async && !self.in_static_block && !self.is_module =>
            {
                Some("await".to_string())
            }
            Token::Keyword(Keyword::Let) if !self.strict => Some("let".to_string()),
            Token::Keyword(Keyword::Static) if !self.strict => Some("static".to_string()),
            Token::Keyword(Keyword::Async) => Some("async".to_string()),
            Token::Keyword(Keyword::Of) => Some("of".to_string()),
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
            Pattern::MemberExpression(_) => {}
        }
    }

    fn is_simple_parameter_list(params: &[Pattern]) -> bool {
        params.iter().all(|p| matches!(p, Pattern::Identifier(_)))
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

    fn check_strict_params(&self, params: &[Pattern]) -> Result<(), ParseError> {
        let mut names = Vec::new();
        for p in params {
            Self::collect_bound_names(p, &mut names);
        }
        for name in &names {
            if name == "eval" || name == "arguments" {
                return Err(self.error(format!(
                    "'{}' can't be used as a parameter name in strict mode",
                    name
                )));
            }
            if Self::is_strict_reserved_word(name) {
                return Err(self.error(format!("Unexpected strict mode reserved word '{name}'")));
            }
        }
        Ok(())
    }

    pub(super) fn check_strict_assignment_pattern(&self, pat: &Pattern) -> Result<(), ParseError> {
        if !self.strict {
            return Ok(());
        }
        match pat {
            Pattern::Identifier(name) => {
                if name == "eval" || name == "arguments" {
                    return Err(self.error(&format!(
                        "Assignment to '{name}' in strict mode"
                    )));
                }
            }
            Pattern::Array(elems) => {
                for elem in elems.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                            self.check_strict_assignment_pattern(p)?;
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                            self.check_strict_assignment_pattern(p)?;
                        }
                        ObjectPatternProperty::Shorthand(name) => {
                            if name == "eval" || name == "arguments" {
                                return Err(self.error(&format!(
                                    "Assignment to '{name}' in strict mode"
                                )));
                            }
                        }
                    }
                }
            }
            Pattern::Assign(p, _) => {
                self.check_strict_assignment_pattern(p)?;
            }
            Pattern::Rest(p) => {
                self.check_strict_assignment_pattern(p)?;
            }
            Pattern::MemberExpression(_) => {}
        }
        Ok(())
    }

    fn check_strict_binding_identifier(&self, name: &str) -> Result<(), ParseError> {
        self.check_strict_identifier(name)?;
        if self.strict && (name == "eval" || name == "arguments") {
            return Err(self.error(format!(
                "'{}' can't be used as a binding identifier in strict mode",
                name
            )));
        }
        Ok(())
    }

    fn contains_arguments(expr: &Expression) -> bool {
        use crate::ast::{ArrowBody, Expression, MemberProperty, Property};
        match expr {
            Expression::Identifier(name) => name == "arguments",
            Expression::Array(elems, _) => elems
                .iter()
                .any(|e| e.as_ref().is_some_and(Self::contains_arguments)),
            Expression::Object(props) => props.iter().any(|p: &Property| {
                Self::contains_arguments(&p.value)
                    || matches!(&p.key, crate::ast::PropertyKey::Computed(e) if Self::contains_arguments(e))
            }),
            Expression::Member(object, property) => {
                Self::contains_arguments(object)
                    || matches!(property, MemberProperty::Computed(e) if Self::contains_arguments(e))
            }
            Expression::Call(callee, args) | Expression::New(callee, args) => {
                Self::contains_arguments(callee)
                    || args.iter().any(Self::contains_arguments)
            }
            Expression::Binary(_, left, right)
            | Expression::Logical(_, left, right)
            | Expression::Assign(_, left, right) => {
                Self::contains_arguments(left) || Self::contains_arguments(right)
            }
            Expression::Unary(_, operand) | Expression::Update(_, _, operand) => {
                Self::contains_arguments(operand)
            }
            Expression::Conditional(test, consequent, alternate) => {
                Self::contains_arguments(test)
                    || Self::contains_arguments(consequent)
                    || Self::contains_arguments(alternate)
            }
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                exprs.iter().any(Self::contains_arguments)
            }
            Expression::Import(inner, opts)
            | Expression::ImportDefer(inner, opts)
            | Expression::ImportSource(inner, opts) => {
                Self::contains_arguments(inner)
                    || opts.as_ref().is_some_and(|e| Self::contains_arguments(e))
            }
            Expression::Spread(inner)
            | Expression::Await(inner) => Self::contains_arguments(inner),
            Expression::Yield(opt_e, _) => {
                opt_e.as_ref().is_some_and(|e| Self::contains_arguments(e))
            }
            // Arrow functions don't create their own arguments binding,
            // so references inside them still refer to the enclosing scope's arguments
            Expression::ArrowFunction(af) => match &af.body {
                ArrowBody::Expression(e) => Self::contains_arguments(e),
                ArrowBody::Block(stmts) => Self::stmts_contain_arguments(stmts),
            },
            Expression::Template(tl) => {
                tl.expressions.iter().any(Self::contains_arguments)
            }
            Expression::TaggedTemplate(tag, tl) => {
                Self::contains_arguments(tag)
                    || tl.expressions.iter().any(Self::contains_arguments)
            }
            Expression::Typeof(e) | Expression::Void(e) | Expression::Delete(e) => {
                Self::contains_arguments(e)
            }
            Expression::OptionalChain(object, chain) => {
                Self::contains_arguments(object) || Self::contains_arguments(chain)
            }
            // Class expressions: recurse into computed property names and field initializers
            // (they are evaluated in the enclosing scope), but NOT method bodies
            Expression::Class(cls) => {
                if let Some(ref sc) = cls.super_class {
                    if Self::contains_arguments(sc) {
                        return true;
                    }
                }
                Self::class_elements_contain_arguments(&cls.body)
            }
            // Functions/classes create their own scope, don't recurse
            Expression::Literal(_)
            | Expression::This
            | Expression::Super
            | Expression::NewTarget
            | Expression::ImportMeta
            | Expression::Function(_)
            | Expression::PrivateIdentifier(_) => false,
        }
    }

    fn class_elements_contain_arguments(body: &[crate::ast::ClassElement]) -> bool {
        for elem in body {
            match elem {
                crate::ast::ClassElement::Method(m) => {
                    if let crate::ast::PropertyKey::Computed(e) = &m.key {
                        if Self::contains_arguments(e) {
                            return true;
                        }
                    }
                }
                crate::ast::ClassElement::Property(p) => {
                    if let crate::ast::PropertyKey::Computed(e) = &p.key {
                        if Self::contains_arguments(e) {
                            return true;
                        }
                    }
                }
                crate::ast::ClassElement::StaticBlock(_) => {}
            }
        }
        false
    }

    fn stmts_contain_arguments(stmts: &[Statement]) -> bool {
        stmts.iter().any(Self::stmt_contains_arguments)
    }

    fn stmt_contains_arguments(stmt: &Statement) -> bool {
        use crate::ast::Statement;
        match stmt {
            Statement::Expression(e) | Statement::Throw(e) => Self::contains_arguments(e),
            Statement::Return(Some(e)) => Self::contains_arguments(e),
            Statement::Return(None) | Statement::Empty | Statement::Debugger => false,
            Statement::Block(stmts) => Self::stmts_contain_arguments(stmts),
            Statement::Variable(decl) => decl
                .declarations
                .iter()
                .any(|d| d.init.as_ref().is_some_and(Self::contains_arguments)),
            Statement::If(i) => {
                Self::contains_arguments(&i.test)
                    || Self::stmt_contains_arguments(&i.consequent)
                    || i.alternate
                        .as_ref()
                        .is_some_and(|a| Self::stmt_contains_arguments(a))
            }
            Statement::While(w) => {
                Self::contains_arguments(&w.test) || Self::stmt_contains_arguments(&w.body)
            }
            Statement::DoWhile(d) => {
                Self::contains_arguments(&d.test) || Self::stmt_contains_arguments(&d.body)
            }
            Statement::For(f) => {
                f.init.as_ref().is_some_and(|i| match i {
                    crate::ast::ForInit::Expression(e) => Self::contains_arguments(e),
                    crate::ast::ForInit::Variable(d) => d
                        .declarations
                        .iter()
                        .any(|dd| dd.init.as_ref().is_some_and(Self::contains_arguments)),
                }) || f.test.as_ref().is_some_and(Self::contains_arguments)
                    || f.update.as_ref().is_some_and(Self::contains_arguments)
                    || Self::stmt_contains_arguments(&f.body)
            }
            Statement::ForIn(f) => {
                Self::contains_arguments(&f.right) || Self::stmt_contains_arguments(&f.body)
            }
            Statement::ForOf(f) => {
                Self::contains_arguments(&f.right) || Self::stmt_contains_arguments(&f.body)
            }
            Statement::Try(t) => {
                Self::stmts_contain_arguments(&t.block)
                    || t.handler
                        .as_ref()
                        .is_some_and(|h| Self::stmts_contain_arguments(&h.body))
                    || t.finalizer
                        .as_ref()
                        .is_some_and(|f| Self::stmts_contain_arguments(f))
            }
            Statement::Switch(s) => {
                Self::contains_arguments(&s.discriminant)
                    || s.cases.iter().any(|c| {
                        c.test.as_ref().is_some_and(Self::contains_arguments)
                            || Self::stmts_contain_arguments(&c.consequent)
                    })
            }
            Statement::Labeled(_, s) => Self::stmt_contains_arguments(s),
            Statement::With(e, s) => {
                Self::contains_arguments(e) || Self::stmt_contains_arguments(s)
            }
            Statement::Break(_) | Statement::Continue(_) => false,
            // Function declarations create their own scope
            Statement::FunctionDeclaration(_) => false,
            // Class declarations: check computed property names (evaluated in enclosing scope)
            Statement::ClassDeclaration(cls) => {
                if let Some(ref sc) = cls.super_class {
                    if Self::contains_arguments(sc) {
                        return true;
                    }
                }
                Self::class_elements_contain_arguments(&cls.body)
            }
        }
    }

    pub(super) fn expr_contains_await_identifier(expr: &Expression) -> bool {
        use crate::ast::{ArrowBody, Expression, MemberProperty, Property};
        match expr {
            Expression::Identifier(name) => name == "await",
            Expression::Array(elems, _) => elems
                .iter()
                .any(|e| e.as_ref().is_some_and(Self::expr_contains_await_identifier)),
            Expression::Object(props) => props.iter().any(|p: &Property| {
                Self::expr_contains_await_identifier(&p.value)
                    || matches!(&p.key, crate::ast::PropertyKey::Computed(e) if Self::expr_contains_await_identifier(e))
            }),
            Expression::Member(object, property) => {
                Self::expr_contains_await_identifier(object)
                    || matches!(property, MemberProperty::Computed(e) if Self::expr_contains_await_identifier(e))
            }
            Expression::Call(callee, args) | Expression::New(callee, args) => {
                Self::expr_contains_await_identifier(callee)
                    || args.iter().any(Self::expr_contains_await_identifier)
            }
            Expression::Binary(_, left, right)
            | Expression::Logical(_, left, right)
            | Expression::Assign(_, left, right) => {
                Self::expr_contains_await_identifier(left) || Self::expr_contains_await_identifier(right)
            }
            Expression::Unary(_, operand) | Expression::Update(_, _, operand) => {
                Self::expr_contains_await_identifier(operand)
            }
            Expression::Conditional(test, consequent, alternate) => {
                Self::expr_contains_await_identifier(test)
                    || Self::expr_contains_await_identifier(consequent)
                    || Self::expr_contains_await_identifier(alternate)
            }
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                exprs.iter().any(Self::expr_contains_await_identifier)
            }
            Expression::Import(inner, opts)
            | Expression::ImportDefer(inner, opts)
            | Expression::ImportSource(inner, opts) => {
                Self::expr_contains_await_identifier(inner)
                    || opts.as_ref().is_some_and(|e| Self::expr_contains_await_identifier(e))
            }
            Expression::Spread(inner)
            | Expression::Await(inner) => Self::expr_contains_await_identifier(inner),
            Expression::Yield(opt_e, _) => {
                opt_e.as_ref().is_some_and(|e| Self::expr_contains_await_identifier(e))
            }
            Expression::ArrowFunction(af) if !af.is_async => {
                af.params.iter().any(Self::pattern_contains_await_identifier)
                    || match &af.body {
                        ArrowBody::Expression(e) => Self::expr_contains_await_identifier(e),
                        ArrowBody::Block(_) => false,
                    }
            }
            Expression::Template(tl) => {
                tl.expressions.iter().any(Self::expr_contains_await_identifier)
            }
            Expression::TaggedTemplate(tag, tl) => {
                Self::expr_contains_await_identifier(tag)
                    || tl.expressions.iter().any(Self::expr_contains_await_identifier)
            }
            Expression::Typeof(e) | Expression::Void(e) | Expression::Delete(e) => {
                Self::expr_contains_await_identifier(e)
            }
            Expression::OptionalChain(object, chain) => {
                Self::expr_contains_await_identifier(object) || Self::expr_contains_await_identifier(chain)
            }
            Expression::Literal(_)
            | Expression::This
            | Expression::Super
            | Expression::NewTarget
            | Expression::ImportMeta
            | Expression::Function(_)
            | Expression::Class(_)
            | Expression::ArrowFunction(_)
            | Expression::PrivateIdentifier(_) => false,
        }
    }

    fn pattern_contains_await_identifier(pat: &Pattern) -> bool {
        match pat {
            Pattern::Identifier(name) => name == "await",
            Pattern::Array(elems) => elems.iter().any(|e| {
                e.as_ref().is_some_and(|elem| match elem {
                    ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                        Self::pattern_contains_await_identifier(p)
                    }
                })
            }),
            Pattern::Object(props) => props.iter().any(|prop| match prop {
                ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                    Self::pattern_contains_await_identifier(p)
                }
                ObjectPatternProperty::Shorthand(name) => name == "await",
            }),
            Pattern::Assign(p, default) => {
                Self::pattern_contains_await_identifier(p)
                    || Self::expr_contains_await_identifier(default)
            }
            Pattern::Rest(p) => Self::pattern_contains_await_identifier(p),
            Pattern::MemberExpression(e) => Self::expr_contains_await_identifier(e),
        }
    }

    pub(super) fn expr_contains_await_expression(expr: &Expression) -> bool {
        use crate::ast::{ArrowBody, Expression, MemberProperty, Property};
        match expr {
            Expression::Await(_) => true,
            Expression::Array(elems, _) => elems
                .iter()
                .any(|e| e.as_ref().is_some_and(Self::expr_contains_await_expression)),
            Expression::Object(props) => props.iter().any(|p: &Property| {
                Self::expr_contains_await_expression(&p.value)
                    || matches!(&p.key, crate::ast::PropertyKey::Computed(e) if Self::expr_contains_await_expression(e))
            }),
            Expression::Member(object, property) => {
                Self::expr_contains_await_expression(object)
                    || matches!(property, MemberProperty::Computed(e) if Self::expr_contains_await_expression(e))
            }
            Expression::Call(callee, args) | Expression::New(callee, args) => {
                Self::expr_contains_await_expression(callee)
                    || args.iter().any(Self::expr_contains_await_expression)
            }
            Expression::Binary(_, left, right)
            | Expression::Logical(_, left, right)
            | Expression::Assign(_, left, right) => {
                Self::expr_contains_await_expression(left) || Self::expr_contains_await_expression(right)
            }
            Expression::Unary(_, operand) | Expression::Update(_, _, operand) => {
                Self::expr_contains_await_expression(operand)
            }
            Expression::Conditional(test, consequent, alternate) => {
                Self::expr_contains_await_expression(test)
                    || Self::expr_contains_await_expression(consequent)
                    || Self::expr_contains_await_expression(alternate)
            }
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                exprs.iter().any(Self::expr_contains_await_expression)
            }
            Expression::Spread(inner) => Self::expr_contains_await_expression(inner),
            Expression::Yield(opt_e, _) => {
                opt_e.as_ref().is_some_and(|e| Self::expr_contains_await_expression(e))
            }
            Expression::ArrowFunction(af) if !af.is_async => {
                af.params.iter().any(Self::pattern_contains_await_expression)
                    || match &af.body {
                        ArrowBody::Expression(e) => Self::expr_contains_await_expression(e),
                        ArrowBody::Block(_) => false,
                    }
            }
            Expression::Template(tl) => {
                tl.expressions.iter().any(Self::expr_contains_await_expression)
            }
            Expression::TaggedTemplate(tag, tl) => {
                Self::expr_contains_await_expression(tag)
                    || tl.expressions.iter().any(Self::expr_contains_await_expression)
            }
            Expression::Typeof(e) | Expression::Void(e) | Expression::Delete(e) => {
                Self::expr_contains_await_expression(e)
            }
            Expression::OptionalChain(object, chain) => {
                Self::expr_contains_await_expression(object) || Self::expr_contains_await_expression(chain)
            }
            _ => false,
        }
    }

    fn pattern_contains_await_expression(pat: &Pattern) -> bool {
        match pat {
            Pattern::Identifier(_) => false,
            Pattern::Array(elems) => elems.iter().any(|e| {
                e.as_ref().is_some_and(|elem| match elem {
                    ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                        Self::pattern_contains_await_expression(p)
                    }
                })
            }),
            Pattern::Object(props) => props.iter().any(|prop| match prop {
                ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                    Self::pattern_contains_await_expression(p)
                }
                ObjectPatternProperty::Shorthand(_) => false,
            }),
            Pattern::Assign(p, default) => {
                Self::pattern_contains_await_expression(p)
                    || Self::expr_contains_await_expression(default)
            }
            Pattern::Rest(p) => Self::pattern_contains_await_expression(p),
            Pattern::MemberExpression(e) => Self::expr_contains_await_expression(e),
        }
    }

    pub(super) fn expr_contains_yield_expression(expr: &Expression) -> bool {
        use crate::ast::{ArrowBody, Expression, MemberProperty, Property};
        match expr {
            Expression::Yield(_, _) => true,
            Expression::Array(elems, _) => elems
                .iter()
                .any(|e| e.as_ref().is_some_and(Self::expr_contains_yield_expression)),
            Expression::Object(props) => props.iter().any(|p: &Property| {
                Self::expr_contains_yield_expression(&p.value)
                    || matches!(&p.key, crate::ast::PropertyKey::Computed(e) if Self::expr_contains_yield_expression(e))
            }),
            Expression::Member(object, property) => {
                Self::expr_contains_yield_expression(object)
                    || matches!(property, MemberProperty::Computed(e) if Self::expr_contains_yield_expression(e))
            }
            Expression::Call(callee, args) | Expression::New(callee, args) => {
                Self::expr_contains_yield_expression(callee)
                    || args.iter().any(Self::expr_contains_yield_expression)
            }
            Expression::Binary(_, left, right)
            | Expression::Logical(_, left, right)
            | Expression::Assign(_, left, right) => {
                Self::expr_contains_yield_expression(left) || Self::expr_contains_yield_expression(right)
            }
            Expression::Unary(_, operand) | Expression::Update(_, _, operand) => {
                Self::expr_contains_yield_expression(operand)
            }
            Expression::Conditional(test, consequent, alternate) => {
                Self::expr_contains_yield_expression(test)
                    || Self::expr_contains_yield_expression(consequent)
                    || Self::expr_contains_yield_expression(alternate)
            }
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                exprs.iter().any(Self::expr_contains_yield_expression)
            }
            Expression::Spread(inner) => Self::expr_contains_yield_expression(inner),
            Expression::Await(inner) => Self::expr_contains_yield_expression(inner),
            Expression::ArrowFunction(af) => {
                af.params.iter().any(Self::pattern_contains_yield_expression)
                    || match &af.body {
                        ArrowBody::Expression(e) => Self::expr_contains_yield_expression(e),
                        ArrowBody::Block(_) => false,
                    }
            }
            Expression::Template(tl) => {
                tl.expressions.iter().any(Self::expr_contains_yield_expression)
            }
            Expression::TaggedTemplate(tag, tl) => {
                Self::expr_contains_yield_expression(tag)
                    || tl.expressions.iter().any(Self::expr_contains_yield_expression)
            }
            Expression::Typeof(e) | Expression::Void(e) | Expression::Delete(e) => {
                Self::expr_contains_yield_expression(e)
            }
            Expression::OptionalChain(object, chain) => {
                Self::expr_contains_yield_expression(object) || Self::expr_contains_yield_expression(chain)
            }
            _ => false,
        }
    }

    fn pattern_contains_yield_expression(pat: &Pattern) -> bool {
        match pat {
            Pattern::Identifier(_) => false,
            Pattern::Array(elems) => elems.iter().any(|e| {
                e.as_ref().is_some_and(|elem| match elem {
                    ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                        Self::pattern_contains_yield_expression(p)
                    }
                })
            }),
            Pattern::Object(props) => props.iter().any(|prop| match prop {
                ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                    Self::pattern_contains_yield_expression(p)
                }
                ObjectPatternProperty::Shorthand(_) => false,
            }),
            Pattern::Assign(p, default) => {
                Self::pattern_contains_yield_expression(p)
                    || Self::expr_contains_yield_expression(default)
            }
            Pattern::Rest(p) => Self::pattern_contains_yield_expression(p),
            Pattern::MemberExpression(e) => Self::expr_contains_yield_expression(e),
        }
    }

    fn is_directive_prologue(&self, stmt: &Statement) -> Option<String> {
        match stmt {
            Statement::Expression(Expression::Literal(Literal::String(s))) => {
                if self.last_string_literal_has_escape {
                    None
                } else {
                    Some(String::from_utf16_lossy(s))
                }
            }
            _ => None,
        }
    }

    fn is_string_literal_statement(stmt: &Statement) -> bool {
        matches!(stmt, Statement::Expression(Expression::Literal(Literal::String(_))))
    }

    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut body = Vec::new();
        let mut in_directive_prologue = true;
        let mut body_is_strict = false;
        let mut prologue_had_legacy_octal = false;
        let mut lexical_names: Vec<String> = Vec::new();

        while self.current != Token::Eof {
            let stmt = self.parse_statement_or_declaration()?;

            if in_directive_prologue {
                if Self::is_string_literal_statement(&stmt) {
                    if let Some(directive) = self.is_directive_prologue(&stmt) {
                        if directive == "use strict" {
                            if prologue_had_legacy_octal {
                                return Err(self.error("Octal escape sequences are not allowed in strict mode"));
                            }
                            self.set_strict(true);
                            body_is_strict = true;
                        }
                    }
                    if self.last_string_literal_has_legacy_octal {
                        prologue_had_legacy_octal = true;
                    }
                } else {
                    in_directive_prologue = false;
                }
            }

            // §16.1.4: At script level, only let/const/class are lexically declared
            // (function declarations are var-scoped at script level)
            match &stmt {
                Statement::Variable(decl) if decl.kind != VarKind::Var => {
                    let new_names = Self::bound_names_from_decl(decl);
                    for name in &new_names {
                        if lexical_names.contains(name) {
                            return Err(self.error(&format!(
                                "Identifier '{name}' has already been declared"
                            )));
                        }
                    }
                    lexical_names.extend(new_names);
                }
                Statement::ClassDeclaration(cls) => {
                    let name = &cls.name;
                    if lexical_names.contains(name) {
                        return Err(self.error(&format!(
                            "Identifier '{name}' has already been declared"
                        )));
                    }
                    lexical_names.push(name.clone());
                }
                _ => {}
            }

            body.push(stmt);
        }

        // §16.1.1: VarDeclaredNames must not overlap LexicallyDeclaredNames
        if !lexical_names.is_empty() {
            let mut var_names = Vec::new();
            for stmt in &body {
                Self::collect_var_declared_names(stmt, &mut var_names);
            }
            for name in &var_names {
                if lexical_names.contains(name) {
                    return Err(self.error(&format!(
                        "Identifier '{name}' has already been declared"
                    )));
                }
            }
        }

        Ok(Program {
            source_type: SourceType::Script,
            body,
            module_items: Vec::new(),
            body_is_strict,
        })
    }

    pub fn parse_program_as_module(&mut self) -> Result<Program, ParseError> {
        self.is_module = true;
        self.lexer.is_module = true;
        self.set_strict(true);

        let mut module_items = Vec::new();
        let mut exported_names = std::collections::HashSet::new();

        while self.current != Token::Eof {
            let item = self.parse_module_item()?;

            // Check for duplicate exported names
            if let ModuleItem::ExportDeclaration(ref export) = item {
                for name in self.get_exported_names(export) {
                    if !exported_names.insert(name.clone()) {
                        return Err(self.error(format!("Duplicate export of '{}'", name)));
                    }
                }
            }

            module_items.push(item);
        }

        // §16.2.1.1 Module early errors
        self.validate_module_early_errors(&module_items)?;

        Ok(Program {
            source_type: SourceType::Module,
            body: Vec::new(),
            module_items,
            body_is_strict: true,
        })
    }

    fn get_exported_names(&self, export: &ExportDeclaration) -> Vec<String> {
        match export {
            ExportDeclaration::Named {
                specifiers,
                declaration,
                ..
            } => {
                let mut names = Vec::new();
                for spec in specifiers {
                    names.push(spec.exported.clone());
                }
                if let Some(decl) = declaration {
                    names.extend(self.get_declaration_export_names(decl));
                }
                names
            }
            ExportDeclaration::Default(_)
            | ExportDeclaration::DefaultFunction(_)
            | ExportDeclaration::DefaultClass(_) => {
                vec!["default".to_string()]
            }
            ExportDeclaration::All { exported, .. } => {
                // export * as ns from "mod" exports 'ns'
                // export * from "mod" doesn't add to local exported names
                exported.iter().cloned().collect()
            }
        }
    }

    fn get_declaration_export_names(&self, decl: &Statement) -> Vec<String> {
        match decl {
            Statement::Variable(var) => {
                let mut names = Vec::new();
                for d in &var.declarations {
                    self.collect_pattern_names(&d.pattern, &mut names);
                }
                names
            }
            Statement::FunctionDeclaration(f) => vec![f.name.clone()],
            Statement::ClassDeclaration(c) => vec![c.name.clone()],
            _ => vec![],
        }
    }

    fn collect_pattern_names(&self, pattern: &Pattern, names: &mut Vec<String>) {
        match pattern {
            Pattern::Identifier(name) => names.push(name.clone()),
            Pattern::Array(elements) => {
                for elem in elements.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) => {
                            self.collect_pattern_names(p, names);
                        }
                        ArrayPatternElement::Rest(p) => {
                            self.collect_pattern_names(p, names);
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(_, value) => {
                            self.collect_pattern_names(value, names);
                        }
                        ObjectPatternProperty::Shorthand(name) => {
                            names.push(name.clone());
                        }
                        ObjectPatternProperty::Rest(pat) => {
                            self.collect_pattern_names(pat, names);
                        }
                    }
                }
            }
            Pattern::Assign(inner, _) => {
                self.collect_pattern_names(inner, names);
            }
            Pattern::Rest(inner) => {
                self.collect_pattern_names(inner, names);
            }
            Pattern::MemberExpression(_) => {}
        }
    }

    /// §16.2.1.1 Static Semantics: Early Errors for Module
    fn validate_module_early_errors(&self, items: &[ModuleItem]) -> Result<(), ParseError> {
        use std::collections::HashSet;

        let mut lex_names: HashSet<String> = HashSet::new();
        let mut var_names: HashSet<String> = HashSet::new();
        let mut exported_bindings: HashSet<String> = HashSet::new();

        // Collect all lexically-declared and var-declared names
        for item in items {
            match item {
                ModuleItem::Statement(stmt) => {
                    self.collect_module_var_names(stmt, &mut var_names);
                    self.collect_module_lex_names(stmt, &mut lex_names)?;
                }
                ModuleItem::ExportDeclaration(export) => {
                    self.collect_export_var_names(export, &mut var_names);
                    self.collect_export_lex_names(export, &mut lex_names)?;
                    self.collect_exported_bindings(export, &mut exported_bindings);
                }
                ModuleItem::ImportDeclaration(import) => {
                    for spec in &import.specifiers {
                        let name = match spec {
                            ImportSpecifier::Default(n) => n,
                            ImportSpecifier::Named { local, .. } => local,
                            ImportSpecifier::Namespace(n) => n,
                            ImportSpecifier::DeferredNamespace(n) => n,
                        };
                        // §13.1.1: import binding cannot be "arguments" or "eval" (strict mode)
                        if name == "arguments" || name == "eval" {
                            return Err(self.error(format!(
                                "'{name}' cannot be used as an import binding in module code"
                            )));
                        }
                        if !lex_names.insert(name.clone()) {
                            return Err(
                                self.error(format!("Duplicate binding '{name}' in module scope"))
                            );
                        }
                    }
                }
            }
        }

        // Check: LexicallyDeclaredNames ∩ VarDeclaredNames must be empty
        for name in &lex_names {
            if var_names.contains(name) {
                return Err(self.error(format!("Identifier '{name}' has already been declared")));
            }
        }

        // Check: ExportedBindings must all be in VarDeclaredNames ∪ LexicallyDeclaredNames
        for name in &exported_bindings {
            if !var_names.contains(name) && !lex_names.contains(name) {
                return Err(self.error(format!("Export '{name}' is not defined")));
            }
        }

        Ok(())
    }

    fn collect_module_var_names(
        &self,
        stmt: &Statement,
        names: &mut std::collections::HashSet<String>,
    ) {
        match stmt {
            Statement::Variable(decl) if decl.kind == VarKind::Var => {
                for d in &decl.declarations {
                    let mut n = Vec::new();
                    self.collect_pattern_names(&d.pattern, &mut n);
                    names.extend(n);
                }
            }
            _ => {}
        }
    }

    fn collect_module_lex_names(
        &self,
        stmt: &Statement,
        names: &mut std::collections::HashSet<String>,
    ) -> Result<(), ParseError> {
        match stmt {
            Statement::Variable(decl) if decl.kind != VarKind::Var => {
                for d in &decl.declarations {
                    let mut n = Vec::new();
                    self.collect_pattern_names(&d.pattern, &mut n);
                    for name in n {
                        if !names.insert(name.clone()) {
                            return Err(self
                                .error(format!("Identifier '{name}' has already been declared")));
                        }
                    }
                }
            }
            // In modules, function/class/generator declarations are lexically scoped
            Statement::FunctionDeclaration(f) => {
                if !f.name.is_empty() && !names.insert(f.name.clone()) {
                    return Err(
                        self.error(format!("Identifier '{}' has already been declared", f.name))
                    );
                }
            }
            Statement::ClassDeclaration(c) => {
                if !c.name.is_empty() && !names.insert(c.name.clone()) {
                    return Err(
                        self.error(format!("Identifier '{}' has already been declared", c.name))
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn collect_export_var_names(
        &self,
        export: &ExportDeclaration,
        names: &mut std::collections::HashSet<String>,
    ) {
        if let ExportDeclaration::Named {
            declaration: Some(decl),
            ..
        } = export
        {
            self.collect_module_var_names(decl, names);
        }
    }

    fn collect_export_lex_names(
        &self,
        export: &ExportDeclaration,
        names: &mut std::collections::HashSet<String>,
    ) -> Result<(), ParseError> {
        match export {
            ExportDeclaration::Named {
                declaration: Some(decl),
                ..
            } => {
                self.collect_module_lex_names(decl, names)?;
            }
            ExportDeclaration::DefaultFunction(f) => {
                if !f.name.is_empty() && !names.insert(f.name.clone()) {
                    return Err(
                        self.error(format!("Identifier '{}' has already been declared", f.name))
                    );
                }
            }
            ExportDeclaration::DefaultClass(c) => {
                if !c.name.is_empty() && !names.insert(c.name.clone()) {
                    return Err(
                        self.error(format!("Identifier '{}' has already been declared", c.name))
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn collect_exported_bindings(
        &self,
        export: &ExportDeclaration,
        names: &mut std::collections::HashSet<String>,
    ) {
        if let ExportDeclaration::Named {
            specifiers,
            source: None,
            ..
        } = export
        {
            for spec in specifiers {
                names.insert(spec.local.clone());
            }
        }
    }

    fn parse_module_item(&mut self) -> Result<ModuleItem, ParseError> {
        match &self.current {
            Token::Keyword(Keyword::Import) => {
                // Check if it's dynamic import or import.meta (expressions), not declaration
                if self.is_import_expression() {
                    let stmt = self.parse_statement_or_declaration()?;
                    return Ok(ModuleItem::Statement(stmt));
                }
                let decl = self.parse_import_declaration()?;
                Ok(ModuleItem::ImportDeclaration(decl))
            }
            Token::Keyword(Keyword::Export) => {
                let decl = self.parse_export_declaration()?;
                Ok(ModuleItem::ExportDeclaration(decl))
            }
            _ => {
                let stmt = self.parse_statement_or_declaration()?;
                Ok(ModuleItem::Statement(stmt))
            }
        }
    }

    fn is_import_expression(&mut self) -> bool {
        // Peek ahead to see if this is `import(` or `import.meta`
        let saved_lt = self.prev_line_terminator;
        let saved_ts = self.current_token_start;
        let saved_te = self.current_token_end;
        let saved = match self.advance() {
            Ok(t) => t,
            Err(_) => return false,
        };
        let is_expr = self.current == Token::LeftParen || self.current == Token::Dot;
        // Restore
        self.push_back(self.current.clone(), self.prev_line_terminator);
        self.current = saved;
        self.prev_line_terminator = saved_lt;
        self.current_token_start = saved_ts;
        self.current_token_end = saved_te;
        is_expr
    }
}

fn expr_to_pattern(expr: Expression) -> Result<Pattern, ParseError> {
    match expr {
        Expression::Identifier(name) => Ok(Pattern::Identifier(name)),
        Expression::Assign(AssignOp::Assign, left, right) => {
            let pat = expr_to_pattern(*left)?;
            Ok(Pattern::Assign(Box::new(pat), right))
        }
        Expression::Array(elements, trailing_comma_after_spread) => {
            let mut pats = Vec::new();
            let mut saw_rest = false;
            for e in elements {
                if saw_rest {
                    return Err(ParseError {
                        message: "Rest element must be last element".to_string(),
                    });
                }
                let pat = e
                    .map(|e| {
                        if let Expression::Spread(inner) = e {
                            if let Expression::Assign(AssignOp::Assign, _, _) = *inner {
                                return Err(ParseError {
                                    message: "Rest element may not have a default initializer"
                                        .to_string(),
                                });
                            }
                            saw_rest = true;
                            expr_to_pattern(*inner).map(ArrayPatternElement::Rest)
                        } else {
                            expr_to_pattern(e).map(ArrayPatternElement::Pattern)
                        }
                    })
                    .transpose()?;
                if saw_rest && pat.is_none() {
                    return Err(ParseError {
                        message: "Rest element must be last element".to_string(),
                    });
                }
                pats.push(pat);
            }
            if saw_rest && trailing_comma_after_spread {
                return Err(ParseError {
                    message: "Rest element must be last element".to_string(),
                });
            }
            Ok(Pattern::Array(pats))
        }
        Expression::Object(props) => {
            let mut pat_props = Vec::new();
            let mut saw_rest = false;
            for prop in props {
                if saw_rest {
                    return Err(ParseError {
                        message: "Rest element must be last element".to_string(),
                    });
                }
                if let PropertyKind::Init = prop.kind {
                    if let Expression::Spread(inner) = prop.value {
                        saw_rest = true;
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
        Expression::Member(_, _) => Ok(Pattern::MemberExpression(Box::new(expr))),
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
