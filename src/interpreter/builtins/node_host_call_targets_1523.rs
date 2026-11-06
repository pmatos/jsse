//! V8's wording of the ReferenceError a call used as an assignment target
//! throws, under `--node` only (#1523). Without the stack hooks each keeps the
//! engine's own text.

use super::super::Interpreter;
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
}
