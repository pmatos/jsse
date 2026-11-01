//! Source-text scanning shared by the `--node` position readers (#1511).

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$' | '\\' | '\u{200c}' | '\u{200d}')
}

pub(crate) fn skip_trivia(text: &str, mut at: usize) -> usize {
    loop {
        let rest = &text[at..];
        let Some(c) = rest.chars().next() else {
            return at;
        };
        if c.is_whitespace() || c == '\u{feff}' {
            at += c.len_utf8();
        } else if rest.starts_with("//") {
            let line = rest.find(['\n', '\r', '\u{2028}', '\u{2029}']);
            at += line.unwrap_or(rest.len());
        } else if let Some(comment) = rest.strip_prefix("/*") {
            match comment.find("*/") {
                Some(end) => at += end + 4,
                None => return text.len(),
            }
        } else {
            return at;
        }
    }
}

/// The end of the word at `at`, or `at` when none starts there.
pub(crate) fn word_end(text: &str, at: usize) -> usize {
    let word: usize = text[at..]
        .chars()
        .take_while(|&c| is_word_char(c))
        .map(char::len_utf8)
        .sum();
    at + word
}

/// Inside a parenthesised expression whose text starts at `at`: the offset of
/// its last operand when it is a comma expression, found by scanning to the
/// closing parenthesis (strings, templates and comments are skipped; a regular
/// expression literal holding a bracket or comma is not).
pub(crate) fn sequence_tail(text: &str, at: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut tail = None;
    let mut i = at;
    while i < text.len() {
        i = skip_trivia(text, i);
        let rest = &text[i..];
        let c = rest.chars().next()?;
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => return tail.map(|t| skip_trivia(text, t)),
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => tail = Some(i + 1),
            '"' | '\'' | '`' => {
                let mut chars = rest.char_indices().skip(1);
                i += loop {
                    match chars.next()? {
                        (_, '\\') => {
                            chars.next();
                        }
                        (j, q) if q == c => break j,
                        _ => {}
                    }
                };
            }
            _ => {}
        }
        i += c.len_utf8();
    }
    None
}
