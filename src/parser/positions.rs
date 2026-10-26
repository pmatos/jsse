use super::*;

impl Parser<'_> {
    /// The position a V8 stack trace reports for a call whose `(` is the
    /// current token: the callee's last identifier, else the parenthesis.
    pub(super) fn call_site(&self, callee: &Expression) -> CallSiteId {
        let name_len = match callee {
            Expression::Identifier(name) | Expression::Member(_, MemberProperty::Dot(name), _) => {
                name.len()
            }
            Expression::Member(_, MemberProperty::Private(name), _) => name.len() + 1,
            _ => return CallSiteId::at(self.current_token_start),
        };
        CallSiteId::at(self.prev_token_end.saturating_sub(name_len))
    }
}
