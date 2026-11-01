//! V8's position for a null or undefined value assigned to an object literal
//! (#1511): the first property's value target, else the `{` (no property, a
//! computed first key). The literal's tokens are not kept, so the first
//! property is read back from the source text; `--node` only.

use super::*;
use crate::parser::source_scan::{skip_trivia, word_end};

fn key_end(text: &str, at: usize) -> usize {
    let rest = &text[at..];
    match rest.chars().next() {
        Some(quote @ ('"' | '\'')) => {
            let mut escaped = false;
            for (i, c) in rest.char_indices().skip(1) {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == quote {
                    return at + i + 1;
                }
            }
            text.len()
        }
        Some(c) if c.is_ascii_digit() || c == '.' => {
            at + rest
                .find(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '_')))
                .unwrap_or(rest.len())
        }
        _ => word_end(text, at),
    }
}

impl Parser<'_> {
    /// The `at` of an assignment: an object literal target reports at its
    /// first target (`start` is where the literal begins), every other
    /// assignment at its operator.
    pub(in crate::parser) fn assign_pos(
        &self,
        left: &Expression,
        op: AssignOp,
        start: usize,
    ) -> u32 {
        if !inferred_names::enabled()
            || op != AssignOp::Assign
            || !matches!(left, Expression::Object(..))
        {
            return self.op_pos();
        }
        let text = &*self.source_text_source;
        let first = skip_trivia(text, start + 1);
        let target = match text.get(first..) {
            Some(rest) if rest.starts_with("...") => skip_trivia(text, first + 3),
            Some(rest) if rest.starts_with(['}', '[']) => start,
            Some(_) => {
                let after = skip_trivia(text, key_end(text, first));
                if text[after..].starts_with(':') {
                    skip_trivia(text, after + 1)
                } else {
                    first
                }
            }
            None => start,
        };
        u32::try_from(target + 1).unwrap_or(0)
    }
}

impl Parser<'_> {
    /// The operand of a spread element, after its `...`, with the position V8
    /// reports for an operand that is not iterable.
    pub(in crate::parser) fn parse_spread_operand(&mut self) -> Result<Expression, ParseError> {
        let start = self.op_pos();
        let operand = self.parse_assignment_expression()?;
        let at = operand.tail_pos(start);
        Ok(Expression::Spread(ExprBox::new(operand), at))
    }
}
