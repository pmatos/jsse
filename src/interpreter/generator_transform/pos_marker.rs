//! Statement positions across the lowering (#1511). A statement split at a
//! suspension becomes several state statements, none of which is the original,
//! so the position V8 reports for an error the first of them throws would be
//! lost. Under `--node` a no-op statement carrying the original position
//! leads them.

use super::*;

pub(super) fn emit_pos_marker(ctx: &mut TransformContext, at: u32) {
    if at != 0 {
        ctx.emit_statement(Statement::Expression(
            Expression::Literal(Literal::Null),
            at,
        ));
    }
}
