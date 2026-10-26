use super::*;

impl Parser<'_> {
    /// The position a V8 stack trace reports for a call whose `(` is the
    /// current token: the callee's last identifier when it ends the previous
    /// token (not parenthesized or written with escapes), else the parenthesis.
    pub(super) fn call_site(&self, callee: &Expression) -> CallSiteId {
        let (name, extra): (&str, usize) = match callee {
            Expression::Identifier(name) | Expression::Member(_, MemberProperty::Dot(name), _) => {
                (name.as_ref(), 0)
            }
            Expression::Member(_, MemberProperty::Private(name), _) => (name.as_ref(), 1),
            _ => return CallSiteId::at(self.current_token_start),
        };
        let end = self.prev_token_end;
        let start = end.saturating_sub(name.len() + extra);
        let written = self.source.get(start + extra..end);
        if written == Some(name) && !name.is_empty() {
            CallSiteId::at(start)
        } else {
            CallSiteId::at(self.current_token_start)
        }
    }
}
