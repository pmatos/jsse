use super::*;

impl Interpreter {
    /// Execute an `eval` / ShadowRealm-eval body. Like `exec_body`, this switches
    /// `current_ic` to the body's own IC store (dynamic code gets its own
    /// dense site namespace via `assign_ic_sites`). Unlike `exec_body`, it does
    /// NOT run the function-body hoisting pass: eval callers run
    /// `eval_declaration_instantiation` first (which applies the eval-specific
    /// var/function hoisting rules), and re-hoisting here with function-body
    /// semantics would corrupt those bindings. Returns the last statement's
    /// completion value (eval's result), matching PerformEval / PerformShadowRealmEval.
    pub(crate) fn exec_eval_body(&mut self, body: &Body, env: &EnvRef) -> Completion {
        // Same attribution gap as `exec_body`: eval'd code never passes through
        // `dispatch_body`, so it needs its own frame (#537 review).
        #[cfg(feature = "perf-counters")]
        {
            self.perf.body_non_function += 1;
            let name = self.perf.name_eval_body.clone();
            self.perf.enter_ast_body(
                name,
                crate::interpreter::perf_counters::SYNTHETIC_BODY_ID,
                false,
            );
        }
        let last = self.with_ic_body(body, |interp| {
            let mut last = Completion::Empty;
            for stmt in body.as_slice() {
                interp.gc_root_completion(&last);
                let comp = interp.exec_statement_at_safepoint(stmt, env);
                interp.gc_unroot_completion(&last);
                match comp {
                    Completion::Normal(v) => last = Completion::Normal(v),
                    Completion::Empty => {}
                    other => {
                        last = other;
                        break;
                    }
                }
            }
            last
        });
        #[cfg(feature = "perf-counters")]
        self.perf.leave_ast_body();
        last
    }
}
