use super::*;

impl Interpreter {
    /// A `return f()` that reached a scope with pending resources was
    /// evaluated as a tail call, but DisposeResources must run after the
    /// return value is computed (`using` is not a tail-call context). Runs the
    /// call to its result before the disposers start; any other completion is
    /// returned untouched.
    pub(crate) fn settle_tail_call(
        &mut self,
        stack: &[DisposableResource],
        completion: Completion,
    ) -> Completion {
        let Completion::TailCall { func, this, args } = completion else {
            return completion;
        };
        let result = self.with_gc_root_scope(|interp| {
            for resource in stack {
                interp.gc_root_value(&resource.value);
                interp.gc_root_value(&resource.dispose_method);
            }
            interp.call_function(&func, &this, &args)
        });
        match result {
            Completion::Normal(v) => Completion::Return(v),
            other => other,
        }
    }
}
