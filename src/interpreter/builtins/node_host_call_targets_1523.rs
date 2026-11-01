//! V8's wording of the ReferenceError a call used as an assignment target
//! throws, under `--node` only (#1523). Without the stack hooks each keeps the
//! engine's own text.

use super::super::{Completion, EnvRef, Interpreter};
use crate::ast::ForInOfLeft;
use crate::types::JsValue;

impl Interpreter {
    /// `for (f() of c)` / `for (f() in c)`: V8 words both as one loop error.
    /// `kind` is `in`, `of`, or `in/of` where the state machine of a
    /// generator or async function does not know which loop it runs.
    pub(crate) fn invalid_for_target(&mut self, kind: &str) -> JsValue {
        let text = if self.node_host.stack.installed {
            "Invalid left-hand side in for-loop".to_string()
        } else {
            format!("Invalid left-hand side in for-{kind} loop")
        };
        self.create_reference_error(&text)
    }

    /// `f()++` / `--f()`.
    pub(crate) fn invalid_update_target(&mut self, prefix: bool) -> JsValue {
        let operation = match (self.node_host.stack.installed, prefix) {
            (false, _) => "update expression",
            (true, true) => "prefix operation",
            (true, false) => "postfix operation",
        };
        self.create_reference_error(&format!("Invalid left-hand side expression in {operation}"))
    }

    /// The left side of a head of the generator and async-function state
    /// machines that is not a declaration. Only a call is left as an
    /// expression (web compatibility; every other target became a pattern):
    /// it runs and then throws, like the loops that do not suspend.
    pub(crate) fn assign_to_for_left(
        &mut self,
        left: &ForInOfLeft,
        value: JsValue,
        env: &EnvRef,
    ) -> Completion {
        match left {
            ForInOfLeft::Pattern(pattern) => self.assign_to_for_pattern(pattern, value, env),
            ForInOfLeft::Expression(call) => match self.eval_expr(call, env) {
                Completion::Normal(_) => Completion::Throw(self.invalid_for_target("in/of")),
                other => other,
            },
            ForInOfLeft::Variable(_) => Completion::Normal(JsValue::UNDEFINED),
        }
    }
}
