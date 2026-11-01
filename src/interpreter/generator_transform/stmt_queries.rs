use super::*;

pub(super) fn stmt_contains_for_of_head(
    stmt: &Statement,
    head: impl Fn(&ForOfStatement) -> bool + Copy,
) -> bool {
    let contains = |s: &Statement| stmt_contains_for_of_head(s, head);
    match stmt {
        Statement::ForOf(f) => head(f) || contains(&f.body),
        Statement::Block(stmts) => stmts.iter().any(contains),
        Statement::If(i) => {
            contains(&i.consequent) || i.alternate.as_ref().is_some_and(|s| contains(s))
        }
        Statement::While(w) => contains(&w.body),
        Statement::DoWhile(d) => contains(&d.body),
        Statement::For(f) => contains(&f.body),
        Statement::ForIn(f) => contains(&f.body),
        Statement::Try(t) => {
            t.block.iter().any(contains)
                || t.handler
                    .as_ref()
                    .is_some_and(|h| h.body.iter().any(contains))
                || t.finalizer.as_ref().is_some_and(|f| f.iter().any(contains))
        }
        Statement::Switch(s) => s.cases.iter().any(|c| c.consequent.iter().any(contains)),
        Statement::Labeled(_, inner) => contains(inner),
        Statement::With(_, s) => contains(s),
        _ => false,
    }
}

pub(super) fn stmt_contains_return(stmt: &Statement) -> bool {
    match stmt {
        Statement::Return(_) => true,
        Statement::Block(stmts) => stmts.iter().any(stmt_contains_return),
        Statement::If(i) => {
            stmt_contains_return(&i.consequent)
                || i.alternate
                    .as_ref()
                    .is_some_and(|s| stmt_contains_return(s))
        }
        Statement::While(w) => stmt_contains_return(&w.body),
        Statement::DoWhile(d) => stmt_contains_return(&d.body),
        Statement::For(f) => stmt_contains_return(&f.body),
        Statement::ForIn(f) => stmt_contains_return(&f.body),
        Statement::ForOf(f) => stmt_contains_return(&f.body),
        Statement::Try(t) => {
            t.block.iter().any(stmt_contains_return)
                || t.handler
                    .as_ref()
                    .is_some_and(|h| h.body.iter().any(stmt_contains_return))
                || t.finalizer
                    .as_ref()
                    .is_some_and(|f| f.iter().any(stmt_contains_return))
        }
        Statement::Switch(s) => s
            .cases
            .iter()
            .any(|c| c.consequent.iter().any(stmt_contains_return)),
        Statement::Labeled(_, inner) => stmt_contains_return(inner),
        Statement::With(_, s) => stmt_contains_return(s),
        // Don't recurse into function/class boundaries
        _ => false,
    }
}
