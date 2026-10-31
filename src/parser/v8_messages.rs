//! V8's wording for the early errors whose text is not "this token is not
//! allowed here", under `--node` only (issues #1484, #1523). The text of a
//! message that needs no parser state is looked up by `static_text`, which
//! `create_error` also applies to errors the parser did not word itself
//! (lexer, regexp literals). A message that depends on the offending token
//! is reworded by `Parser::v8_text`. Default mode keeps its own wording.

use super::*;
use std::borrow::Cow;

pub(super) const STRICT_WORDS: [&str; 7] = [
    "implements",
    "interface",
    "let",
    "package",
    "private",
    "protected",
    "public",
];

const INVALID_TOKEN: &str = "Invalid or unexpected token";

const EXACT: &[(&str, &str)] = &[
    (
        "'let' is not allowed as a variable name in lexical declarations",
        "let is disallowed as a lexically bound name",
    ),
    ("Expected decorator expression after '@'", INVALID_TOKEN),
    ("Expected 'class' after decorator", INVALID_TOKEN),
    (
        "Illegal continue statement",
        "Illegal continue statement: no surrounding iteration statement",
    ),
    (
        "Delete of an unqualified identifier in strict mode",
        "Delete of an unqualified identifier in strict mode.",
    ),
    (
        "Assignment to 'eval' or 'arguments' in strict mode",
        "Unexpected eval or arguments in strict mode",
    ),
    (
        "Octal literals are not allowed in strict mode",
        "Octal literals are not allowed in strict mode.",
    ),
    (
        "Octal escape sequences are not allowed in strict mode",
        "Octal escape sequences are not allowed in strict mode.",
    ),
    (
        "In strict mode code, functions can only be declared at top level or inside a block",
        "In strict mode code, functions can only be declared at top level or inside a block.",
    ),
    (
        "Async function declaration cannot appear in a single-statement context",
        "Async functions can only be declared at the top level or inside a block.",
    ),
    (
        "Generators can only be declared at the top level or inside a block",
        "Generators can only be declared at the top level or inside a block.",
    ),
    (
        "The identifier 'async' is disallowed as a left-hand side expression of a for-of loop",
        "The left-hand side of a for-of loop may not be 'async'.",
    ),
    (
        "for-of loop variable declaration may not have an initializer",
        "for-of loop variable declaration may not have an initializer.",
    ),
    (
        "for-in loop variable declaration may not have an initializer",
        "for-in loop variable declaration may not have an initializer.",
    ),
    (
        "for await...of is only valid in async functions or at module top level",
        "Unexpected reserved word",
    ),
    (
        "Getter must not have any formal parameters",
        "Getter must not have any formal parameters.",
    ),
    (
        "Setter must have exactly one formal parameter",
        "Setter must have exactly one formal parameter.",
    ),
    (
        "Class fields and methods cannot be named '#constructor'",
        "Classes may not have a field named 'constructor'",
    ),
    (
        "Class field initializer cannot reference 'arguments'",
        "'arguments' is not allowed in class field initializer or static initialization block",
    ),
    (
        "'arguments' is not allowed in class static blocks",
        "'arguments' is not allowed in class field initializer or static initialization block",
    ),
    (
        "Applying the 'delete' operator to a private name is not allowed",
        "Private fields can not be deleted",
    ),
    (
        "Tagged template cannot be used in an optional chain",
        "Invalid tagged template on optional chain",
    ),
    (
        "Await expression is not allowed in formal parameters of an async function",
        "Illegal await-expression in formal parameters of async function",
    ),
    (
        "Yield expression is not allowed in formal parameters of a generator function",
        "Yield expression not allowed in formal parameter",
    ),
    (
        "'await' is not allowed in async arrow formal parameters",
        "'await' is not a valid identifier name in an async function",
    ),
    (
        "Octal escape sequences are not allowed in template literals",
        "Octal escape sequences are not allowed in template strings.",
    ),
    (
        "\\8 and \\9 are not allowed in template literals",
        "\\8 and \\9 are not allowed in template strings.",
    ),
    (
        "\\8 and \\9 are not allowed in strict mode",
        "\\8 and \\9 are not allowed in strict mode.",
    ),
    (
        "Unary operator used immediately before exponentiation expression. Parenthesization is required",
        "Unary operator used immediately before exponentiation expression. Parenthesis must be used to disambiguate operator precedence",
    ),
    (
        "Cannot use 'new' with import()",
        "Cannot use new with import",
    ),
    (
        "import.meta is only valid in module code",
        "Cannot use 'import.meta' outside a module",
    ),
    ("Unexpected token )", "Unexpected token ')'"),
    (
        "An imported name that is not a valid binding identifier requires 'as'",
        "Unexpected reserved word",
    ),
    (
        "Invalid destructuring target",
        "Invalid destructuring assignment target",
    ),
    (
        "Unexpected 'import'",
        "Cannot use import statement outside a module",
    ),
    (
        "A switch statement may only have one default clause",
        "More than one default clause in switch statement",
    ),
    (
        "Expected '}' after template substitution",
        "Missing } in template expression",
    ),
    ("Invalid Unicode escape", "Invalid Unicode escape sequence"),
    (
        "Unterminated Unicode escape",
        "Invalid Unicode escape sequence",
    ),
    ("Invalid hex escape", "Invalid hexadecimal escape sequence"),
    (
        "Unicode code point out of range",
        "Undefined Unicode code-point",
    ),
    ("Unterminated template literal", "Unexpected end of input"),
    (
        "Unterminated regular expression",
        "Invalid regular expression: missing /",
    ),
    (
        "Numeric separator must be exactly one underscore",
        "Only one underscore is allowed as numeric separator",
    ),
    (
        "Trailing numeric separator",
        "Numeric separators are not allowed at the end of numeric literals",
    ),
    (
        "Numeric separator cannot appear after leading 0",
        "Numeric separator can not be used after leading 0.",
    ),
];

fn quoted<'a>(msg: &'a str, prefix: &str) -> Option<&'a str> {
    msg.strip_prefix(prefix)?.strip_suffix('\'')
}

pub(crate) fn static_text(msg: &str) -> Option<Cow<'_, str>> {
    if let Some((_, v8)) = EXACT.iter().find(|(ours, _)| *ours == msg) {
        return Some(Cow::Borrowed(v8));
    }
    if msg.starts_with("Assignment to '") && msg.ends_with("' in strict mode")
        || msg.ends_with("can't be used as a binding identifier in strict mode")
    {
        return Some(Cow::Borrowed("Unexpected eval or arguments in strict mode"));
    }
    if msg.starts_with("Unexpected strict mode reserved word '") {
        return Some(Cow::Borrowed("Unexpected strict mode reserved word"));
    }
    if let Some(name) = quoted(msg, "Unexpected reserved word '") {
        return Some(Cow::Borrowed(if STRICT_WORDS.contains(&name) {
            "Unexpected strict mode reserved word"
        } else {
            "Unexpected reserved word"
        }));
    }
    if let Some(label) = msg
        .strip_prefix("Label '")
        .and_then(|l| l.strip_suffix("' is not an iteration statement"))
    {
        return Some(Cow::Owned(format!(
            "Illegal continue statement: '{label}' does not denote an iteration statement"
        )));
    }
    if let Some(name) = msg
        .strip_prefix("Duplicate binding '")
        .and_then(|rest| rest.strip_suffix("' in module scope"))
    {
        return Some(Cow::Owned(format!(
            "Identifier '{name}' has already been declared"
        )));
    }
    if let Some(key) = msg.strip_prefix("Duplicate attribute key ") {
        return Some(Cow::Owned(format!(
            "Import attribute has duplicate key {key}"
        )));
    }
    if msg.starts_with("Export '") && msg.ends_with("' is not defined") {
        return Some(Cow::Owned(format!("{msg} in module")));
    }
    if let Some(name) = msg.strip_prefix("Unexpected token #") {
        return Some(Cow::Owned(format!("Unexpected identifier '#{name}'")));
    }
    if let Some(name) = msg
        .strip_prefix("Reference to undeclared private field or method '")
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return Some(Cow::Owned(format!(
            "Private field '{name}' must be declared in an enclosing class"
        )));
    }
    if msg.starts_with("Invalid regular expression flags") {
        return Some(Cow::Borrowed("Invalid regular expression flags"));
    }
    if msg.starts_with("Rest element must be last element in ") {
        return Some(Cow::Borrowed("Rest element must be last element"));
    }
    let lexical = msg.starts_with("Numeric separator cannot appear ")
        || msg.starts_with("Unexpected character: ")
        || msg.starts_with("Invalid Unicode escape in ")
        || matches!(
            msg,
            "Invalid BigInt literal"
                | "Invalid hex literal"
                | "Invalid binary literal"
                | "Invalid octal literal"
        )
        || matches!(
            msg,
            "Unterminated string literal"
                | "Unterminated block comment"
                | "Unterminated escape sequence"
                | "Identifier directly after number"
                | "Invalid numeric literal"
                | "Invalid or unexpected '#'"
        );
    lexical.then_some(Cow::Borrowed(INVALID_TOKEN))
}

/// `message` as `--node` reports it: V8's text for the cases that need no parser
/// state, and no doubled `SyntaxError: ` prefix.
pub(crate) fn node_message(message: String) -> String {
    let bare = message.strip_prefix("SyntaxError: ").unwrap_or(&message);
    match static_text(bare) {
        Some(text) => text.into_owned(),
        None => bare.to_string(),
    }
}

impl ParseError {
    /// The message of the `SyntaxError` `eval`, `Function` and `$262.evalScript`
    /// throw: `SyntaxError: ...` as the engine words it, V8's text under `--node`.
    pub(crate) fn js(&self) -> String {
        if inferred_names::enabled() {
            self.js_bare()
        } else {
            self.to_string()
        }
    }

    /// The same without the prefix, for the module loader.
    pub(crate) fn js_bare(&self) -> String {
        if inferred_names::enabled() {
            node_message(self.message.clone())
        } else {
            self.message.clone()
        }
    }
}

/// V8's message for a regexp literal that does not compile: the flags are part
/// of the printed literal and a few reasons are worded differently.
pub(crate) fn regexp_literal_message(msg: &str, flags: &str) -> String {
    let Some(rest) = msg.strip_prefix("Invalid regular expression: /") else {
        return msg.to_string();
    };
    let Some((pattern, reason)) = rest.rsplit_once("/ : ") else {
        return msg.to_string();
    };
    let reason = match reason {
        "Invalid named reference" => "Invalid named capture referenced",
        other => other,
    };
    format!("Invalid regular expression: /{pattern}/{flags}: {reason}")
}

/// The message V8 gives for the first bad escape of a template's raw text.
fn template_escape_message(raw: &str) -> &'static str {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        let rest = &raw[i + 1..];
        let next = rest.bytes().next();
        let hex =
            |s: &str, n: usize| s.len() >= n && s.bytes().take(n).all(|b| b.is_ascii_hexdigit());
        match next {
            Some(b'x') if !hex(&rest[1..], 2) => return "Invalid hexadecimal escape sequence",
            Some(b'u') => {
                let tail = &rest[1..];
                if let Some(braced) = tail.strip_prefix('{') {
                    match braced.find('}') {
                        Some(end)
                            if end > 0 && braced[..end].bytes().all(|b| b.is_ascii_hexdigit()) =>
                        {
                            if !u32::from_str_radix(&braced[..end], 16).is_ok_and(|v| v <= 0x10FFFF)
                            {
                                return "Undefined Unicode code-point";
                            }
                        }
                        _ => return "Invalid Unicode escape sequence",
                    }
                } else if !hex(tail, 4) {
                    return "Invalid Unicode escape sequence";
                }
            }
            Some(b'0') if rest.as_bytes().get(1).is_some_and(u8::is_ascii_digit) => {
                return "Octal escape sequences are not allowed in template strings.";
            }
            Some(b'1'..=b'7') => {
                return "Octal escape sequences are not allowed in template strings.";
            }
            Some(b'8' | b'9') => return "\\8 and \\9 are not allowed in template strings.",
            _ => {}
        }
        i += 2;
    }
    "Invalid Unicode escape sequence"
}

impl<'a> Parser<'a> {
    /// The text of `msg` for the token the parser stopped at, once the `--node`
    /// hooks are installed.
    pub(super) fn v8_text(&self, msg: String) -> String {
        if !inferred_names::enabled() {
            return msg;
        }
        if msg == "Unexpected 'import'" && self.is_module {
            return self.v8_unexpected();
        }
        if msg == "Octal literals are not allowed in strict mode"
            && matches!(self.current, Token::NonOctalDecimalLiteral(_))
        {
            return "Decimals with leading zeros are not allowed in strict mode.".to_string();
        }
        if let Some(text) = static_text(&msg) {
            return text.into_owned();
        }
        let single_statement = msg.ends_with("cannot appear in a single-statement context")
            || msg == "Declaration not allowed in statement position of iteration statement";
        match msg.as_str() {
            "Expected function name" => match self.current {
                Token::LeftParen => "Function statements require a function name".to_string(),
                _ => self.v8_unexpected(),
            },
            "Invalid escape sequence in template literal" => match &self.current {
                Token::NoSubstitutionTemplate(_, raw)
                | Token::TemplateHead(_, raw)
                | Token::TemplateMiddle(_, raw)
                | Token::TemplateTail(_, raw) => template_escape_message(raw).to_string(),
                _ => msg,
            },
            "Expected property after '?.'"
                if matches!(
                    self.current,
                    Token::NoSubstitutionTemplate(..) | Token::TemplateHead(..)
                ) =>
            {
                "Invalid tagged template on optional chain".to_string()
            }
            "Declaration not allowed in statement position of iteration statement"
                if self.current == Token::Keyword(Keyword::Function) =>
            {
                "In non-strict mode code, functions can only be declared at top level, inside a block, or as the body of an if statement.".to_string()
            }
            _ if single_statement
                && matches!(
                    self.current,
                    Token::Keyword(Keyword::Class | Keyword::Const)
                ) =>
            {
                self.v8_unexpected()
            }
            "for await is only valid with for...of loops"
            | "Private fields are not allowed in object literals"
            | "Logical expressions and coalesce expressions cannot be mixed without parentheses" => {
                self.v8_unexpected()
            }
            _ if msg.starts_with("Expected ") => self.v8_unexpected(),
            _ => msg,
        }
    }

    /// The target of a `for (target of/in ...)` head as a pattern.
    pub(super) fn for_head(&self, expr: Expression) -> Result<Pattern, ParseError> {
        let nested = matches!(expr, Expression::Array(..) | Expression::Object(..));
        expr_to_pattern(expr).map_err(|e| {
            if inferred_names::enabled() && e.message == "Invalid destructuring target" && !nested {
                self.error("Invalid left-hand side in for-loop")
            } else {
                e
            }
        })
    }

    /// `++x` / `x++` on something that cannot be assigned to.
    pub(super) fn validate_update_target(
        &self,
        expr: &Expression,
        prefix: bool,
    ) -> Result<(), ParseError> {
        self.validate_assignment_target(expr, true, true)
            .map_err(|e| {
                if inferred_names::enabled() && e.message == "Invalid left-hand side in assignment"
                {
                    let kind = if prefix { "prefix" } else { "postfix" };
                    self.error(format!(
                        "Invalid left-hand side expression in {kind} operation"
                    ))
                } else {
                    e
                }
            })
    }
}
