mod for_head_site;

use super::*;

impl Parser<'_> {
    /// The position a V8 stack trace reports for a call whose `(` is the
    /// current token: the callee's last identifier when it ends the previous
    /// token (not parenthesized or written with escapes), else the parenthesis.
    /// V8 reports the parenthesis for a private name (`this.#m(`) too, and
    /// for a property named like a reserved word (`it.return(`).
    pub(super) fn call_site(&self, callee: &Expression) -> CallSiteId {
        let name: &str = match callee {
            Expression::Identifier(name) | Expression::Member(_, MemberProperty::Dot(name), _) => {
                name.as_ref()
            }
            _ => return CallSiteId::at(self.current_token_start),
        };
        if Self::is_reserved_identifier(name, false) {
            return CallSiteId::at(self.current_token_start);
        }
        let end = self.prev_token_end;
        let start = end.saturating_sub(name.len());
        let written = self.source.get(start..end);
        if written == Some(name) && !name.is_empty() {
            CallSiteId::at(start)
        } else {
            CallSiteId::at(self.current_token_start)
        }
    }

    /// V8 reports a failed `o.x = v` at the `=`, which is the current token:
    /// a plain assignment's member target takes that position (a compound
    /// one reads first, at the member's own).
    pub(super) fn retarget_store_site(&self, target: &mut Expression, op: AssignOp) {
        if let (AssignOp::Assign, Expression::Member(_, _, site)) = (op, target) {
            site.1 = PropSiteId::at(self.current_token_start).1;
        }
    }

    pub(super) fn check_cooked(
        &self,
        cooked: &Option<Vec<u16>>,
        tagged: bool,
    ) -> Result<(), ParseError> {
        if !tagged && cooked.is_none() {
            return Err(self.error("Invalid escape sequence in template literal"));
        }
        Ok(())
    }

    /// A template literal's cache id. A tagged template also
    /// carries its source offset plus one in the high 32 bits: V8 reports a
    /// failed tag call at the template (`interpreter::eval`, `note_template_pos`).
    pub(super) fn template_id(&self, tagged: bool, start: usize) -> u64 {
        let id = crate::ast::next_template_id();
        if tagged {
            id | (u64::try_from(start + 1).unwrap_or(0) << 32)
        } else {
            id
        }
    }

    /// The source every function of this parse shares as its `SourceText` origin.
    pub(crate) fn source_origin(&self) -> Rc<str> {
        self.source_text_source.clone()
    }

    /// Byte span of the token the parser stopped at (meaningful after an error).
    pub(crate) fn failure_span(&self) -> (usize, usize) {
        (self.current_token_start, self.current_token_end)
    }
}
