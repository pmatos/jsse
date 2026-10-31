use super::*;

impl Interpreter {
    /// Evaluates the module graph a dynamic `import()` loaded. When evaluation
    /// throws, every module still on the DFS stack is left errored with that
    /// value (§16.2.1.5.3.1 InnerModuleEvaluation step: abrupt completion), and
    /// the error is returned.
    pub(super) fn evaluate_dynamic_import(&mut self, resolved: &ModuleKey) -> Result<(), JsValue> {
        let canon = resolved.canonicalize();
        let mut stack = vec![];
        let Err(error) = self.inner_module_evaluation(&canon, &mut stack, 0) else {
            return Ok(());
        };
        for path in &stack {
            if let Some(module) = self.module_registry_get(path) {
                let mut module = module.borrow_mut();
                module.evaluated = true;
                module.is_evaluating = false;
                if module.error.is_none() {
                    module.error = Some(error.clone());
                }
            }
        }
        Err(error)
    }
}
