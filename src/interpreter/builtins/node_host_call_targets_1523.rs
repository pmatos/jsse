//! V8's wording of the ReferenceError a call used as an assignment target
//! throws, under `--node` only (#1523). Without the stack hooks each keeps the
//! engine's own text.

use super::super::{Completion, EnvRef, Interpreter};
use crate::ast::{Expression, ForInOfLeft};
use crate::types::JsValue;

impl Interpreter {
    /// `for (f() of c)` / `for (f() in c)`: V8 words both as one loop error.
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

    /// The left side of a for-of/in head that is a pattern or an assignment
    /// target expression, for the generator and async-function state machine.
    /// Only a call (web compatibility) is left as an expression: it runs and
    /// then throws, like the loops that do not suspend.
    pub(crate) fn assign_to_for_left(
        &mut self,
        left: &ForInOfLeft,
        value: JsValue,
        env: &EnvRef,
    ) -> Completion {
        match left {
            ForInOfLeft::Pattern(pattern) => self.assign_to_for_pattern(pattern, value, env),
            ForInOfLeft::Expression(
                expr @ (Expression::Identifier(..) | Expression::Member(..)),
            ) => match self.assign_to_expr(expr, value, env) {
                Ok(()) => Completion::Normal(JsValue::UNDEFINED),
                Err(e) => Completion::Throw(e),
            },
            ForInOfLeft::Expression(expr) => match self.eval_expr(expr, env) {
                Completion::Normal(_) => Completion::Throw(self.invalid_for_target("of")),
                other => other,
            },
            ForInOfLeft::Variable(_) => Completion::Normal(JsValue::UNDEFINED),
        }
    }
}
