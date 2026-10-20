use super::*;

impl Interpreter {
    /// PutValue of one iteration's value to a for-in/of identifier target
    /// (§14.7.5.7 ForIn/OfBodyEvaluation): the reference is resolved afresh, so
    /// a binding still in its TDZ or a `const` throws and strict code cannot
    /// create an implicit global.
    pub(super) fn assign_loop_identifier_target(
        &mut self,
        name: &str,
        value: JsValue,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        let id_ref = self.resolve_identifier_ref(name, env)?;
        match self.put_value_by_ref(name, value, &id_ref, env) {
            Completion::Throw(e) => Err(e),
            _ => Ok(()),
        }
    }
}
