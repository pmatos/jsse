//! V8's wording for a call or `new` argument spread of something that is not
//! iterable, under `--node` only (#1512). A nullish operand reads like a
//! failed property read (`u is not iterable (cannot read property undefined)`),
//! anything else with no callable `@@iterator` is `Spread syntax requires
//! ...iterable[Symbol.iterator] to be a function`. Array literals and `for-of`
//! keep their own `x is not iterable` (`node_host_destructure.rs`).

use super::super::*;
use super::node_host_expr_text::expr_text;

impl Interpreter {
    /// The tree-walker's spread of `inner` (whose value is `value`) in call
    /// arguments.
    pub(crate) fn call_spread_items(
        &mut self,
        inner: &Expression,
        value: &JsValue,
    ) -> Result<Vec<JsValue>, JsValue> {
        match self.spread_values(inner, value) {
            Err(e) if self.node_host.stack.installed => {
                Err(self.reword_spread_text(e, value, Some(expr_text(inner))))
            }
            other => other,
        }
    }

    fn reword_spread_text(
        &mut self,
        error: JsValue,
        value: &JsValue,
        text: Option<String>,
    ) -> JsValue {
        if !self.iterable_error_here(&error) {
            return error;
        }
        let message = if value.is_nullish() {
            let shown = text.unwrap_or_else(|| value.to_string());
            let what = if value.is_null() { "null" } else { "undefined" };
            format!("{shown} is not iterable (cannot read property {what})")
        } else {
            "Spread syntax requires ...iterable[Symbol.iterator] to be a function".to_string()
        };
        self.create_type_error(&message)
    }

    /// The bytecode's, whose compiler noted the operand text.
    pub(crate) fn iterate_call_spread_noted(
        &mut self,
        value: &JsValue,
    ) -> Result<Vec<JsValue>, JsValue> {
        let noted = self.node_host.pattern_src.take().map(|t| t.to_string());
        match self.iterate_to_vec(value) {
            Err(e) if self.node_host.stack.installed => {
                Err(self.reword_spread_text(e, value, noted))
            }
            other => other,
        }
    }
}
