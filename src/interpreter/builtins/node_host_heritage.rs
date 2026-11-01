//! V8's error position for a class heritage that is not a constructor or null
//! (#1511): the heritage expression, which the AST does not carry an offset
//! for. The class's own source text starts at `class`, so the offset is found
//! by reading its header: `class`, an optional name, `extends`, and any
//! parentheses around the expression (V8 points inside them).

use super::super::Interpreter;
use crate::ast::SourceText;
use crate::parser::source_scan::{sequence_tail, skip_trivia, word_end};

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
    let mut inside = Vec::new();
    while text[at..].starts_with('(') {
        at = skip_trivia(text, at + 1);
        inside.push(at);
    }
    // The comma expression may sit in any of the parentheses.
    let tail = inside.iter().find_map(|&start| sequence_tail(text, start));
    Some(tail.unwrap_or(at))
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

impl Interpreter {
    /// The TypeError of a `super()` call whose parent is not a constructor;
    /// under `--node`, V8's `Super constructor null of A is not a
    /// constructor`.
    pub(crate) fn super_constructor_error(
        &mut self,
        parent: &crate::types::JsValue,
    ) -> crate::types::JsValue {
        if self.node_host.stack.installed {
            let named = |interp: &Self, id: u64| {
                interp
                    .own_data_string(id, "name")
                    .map(|name| name.to_string())
                    .filter(|name| !name.is_empty())
            };
            let class = self
                .call_stack_frames
                .last()
                .and_then(|frame| named(self, frame.func_obj_id))
                .map_or_else(
                    || "of anonymous class".to_string(),
                    |name| format!("of {name}"),
                );
            let shown = match parent.as_object_id() {
                Some(id) if self.is_callable(parent) => {
                    named(self, id).unwrap_or_else(|| "null".to_string())
                }
                _ => self.v8_plain_text(parent),
            };
            let text = format!("Super constructor {shown} {class} is not a constructor");
            return self.create_type_error(&text);
        }
        self.create_type_error("Super constructor is not a constructor")
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
