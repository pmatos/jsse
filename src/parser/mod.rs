use crate::ast::*;
use crate::lexer::{Keyword, Lexer, Token};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub(crate) use cover_patterns::{check_object_rest_target, expr_to_pattern, pattern_to_expr};
pub(crate) use parse_error::ParseError;

mod arrow_params;
mod catch_param;
mod cover_init;
mod cover_patterns;
mod declarations;
pub(crate) mod dynamic_function;
mod expressions;
mod for_head;
pub(crate) mod inferred_names;
mod module_early_errors;
mod module_start;
mod modules;
mod node_hooks;
mod parse_error;
mod positions;
mod prologue;
pub(crate) mod source_scan;
mod statements;
mod strict_patterns;
pub(crate) mod v8_messages;
pub(crate) mod v8_position;
mod yield_scan;

#[derive(Clone, Copy, PartialEq)]
enum PrivateNameKind {
    Getter,
    Setter,
    Other,
}

/// Snapshot of the two Annex B.3.3 block-scoping flags
/// (`in_block_or_function`/`in_switch_case`), re-scoped at every site that
/// parses a brace-delimited statement list: `Block`, the three `try` bodies,
/// and the `switch` `CaseBlock` consequent. `Copy` and restored
/// unconditionally so a failed nested parse can't leak the re-scoped values
/// into the enclosing construct (issue #608, following #597/#602).
///
/// `save_block_scope` only snapshots; it sets neither flag. The four
/// brace-body sites need `in_block_or_function = true; in_switch_case =
/// false`, the `switch` `CaseBlock` consequent only `in_switch_case = true`,
/// so each site's `enter` closure sets exactly the fields it needs.
#[derive(Clone, Copy)]
struct SavedBlockScope {
    in_block_or_function: bool,
    in_switch_case: bool,
}

/// Snapshot of the function-context flags re-scoped by class static blocks
/// and function bodies. Deliberately excludes `strict` (restored via
/// `set_strict`, which also updates the lexer) and `function_param_names`
/// (its callers reset it to `None` rather than restoring a prior value) —
/// both stay hand-managed locals at the one site that touches each. Each
/// call site mutates only the subset of these fields it needs; an unmutated
/// field round-trips as a same-value no-op (issue #608).
struct SavedFunctionContext {
    in_function: u32,
    in_non_arrow_function: u32,
    in_iteration: u32,
    in_switch: u32,
    in_generator: bool,
    in_async: bool,
    in_static_block: bool,
    in_block_or_function: bool,
    in_switch_case: bool,
    in_formal_parameters: bool,
    allow_super_property: bool,
    allow_super_call: bool,
    labels: Vec<(String, bool)>,
}
enum LookaheadToken<'parser> {
    Parser(&'parser Token),
    Scanned(Token),
}

/// A read-only fork of the token stream for grammar disambiguation.
///
/// Probes advance only the cloned lexer, so every exit path leaves the parser's
/// token, source span, and line-terminator state unchanged.
struct Lookahead<'parser, 'source> {
    lexer: Lexer<'source>,
    current: LookaheadToken<'parser>,
    line_terminator_before: bool,
}

impl Lookahead<'_, '_> {
    fn current(&self) -> &Token {
        match &self.current {
            LookaheadToken::Parser(token) => token,
            LookaheadToken::Scanned(token) => token,
        }
    }

    fn line_terminator_before(&self) -> bool {
        self.line_terminator_before
    }

    fn advance(&mut self) -> Result<(), ParseError> {
        self.line_terminator_before = false;
        loop {
            let token = self.lexer.next_token()?;
            if token == Token::LineTerminator {
                self.line_terminator_before = true;
                continue;
            }
            self.current = LookaheadToken::Scanned(token);
            return Ok(());
        }
    }
}

pub(crate) struct Parser<'a> {
    source: &'a str,
    source_text_source: Rc<str>,
    lexer: Lexer<'a>,
    current: Token,
    current_token_start: usize,
    current_token_end: usize,
    prev_token_end: usize,
    prev_line_terminator: bool,
    strict: bool,
    is_module: bool,
    /// Context depth counters. A construct that re-scopes one of these must
    /// restore it on its **error** path too: any counter whose decrement runs
    /// unconditionally somewhere turns a leaked zero into an underflow, which
    /// is what issue #597 was. The seven re-scoping sites that touch a batch
    /// of these fields together go through `with_block_scope`/
    /// `with_function_context`, the combinators that own the save → enter →
    /// body → unconditional-restore sequence, rather than hand-written
    /// save/mutate/restore blocks (issue #608); a handful of single- or
    /// dual-field sites elsewhere (e.g. `in_non_arrow_function` bumps in
    /// `expressions.rs`) still restore by hand and are unaffected. Adding a
    /// counter here means auditing its decrements for that shape.
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
    private_name_scopes: Vec<(HashSet<String>, Vec<String>)>,
    in_field_initializer_eval: bool,
    in_static_block: bool,
    function_param_names: Option<HashSet<String>>,
    last_expr_parenthesized: bool,
    /// Token held back after an unparenthesized ArrowFunction (see
    /// `parse_left_hand_side_expression`); `current` is `Token::Eof` meanwhile.
    arrow_held_token: Option<Token>,
    /// Set by a `( ArrowFunction )` primary so the enclosing
    /// `parse_primary_expression` keeps `last_expr_parenthesized` for it.
    parenthesized_arrow: bool,
    /// Set while an arrow's parameter list is re-parsed as FormalParameters.
    refining_arrow_params: bool,
    last_obj_had_proto_dup: bool,
    cover_init: cover_init::CoverInit,
    /// Current recursive-descent nesting depth (expressions + statements).
    /// Guarded against `MAX_PARSE_DEPTH` so pathologically nested source raises
    /// a catchable parse error instead of overflowing the native stack.
    parse_depth: u32,
    lone_surrogate_stand_ins: bool,
}

/// Maximum parser recursion depth before a catchable parse error is raised
/// instead of letting recursive-descent recursion overflow the native stack.
/// Sits well below the native capacity of the 128 MiB execution stack the
/// engine runs on (see `lib.rs`) and far above anything test262 contains; the
/// release limit is comparable to V8's own (which rejects ~5000-deep nesting).
/// One source nesting level costs more than one unit: each `[` spends 3
/// (assignment, exponentiation, unary).
///
/// Profile-aware because the resource really being bounded is stack *bytes*,
/// not units. Measured by binary searching the abort point over the most
/// stack-hungry nestings (`[`, `({a:`, `({a:[`, `` `${ ``):
/// the engine stack holds ~37,000 depth units in release but only ~3,800 in
/// debug, and each limit is set ~9x below its own profile's capacity. A single
/// 4,000 sat *above* debug's capacity, which is why deeply nested source
/// aborted (SIGABRT) rather than raising this error there — jsse#599. Reaching
/// either limit costs ~14 MB of stack, so both assume the engine stack: a
/// parser driven on a smaller thread can still overflow first.
///
/// The sibling guards in `interpreter/mod.rs` (`CALL_DEPTH_*`,
/// `EVAL_DEPTH_LIMIT`) got the same profile-aware treatment in jsse#607.
pub(crate) const MAX_PARSE_DEPTH: u32 = if cfg!(debug_assertions) { 400 } else { 4_000 };

impl<'a> Parser<'a> {
    pub(crate) fn new(source: &'a str) -> Result<Self, ParseError> {
        Self::new_with_lexer(source, Lexer::new(source))
    }

    /// Parser for source from `encode_source_units` whose lone surrogates were
    /// mapped to private-use code points (eval and dynamic function text):
    /// string and template values, template raw strings and function source
    /// text all restore them exactly.
    pub(crate) fn new_with_stand_ins(source: &'a str) -> Result<Self, ParseError> {
        let mut parser = Self::new_with_lexer(source, Lexer::new_with_stand_ins(source))?;
        parser.lone_surrogate_stand_ins = true;
        Ok(parser)
    }

    /// `new_with_stand_ins` when `stand_ins`, else `new`.
    pub(crate) fn for_source(source: &'a str, stand_ins: bool) -> Result<Self, ParseError> {
        if stand_ins {
            Self::new_with_stand_ins(source)
        } else {
            Self::new(source)
        }
    }

    fn new_with_lexer(source: &'a str, mut lexer: Lexer<'a>) -> Result<Self, ParseError> {
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
        let source_text_source = Rc::from(source);
        Ok(Self {
            source,
            source_text_source,
            lexer,
            current,
            current_token_start: token_start,
            current_token_end: token_end,
            prev_token_end: 0,
            prev_line_terminator: had_lt,
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
            last_expr_parenthesized: false,
            arrow_held_token: None,
            parenthesized_arrow: false,
            refining_arrow_params: false,
            last_obj_had_proto_dup: false,
            cover_init: Default::default(),
            parse_depth: 0,
            lone_surrogate_stand_ins: false,
        })
    }

    /// Raise the parser recursion depth by one, returning a catchable parse
    /// error if `MAX_PARSE_DEPTH` is exceeded. Paired with `exit_recursion`.
    /// Used by the expression and statement recursion funnels so deeply nested
    /// source raises a parse error rather than overflowing the native stack.
    fn enter_recursion(&mut self) -> Result<(), ParseError> {
        self.parse_depth += 1;
        if self.parse_depth > MAX_PARSE_DEPTH {
            self.parse_depth -= 1;
            return Err(self.error("Maximum parse depth exceeded: input nested too deeply"));
        }
        Ok(())
    }

    fn exit_recursion(&mut self) {
        self.parse_depth -= 1;
    }

    fn lookahead<R>(&self, inspect: impl FnOnce(&mut Lookahead<'_, 'a>) -> R) -> R {
        let mut tokens = Lookahead {
            lexer: self.lexer.clone(),
            current: LookaheadToken::Parser(&self.current),
            line_terminator_before: self.prev_line_terminator,
        };
        inspect(&mut tokens)
    }

    fn advance(&mut self) -> Result<Token, ParseError> {
        self.prev_token_end = self.current_token_end;
        let old = std::mem::replace(&mut self.current, Token::Eof);
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
        Ok(old)
    }

    fn eat(&mut self, expected: &Token) -> Result<(), ParseError> {
        if &self.current == expected {
            self.advance()?;
            Ok(())
        } else {
            Err(self.expected_error(expected))
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
        Err(self.expected_semicolon())
    }

    fn error(&self, msg: impl Into<String>) -> ParseError {
        let message = self.v8_text(msg.into());
        ParseError { message }
    }

    fn source_since(&self, start: usize) -> Option<SourceText> {
        let end = self.prev_token_end.max(start);
        Some(
            SourceText::new(self.source_text_source.clone(), start, end)
                .with_pua_encoded(self.lone_surrogate_stand_ins),
        )
    }

    pub(crate) fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
        self.lexer.strict = strict;
    }

    pub(crate) fn set_eval_in_class_with_names(&mut self, names: HashSet<String>) {
        self.private_name_scopes.push((names, Vec::new()));
    }

    pub(crate) fn set_eval_in_field_initializer(&mut self) {
        self.in_field_initializer_eval = true;
        self.allow_super_property = true;
        self.in_function += 1;
        self.in_non_arrow_function += 1;
    }

    pub(crate) fn set_eval_new_target_allowed(&mut self) {
        self.in_non_arrow_function += 1;
    }

    fn save_block_scope(&self) -> SavedBlockScope {
        SavedBlockScope {
            in_block_or_function: self.in_block_or_function,
            in_switch_case: self.in_switch_case,
        }
    }

    fn restore_block_scope(&mut self, saved: SavedBlockScope) {
        self.in_block_or_function = saved.in_block_or_function;
        self.in_switch_case = saved.in_switch_case;
    }

    /// `enter` closure shared by the four sites (`Block`, the three `try`
    /// bodies) that re-scope a plain brace-delimited statement list. The
    /// `switch` `CaseBlock` consequent is deliberately not one of them — see
    /// the note on `SavedBlockScope`.
    fn enter_block_scope(&mut self) {
        self.in_block_or_function = true;
        self.in_switch_case = false;
    }

    /// Snapshot the function-context fields before a call site re-scopes
    /// whichever subset it needs. `std::mem::take`s `labels` rather than
    /// cloning it — every call site wants `labels` reset to empty for the
    /// nested body, so this both captures the outer value and performs that
    /// reset in one step.
    fn save_function_context(&mut self) -> SavedFunctionContext {
        SavedFunctionContext {
            in_function: self.in_function,
            in_non_arrow_function: self.in_non_arrow_function,
            in_iteration: self.in_iteration,
            in_switch: self.in_switch,
            in_generator: self.in_generator,
            in_async: self.in_async,
            in_static_block: self.in_static_block,
            in_block_or_function: self.in_block_or_function,
            in_switch_case: self.in_switch_case,
            in_formal_parameters: self.in_formal_parameters,
            allow_super_property: self.allow_super_property,
            allow_super_call: self.allow_super_call,
            labels: std::mem::take(&mut self.labels),
        }
    }

    fn restore_function_context(&mut self, saved: SavedFunctionContext) {
        self.in_function = saved.in_function;
        self.in_non_arrow_function = saved.in_non_arrow_function;
        self.in_iteration = saved.in_iteration;
        self.in_switch = saved.in_switch;
        self.in_generator = saved.in_generator;
        self.in_async = saved.in_async;
        self.in_static_block = saved.in_static_block;
        self.in_block_or_function = saved.in_block_or_function;
        self.in_switch_case = saved.in_switch_case;
        self.in_formal_parameters = saved.in_formal_parameters;
        self.allow_super_property = saved.allow_super_property;
        self.allow_super_call = saved.allow_super_call;
        self.labels = saved.labels;
    }

    /// Owns both ends of a block-scope re-scoping: snapshot, `enter`
    /// mutation, `body` parse, unconditional restore, then the result is
    /// handed back for the caller's own `?`. Restoring here — rather than at
    /// each call site — means there is no separate restore call for a future
    /// author to omit, reorder, or gate on `Ok` (issue #608, following
    /// #597/#602's restore-on-error fix).
    fn with_block_scope<T>(
        &mut self,
        enter: impl FnOnce(&mut Self),
        body: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        let saved = self.save_block_scope();
        enter(self);
        let result = body(self);
        self.restore_block_scope(saved);
        result
    }

    /// Function-context counterpart of `with_block_scope`, for the sites that
    /// re-scope the function-context field set (class static blocks, field
    /// initializers, function bodies).
    fn with_function_context<T>(
        &mut self,
        enter: impl FnOnce(&mut Self),
        body: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        let saved = self.save_function_context();
        let outer = v8_position::enter_function();
        enter(self);
        let result = body(self);
        v8_position::restore(outer);
        self.restore_function_context(saved);
        result
    }

    pub(crate) fn set_eval_allow_super_property(&mut self) {
        self.allow_super_property = true;
    }

    pub(crate) fn set_eval_allow_super_call(&mut self) {
        self.allow_super_call = true;
    }

    fn push_private_scope(&mut self) {
        self.private_name_scopes.push((HashSet::new(), Vec::new()));
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
            used.push(name.to_string());
        }
        Ok(())
    }

    pub(crate) fn validate_eval_private_names(&mut self) -> Result<(), ParseError> {
        while !self.private_name_scopes.is_empty() {
            self.pop_private_scope()?;
        }
        Ok(())
    }

    fn pop_private_scope(&mut self) -> Result<(), ParseError> {
        let Some((declared, used)) = self.private_name_scopes.pop() else {
            return Ok(());
        };
        for name in used {
            if declared.contains(&name) {
                continue;
            }
            // The enclosing class may declare the name after this body.
            match self.private_name_scopes.last_mut() {
                Some((_, parent_used)) => parent_used.push(name),
                None => {
                    return Err(self.error(format!(
                        "Reference to undeclared private field or method '#{name}'"
                    )));
                }
            }
        }
        Ok(())
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

    fn identifier_name<'token>(&self, token: &'token Token) -> Option<&'token str> {
        match token {
            Token::Identifier(name) => {
                (!Self::is_reserved_identifier(name, self.strict)).then_some(name)
            }
            Token::IdentifierWithEscape(name) => (!Self::is_reserved_identifier(name, self.strict)
                && !(name == "yield" && (self.in_generator || self.strict))
                && !(name == "await" && (self.in_async || self.in_static_block || self.is_module)))
                .then_some(name),
            Token::Keyword(Keyword::Yield) if !self.in_generator && !self.strict => Some("yield"),
            Token::Keyword(Keyword::Await)
                if !self.in_async && !self.in_static_block && !self.is_module =>
            {
                Some("await")
            }
            Token::Keyword(Keyword::Let) if !self.strict => Some("let"),
            Token::Keyword(Keyword::Static) if !self.strict => Some("static"),
            Token::Keyword(Keyword::Async) => Some("async"),
            Token::Keyword(Keyword::Of) => Some("of"),
            _ => None,
        }
    }

    fn current_identifier_name(&self) -> Option<String> {
        self.identifier_name(&self.current).map(str::to_owned)
    }

    fn is_simple_parameter_list(params: &[Pattern]) -> bool {
        params.iter().all(|p| matches!(p, Pattern::Identifier(_)))
    }

    fn check_duplicate_params_strict(&self, params: &[Pattern]) -> Result<(), ParseError> {
        let mut seen = HashSet::new();
        let mut names = Vec::new();
        for p in params {
            p.bound_names(&mut names);
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
            "implements"
                | "interface"
                | "package"
                | "private"
                | "protected"
                | "public"
                | "yield"
                | "let"
                | "static"
        )
    }

    fn check_strict_identifier(&self, name: &str) -> Result<(), ParseError> {
        if self.strict && Self::is_strict_reserved_word(name) {
            return Err(self.error(format!("Unexpected strict mode reserved word '{name}'")));
        }
        Ok(())
    }

    fn check_strict_function_name(&self, name: &str) -> Result<(), ParseError> {
        if name == "eval" || name == "arguments" {
            return Err(self.error(format!(
                "'{name}' is not allowed as a function name in strict mode"
            )));
        }
        if Self::is_strict_reserved_word(name) {
            return Err(self.error(format!("Unexpected strict mode reserved word '{name}'")));
        }
        Ok(())
    }

    fn check_strict_params(&self, params: &[Pattern]) -> Result<(), ParseError> {
        let mut names = Vec::new();
        for p in params {
            p.bound_names(&mut names);
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

    pub(super) fn expr_contains_await_identifier(expr: &Expression) -> bool {
        use crate::ast::{ArrowBody, Expression, MemberProperty, Property};
        match expr {
            Expression::Identifier(name) => name == "await",
            Expression::Array(elems, _) => elems
                .iter()
                .any(|e| e.as_ref().is_some_and(Self::expr_contains_await_identifier)),
            Expression::Object(props, _) => props.iter().any(|p: &Property| {
                Self::expr_contains_await_identifier(&p.value)
                    || matches!(&p.key, crate::ast::PropertyKey::Computed(e) if Self::expr_contains_await_identifier(e))
            }),
            Expression::Member(object, property, _) => {
                Self::expr_contains_await_identifier(object)
                    || matches!(property, MemberProperty::Computed(e) if Self::expr_contains_await_identifier(e))
            }
            Expression::Call(callee, args, _) | Expression::New(callee, args, _) => {
                Self::expr_contains_await_identifier(callee)
                    || args.iter().any(Self::expr_contains_await_identifier)
            }
            Expression::Binary(_, left, right, _)
            | Expression::Logical(_, left, right)
            | Expression::Assign(_, left, right, _) => {
                Self::expr_contains_await_identifier(left) || Self::expr_contains_await_identifier(right)
            }
            Expression::Unary(_, operand) | Expression::Update(.., operand, _) => {
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
            Expression::Import(inner, opts, _)
            | Expression::ImportDefer(inner, opts)
            | Expression::ImportSource(inner, opts) => {
                Self::expr_contains_await_identifier(inner)
                    || opts.as_ref().is_some_and(|e| Self::expr_contains_await_identifier(e))
            }
            Expression::Spread(inner, _)
            | Expression::PreIterated(inner)
            | Expression::ClassProtoParent(inner, _)
            | Expression::Await(inner, _) => Self::expr_contains_await_identifier(inner),
            Expression::BoundCallee(func, this) | Expression::ResolvedHeritage(func, this) => {
                Self::expr_contains_await_identifier(func) || Self::expr_contains_await_identifier(this)
            }
            Expression::Yield(opt_e, _) => {
                opt_e.as_ref().is_some_and(|e| Self::expr_contains_await_identifier(e))
            }
            Expression::ArrowFunction(af) if !af.is_async => {
                af.params.iter().any(Self::pattern_contains_await_identifier)
                    || match &af.body {
                                            ArrowBody::Expression(body) => {
                                                if let Statement::Return(Some(e)) = &body.as_slice()[0] {
                                                    Self::expr_contains_await_identifier(e)
                                                } else {
                                                    false
                                                }
                                            }
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
                Self::expr_contains_await_identifier(object)
                    || Self::optional_chain_contains_await_identifier(chain)
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

    fn optional_chain_contains_await_identifier(chain: &Expression) -> bool {
        match chain {
            // Optional-chain tails use identifiers for static property names and
            // as placeholders for computed access and optional calls.
            Expression::Identifier(_) => false,
            Expression::Member(base, property, _) => {
                Self::optional_chain_contains_await_identifier(base)
                    || matches!(property, MemberProperty::Computed(e) if Self::expr_contains_await_identifier(e))
            }
            Expression::Call(callee, args, _) => {
                Self::optional_chain_contains_await_identifier(callee)
                    || args.iter().any(Self::expr_contains_await_identifier)
            }
            _ => Self::expr_contains_await_identifier(chain),
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
            Expression::Await(..) => true,
            Expression::Array(elems, _) => elems
                .iter()
                .any(|e| e.as_ref().is_some_and(Self::expr_contains_await_expression)),
            Expression::Object(props, _) => props.iter().any(|p: &Property| {
                Self::expr_contains_await_expression(&p.value)
                    || matches!(&p.key, crate::ast::PropertyKey::Computed(e) if Self::expr_contains_await_expression(e))
            }),
            Expression::Member(object, property, _) => {
                Self::expr_contains_await_expression(object)
                    || matches!(property, MemberProperty::Computed(e) if Self::expr_contains_await_expression(e))
            }
            Expression::Call(callee, args, _) | Expression::New(callee, args, _) => {
                Self::expr_contains_await_expression(callee)
                    || args.iter().any(Self::expr_contains_await_expression)
            }
            Expression::Binary(_, left, right, _)
            | Expression::Logical(_, left, right)
            | Expression::Assign(_, left, right, _) => {
                Self::expr_contains_await_expression(left) || Self::expr_contains_await_expression(right)
            }
            Expression::Unary(_, operand) | Expression::Update(.., operand, _) => {
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
            Expression::Spread(inner, _) => Self::expr_contains_await_expression(inner),
            Expression::Yield(opt_e, _) => {
                opt_e.as_ref().is_some_and(|e| Self::expr_contains_await_expression(e))
            }
            Expression::ArrowFunction(af) if !af.is_async => {
                af.params.iter().any(Self::pattern_contains_await_expression)
                    || match &af.body {
                                            ArrowBody::Expression(body) => {
                                                if let Statement::Return(Some(e)) = &body.as_slice()[0] {
                                                    Self::expr_contains_await_expression(e)
                                                } else {
                                                    false
                                                }
                                            }
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

    pub(crate) fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut body = Vec::new();
        let mut in_directive_prologue = true;
        let mut body_is_strict = false;
        let mut prologue_had_legacy_octal = false;
        let mut lexical_names: HashSet<String> = HashSet::new();

        while self.current != Token::Eof {
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
                        body_is_strict = true;
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
                    for name in Self::bound_names_from_decl(decl) {
                        if lexical_names.contains(&name) {
                            return Err(self
                                .error(format!("Identifier '{name}' has already been declared")));
                        }
                        lexical_names.insert(name);
                    }
                }
                Statement::ClassDeclaration(cls) => {
                    let name = &cls.name;
                    if !lexical_names.insert(name.clone()) {
                        return Err(
                            self.error(format!("Identifier '{name}' has already been declared"))
                        );
                    }
                }
                _ => {}
            }

            body.push(stmt);
        }

        // §16.1.1: VarDeclaredNames must not overlap LexicallyDeclaredNames
        if !lexical_names.is_empty() {
            let mut var_names = Vec::new();
            for stmt in &body {
                Self::collect_top_level_var_declared_names(stmt, &mut var_names);
            }
            for name in &var_names {
                if lexical_names.contains(name) {
                    return Err(
                        self.error(format!("Identifier '{name}' has already been declared"))
                    );
                }
            }
        }

        let mut program = Program {
            source_type: SourceType::Script,
            body: Body::new(body),
            module_items: Vec::new(),
            body_is_strict,
        };
        assign_ic_sites(&mut program.body);
        Ok(program)
    }

    pub(crate) fn parse_program_as_module(&mut self) -> Result<Program, ParseError> {
        self.is_module = true;
        self.relex_first_token_as_module()?;
        self.set_strict(true);

        let mut module_items = Vec::new();
        let mut exported_names = HashSet::new();

        while self.current != Token::Eof {
            let item = self.parse_module_item()?;

            // Check for duplicate exported names
            if let ModuleItem::ExportDeclaration(ref export) = item {
                for name in self.get_exported_names(export) {
                    if !exported_names.insert(name.clone()) {
                        return Err(self.duplicate_export(&name, &module_items, &item));
                    }
                }
            }

            module_items.push(item);
        }

        // §16.2.1.1 Module early errors
        self.validate_module_early_errors(&module_items)?;

        let mut program = Program {
            source_type: SourceType::Module,
            body: Body::new(Vec::new()),
            module_items,
            body_is_strict: true,
        };
        assign_ic_sites_for_module(&mut program);
        Ok(program)
    }

    fn parse_module_item(&mut self) -> Result<ModuleItem, ParseError> {
        match &self.current {
            Token::Keyword(Keyword::Import) => {
                // Dynamic import or import.meta (expressions), not a declaration
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

    fn is_import_expression(&self) -> bool {
        self.lookahead(|tokens| {
            tokens.advance().is_ok() && matches!(tokens.current(), Token::LeftParen | Token::Dot)
        })
    }
}

#[cfg(test)]
mod tests;
