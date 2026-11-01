//! V8's error position for a class heritage that is not a constructor or null
//! (#1511): the heritage expression, which the AST does not carry an offset
//! for. The class's own source text starts at `class`, so the offset is found
//! by reading its header: `class`, an optional name, `extends`, and any
//! parentheses around the expression (V8 points inside them).

use super::super::Interpreter;
use crate::ast::SourceText;

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$' | '\\' | '\u{200c}' | '\u{200d}')
}

fn skip_trivia(text: &str, mut at: usize) -> usize {
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
fn word_end(text: &str, at: usize) -> usize {
    let word: usize = text[at..]
        .chars()
        .take_while(|&c| is_word_char(c))
        .map(char::len_utf8)
        .sum();
    at + word
}

/// The offset in `text` of the heritage expression of the class it starts
/// with, if the header has that shape.
pub(crate) fn heritage_offset(text: &str) -> Option<usize> {
    let mut at = skip_trivia(text, 0);
    let end = word_end(text, at);
    if &text[at..end] != "class" {
        return None;
    }
    at = skip_trivia(text, end);
    let mut end = word_end(text, at);
    if &text[at..end] != "extends" {
        at = skip_trivia(text, end);
        end = word_end(text, at);
        if &text[at..end] != "extends" {
            return None;
        }
    }
    let mut at = skip_trivia(text, end);
    while text[at..].starts_with('(') {
        at = skip_trivia(text, at + 1);
    }
    Some(at)
}

/// The offset plus one of the heritage expression of the class whose source
/// text is `class`, 0 outside `--node` or when it has none.
pub(crate) fn heritage_pos(class: &Option<SourceText>) -> u32 {
    if !crate::interpreter::builtins::node_host_stack::call_positions_enabled() {
        return 0;
    }
    class
        .as_ref()
        .and_then(|text| {
            let at = text.origin().1 + heritage_offset(text.as_str())? + 1;
            u32::try_from(at).ok()
        })
        .unwrap_or(0)
}

impl Interpreter {
    /// The frame's position becomes the class's heritage expression, before
    /// the TypeError for a bad heritage is made.
    pub(crate) fn note_heritage_pos(&mut self, class: &Option<SourceText>) {
        if !self.node_host.stack.installed {
            return;
        }
        if let Some(text) = class
            && let Some(offset) = heritage_offset(text.as_str())
        {
            let start = text.origin().1;
            if let Ok(pos) = u32::try_from(start + offset + 1) {
                self.node_host.call_pos = pos;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::heritage_offset;

    #[test]
    fn finds_the_heritage_expression() {
        let at = |t: &str| heritage_offset(t).and_then(|i| t[i..].chars().next());
        assert_eq!(at("class B extends num {}"), Some('n'));
        assert_eq!(at("class extends (f()) {}"), Some('f'));
        assert_eq!(at("class extends ( /* c */ (n)) {}"), Some('n'));
        assert_eq!(
            at("class /* extends */ B // x\n extends /* c */ Z {}"),
            Some('Z')
        );
        assert_eq!(at("class B {}"), None);
        assert_eq!(at("class B extends"), None);
    }
}
