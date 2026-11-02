//! More of V8's wording for early errors (#1523), `--node` only: the texts
//! that need no parser state, the escape that a strict string rejects and the
//! reasons of the `\k<..>` and `(?<..>` forms of a regular expression.

use super::exact::NON_STRICT_FUNCTION;
use super::*;

const EVAL_OR_ARGUMENTS: &str = "Unexpected eval or arguments in strict mode";

/// V8's text for `msg`, when it needs no parser state.
pub(super) fn text(msg: &str) -> Option<Cow<'static, str>> {
    let fixed = match msg {
        "Class declaration not allowed in statement position" => "Unexpected token 'class'",
        "Invalid right-hand side in 'in' expression" => "Malformed arrow function parameter list",
        "'await' is not allowed as a function name in async functions"
        | "'await' is not allowed as a binding identifier in class static blocks" => {
            "Unexpected reserved word"
        }
        "Invalid Unicode code point" => INVALID_TOKEN,
        "'eval' is not allowed as a function name in strict mode"
        | "'arguments' is not allowed as a function name in strict mode"
        | "'eval' cannot be used as an import binding in module code"
        | "'arguments' cannot be used as an import binding in module code" => EVAL_OR_ARGUMENTS,
        _ => {
            let name = msg
                .strip_prefix("Duplicate binding '")
                .and_then(|rest| rest.strip_suffix("' in for-in loop"))
                .or_else(|| {
                    msg.strip_prefix("Duplicate binding '")?
                        .strip_suffix("' in catch clause")
                })?;
            return Some(Cow::Owned(format!(
                "Identifier '{name}' has already been declared"
            )));
        }
    };
    Some(Cow::Borrowed(fixed))
}

impl<'a> Parser<'a> {
    /// A function declaration where a statement stands, as the engine words it.
    pub(in crate::parser) fn non_strict_fn_error(&self) -> ParseError {
        self.error(NON_STRICT_FUNCTION.trim_end_matches('.'))
    }

    /// The same, as V8 words it.
    pub(in crate::parser) fn function_in_statement_position(&self) -> &'static str {
        if self.strict {
            "In strict mode code, functions can only be declared at top level or inside a block."
        } else {
            NON_STRICT_FUNCTION
        }
    }

    /// A legacy octal escape in strict code: V8 words `\8` and `\9` on their
    /// own. Of the escapes read so far, the last one decides.
    pub(super) fn strict_escape_text(&self) -> &'static str {
        let text = self.source_text_source.get(..self.current_token_end);
        let mut last_is_eight_or_nine = false;
        let mut chars = text.unwrap_or("").chars().peekable();
        while let Some(c) = chars.next() {
            if c != '\\' {
                continue;
            }
            match chars.next() {
                Some('8' | '9') => last_is_eight_or_nine = true,
                Some('1'..='7') => last_is_eight_or_nine = false,
                Some('0') if chars.peek().is_some_and(char::is_ascii_digit) => {
                    last_is_eight_or_nine = false;
                }
                _ => {}
            }
        }
        if last_is_eight_or_nine {
            "\\8 and \\9 are not allowed in strict mode."
        } else {
            "Octal escape sequences are not allowed in strict mode."
        }
    }
}

/// The group names of `pattern` (`(?<name>`), as the text between the brackets.
fn group_names(pattern: &str) -> impl Iterator<Item = &str> {
    pattern.match_indices("(?<").filter_map(|(at, _)| {
        let rest = &pattern[at + 3..];
        if rest.starts_with(['=', '!']) {
            return None;
        }
        Some(rest.split('>').next().unwrap_or(rest))
    })
}

/// Whether the `\u` escapes of `name` are well formed: four hex digits or a
/// braced code point of at most 10FFFF.
fn escapes_well_formed(name: &str) -> bool {
    let mut rest = name;
    while let Some(at) = rest.find("\\u") {
        rest = &rest[at + 2..];
        let hex = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit());
        if let Some(braced) = rest.strip_prefix('{') {
            let Some((digits, _)) = braced.split_once('}') else {
                return false;
            };
            if !hex(digits) || !u32::from_str_radix(digits, 16).is_ok_and(|v| v <= 0x10FFFF) {
                return false;
            }
        } else if !hex(rest.get(..4).unwrap_or("")) {
            return false;
        }
    }
    true
}

/// `Invalid capture group name` unless the fault is a malformed escape.
pub(super) fn group_name_reason(pattern: &str) -> &'static str {
    if group_names(pattern).any(|name| !escapes_well_formed(name)) {
        "Invalid Unicode escape"
    } else {
        "Invalid capture group name"
    }
}

/// The escapes of `pattern` as (offset of the escaped character, the
/// character): an escaped backslash does not start another.
fn escapes(pattern: &str) -> impl Iterator<Item = (usize, char)> + '_ {
    let mut chars = pattern.char_indices();
    std::iter::from_fn(move || {
        loop {
            let (_, c) = chars.next()?;
            if c == '\\' {
                return chars.next();
            }
        }
    })
}

fn is_group_name(name: &str) -> bool {
    let start = |c: char| c.is_alphabetic() || matches!(c, '_' | '$' | '\\');
    let part =
        |c: char| c.is_alphanumeric() || matches!(c, '_' | '$' | '\\' | '{' | '}') || !c.is_ascii();
    let mut chars = name.chars();
    chars.next().is_some_and(start) && chars.all(part)
}

/// `\k<name>`: a name that cannot be one is not a reference to a missing group.
pub(super) fn named_reference_reason(pattern: &str) -> &'static str {
    let malformed = escapes(pattern).any(|(at, c)| {
        let Some(rest) = pattern[at + 1..].strip_prefix('<').filter(|_| c == 'k') else {
            return false;
        };
        rest.split_once('>')
            .is_none_or(|(name, _)| !is_group_name(name))
    });
    if malformed {
        "Invalid capture group name"
    } else {
        "Invalid named capture referenced"
    }
}

/// `\k` not followed by `<`, in a pattern that has named groups.
pub(super) fn bare_k_reason(pattern: &str) -> Option<&'static str> {
    let bare = escapes(pattern).any(|(at, c)| c == 'k' && !pattern[at + 1..].starts_with('<'));
    (bare && group_names(pattern).next().is_some()).then_some("Invalid named reference")
}
