//! The position of a for-of head in `--node` stacks (issue #1529): V8 reports
//! it for every step of the loop's iterator and for the `await`s of a
//! `for await`.

use super::*;

impl Parser<'_> {
    /// Consumes the `of` that ends a for-of head and returns that position.
    pub(in crate::parser) fn advance_past_of(
        &mut self,
        head_start: usize,
    ) -> Result<u32, ParseError> {
        let at = self.for_head_site(head_start);
        self.advance()?;
        Ok(at)
    }

    /// The iterable of a for-of head and its first token's position.
    pub(in crate::parser) fn parse_for_of_right(
        &mut self,
    ) -> Result<(Expression, u32), ParseError> {
        let at = self.op_pos();
        Ok((self.parse_assignment_expression()?, at))
    }

    /// The head `source[head_start..prev_token_end]` is the loop's left side.
    /// V8 reports the binding's start, the closing bracket of a declared
    /// pattern, and a member's property (or its `[`).
    fn for_head_site(&self, head_start: usize) -> u32 {
        let head = self
            .source
            .get(head_start..self.prev_token_end)
            .unwrap_or("")
            .trim_end();
        let mut start = 0;
        let mut declared = false;
        for keyword in ["const", "let", "var"] {
            if let Some(rest) = head.strip_prefix(keyword)
                && rest.starts_with(|c: char| c.is_whitespace() || c == '[' || c == '{')
            {
                declared = true;
                start = head.len() - rest.trim_start().len();
            }
        }
        let binding = &head[start..];
        let pattern = binding.starts_with(['[', '{']);
        let at = if declared && pattern {
            head.len().saturating_sub(1)
        } else if let Some(open) = binding
            .strip_suffix(']')
            .filter(|_| !pattern)
            .and_then(Self::open_bracket)
        {
            start + open
        } else if let Some(dot) = binding.rfind('.').filter(|_| !declared && !pattern) {
            start + dot + 1
        } else {
            start
        };
        CallSiteId::at(head_start + at).1
    }

    /// The offset of the `[` matching the `]` that ended `text`.
    fn open_bracket(text: &str) -> Option<usize> {
        let mut depth = 1;
        for (offset, c) in text.char_indices().rev() {
            match c {
                ']' => depth += 1,
                '[' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                return Some(offset);
            }
        }
        None
    }
}
