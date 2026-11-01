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
