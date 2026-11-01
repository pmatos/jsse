//! V8's reasons for the escapes a unicode-mode pattern rejects (#1523): the
//! engine says `Invalid escape` where V8 names the escape (`\k`, `\p`, a
//! `\c` inside a class).

/// The reason V8 gives for the first escape of `pattern` that unicode mode
/// rejects, when it is not plain `Invalid escape`.
pub(super) fn unicode_escape_reason(pattern: &str, flags: &str) -> Option<&'static str> {
    if !flags.contains(['u', 'v']) {
        return None;
    }
    let chars: Vec<char> = pattern.chars().collect();
    let mut in_class = false;
    let mut at = 0;
    while at < chars.len() {
        match chars[at] {
            '[' if !in_class => in_class = true,
            ']' => in_class = false,
            '\\' => {
                let escaped = *chars.get(at + 1)?;
                let next = chars.get(at + 2).copied();
                match escaped {
                    'k' if next != Some('<') => return Some("Invalid named reference"),
                    'p' | 'P' if next != Some('{') => return Some("Invalid property name"),
                    'c' if !next.is_some_and(|c| c.is_ascii_alphabetic()) => {
                        return in_class.then_some("Invalid Unicode escape");
                    }
                    c if c.is_ascii_alphabetic() && !"dDwWsSbBfnrtvxucpPk".contains(c) => {
                        return None;
                    }
                    _ => {}
                }
                at += 1;
            }
            _ => {}
        }
        at += 1;
    }
    None
}
