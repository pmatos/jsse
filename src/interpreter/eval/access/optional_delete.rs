use super::*;

impl Interpreter {
    /// Handle `delete obj?.prop` and `delete obj?.['prop']` etc.
    pub(in crate::interpreter::eval) fn eval_delete_optional_chain(
        &mut self,
        base: &Expression,
        chain: &Expression,
        env: &EnvRef,
    ) -> Completion {
        // Evaluate the base of the optional chain
        let (base_val, _base_this) = match self.eval_oc_base(base, env) {
            Ok(v) => v,
            Err(c) => return c,
        };
        // If base is null/undefined, short-circuit to true
        if (base_val).is_nullish() {
            return Completion::Normal(JsValue::boolean(true));
        }
        // Walk the chain to find the object and key to delete from
        self.eval_delete_oc_tail(&base_val, chain, env)
    }

    /// `base[key_expr]` inside an optional-chain tail, keeping `base` rooted
    /// while the key expression runs.
    pub(in crate::interpreter::eval) fn oc_computed_get(
        &mut self,
        base: &JsValue,
        key_expr: &Expression,
        env: &EnvRef,
    ) -> Result<JsValue, Completion> {
        self.with_gc_root_scope(|i| {
            i.gc_root_value(base);
            let key_val = i.eval_expr(key_expr, env).into_abrupt()?;
            let key = i.to_property_key(&key_val).into_abrupt()?;
            i.access_property_on_value(base, &key).into_abrupt()
        })
    }
}
