use super::*;

impl Parser<'_> {
    /// The position a V8 stack trace reports for a call whose `(` is the
    /// current token: the callee's last identifier when it ends the previous
    /// token (not parenthesized or written with escapes), else the parenthesis.
    /// V8 reports the parenthesis for a private name (`this.#m(`) too.
    pub(super) fn call_site(&self, callee: &Expression) -> CallSiteId {
        let name: &str = match callee {
            Expression::Identifier(name) | Expression::Member(_, MemberProperty::Dot(name), _) => {
                name.as_ref()
            }
            _ => return CallSiteId::at(self.current_token_start),
        };
        let end = self.prev_token_end;
        let start = end.saturating_sub(name.len());
        let written = self.source.get(start..end);
        if written == Some(name) && !name.is_empty() {
            CallSiteId::at(start)
        } else {
            CallSiteId::at(self.current_token_start)
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
