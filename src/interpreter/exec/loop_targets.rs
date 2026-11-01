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

    /// For a loop sharing one iteration environment, the `let`/`const`
    /// identifier its head binds, interned once so each iteration installs it
    /// without hashing the name again.
    pub(super) fn simple_head_binding(
        left: &ForInOfLeft,
        shared_env: Option<&EnvRef>,
    ) -> Option<(Atom, BindingKind)> {
        shared_env?;
        let ForInOfLeft::Variable(decl) = left else {
            return None;
        };
        let kind = match decl.kind {
            VarKind::Let => BindingKind::Let,
            VarKind::Const => BindingKind::Const,
            _ => return None,
        };
        match decl.declarations.first().map(|d| &d.pattern) {
            Some(Pattern::Identifier(name)) => Some((Atom::intern(name), kind)),
            _ => None,
        }
    }
}
