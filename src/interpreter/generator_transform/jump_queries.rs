use super::*;

pub(super) fn stmt_has_break_or_continue(stmt: &Statement) -> bool {
    match stmt {
        Statement::Break(..) | Statement::Continue(_) => true,
        Statement::Block(stmts) => stmts.iter().any(stmt_has_break_or_continue),
        Statement::If(if_stmt) => {
            stmt_has_break_or_continue(&if_stmt.consequent)
                || if_stmt
                    .alternate
                    .as_ref()
                    .is_some_and(|a| stmt_has_break_or_continue(a))
        }
        Statement::Labeled(_, inner) => stmt_has_break_or_continue(inner),
        // Don't recurse into nested loops/switch — their break/continue targets are separate
        _ => false,
    }
}
