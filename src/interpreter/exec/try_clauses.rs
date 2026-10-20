use super::*;

impl Interpreter {
    /// The `catch` clause of a try statement, entered with the thrown `val`.
    /// The clause's Block owns a scope of its own (§14.15.2 CatchClauseEvaluation
    /// evaluates it as a Block), so its `using` resources are disposed here.
    pub(super) fn exec_catch_clause(
        &mut self,
        handler: &CatchClause,
        val: JsValue,
        env: &EnvRef,
        has_finalizer: bool,
        saved_tco: u32,
    ) -> Completion {
        let catch_env = Environment::new(Some(env.clone()));
        if let Some(param) = &handler.param {
            if matches!(param, Pattern::Identifier(_)) {
                catch_env.borrow_mut().is_simple_catch_scope = true;
            }
            propagate!(self.bind_pattern(param, val, BindingKind::Let, &catch_env));
        }
        let catch_block_env = Environment::new(Some(catch_env));
        // A call in the catch Block is in tail position only when there is no
        // finally; otherwise the finalizer must run.
        if has_finalizer {
            self.tco_suppress_depth = saved_tco + 1;
        }
        let r = self.exec_statements(&handler.body, &catch_block_env);
        self.tco_suppress_depth = saved_tco;
        self.dispose_resources(&catch_block_env, r)
    }

    /// The `finally` Block, run while `result` (the completion of the try and
    /// catch Blocks) is pending. Its own `using` resources are disposed before
    /// an abrupt outcome replaces `result`.
    pub(super) fn exec_finalizer(
        &mut self,
        finalizer: &[Statement],
        result: &Completion,
        env: &EnvRef,
    ) -> Completion {
        let fin_env = Environment::new(Some(env.clone()));
        self.with_gc_root_scope(|interp| {
            result.root_payload(|v| interp.gc_root_value(v));
            let r = interp.exec_statements(finalizer, &fin_env);
            interp.dispose_resources(&fin_env, r)
        })
    }
}
