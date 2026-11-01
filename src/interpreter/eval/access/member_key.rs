use super::*;

impl Interpreter {
    /// Evaluates a computed member's key expression and `ToPropertyKey` with
    /// `base` rooted, since either can run user code that collects.
    pub(in super::super) fn eval_member_key_rooted(
        &mut self,
        base: &JsValue,
        expr: &Expression,
        env: &EnvRef,
    ) -> Result<JsPropertyKey, Completion> {
        self.with_gc_root_scope(|this| {
            this.gc_root_value(base);
            let v = this
                .with_tail_position_suppressed(|this| this.eval_expr(expr, env))
                .into_abrupt()?;
            this.to_property_key(&v).into_abrupt()
        })
    }
}
