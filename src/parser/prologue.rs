use super::*;

impl<'a> Parser<'a> {
    pub(super) fn is_directive_prologue(&self, stmt: &Statement) -> Option<String> {
        match stmt {
            Statement::Expression(Expression::Literal(Literal::String(s))) => {
                if self.last_string_literal_has_escape {
                    None
                } else {
                    Some(String::from_utf16_lossy(s))
                }
            }
            _ => None,
        }
    }

    pub(super) fn is_string_literal_statement(stmt: &Statement) -> bool {
        matches!(
            stmt,
            Statement::Expression(Expression::Literal(Literal::String(_)))
        )
    }
}
