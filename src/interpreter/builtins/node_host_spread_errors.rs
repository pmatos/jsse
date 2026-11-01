//! V8's wording for a call or `new` argument spread of something that is not
//! iterable, under `--node` only (#1512). A nullish operand reads like a
//! failed property read (`u is not iterable (cannot read property undefined)`),
//! anything else with no callable `@@iterator` is `Spread syntax requires
//! ...iterable[Symbol.iterator] to be a function`. That is V8's CallWithSpread,
//! used only when the sole spread is the last argument; any other spread is
//! built like an array literal and keeps the value form (`number 5 is not
//! iterable (cannot read property Symbol(Symbol.iterator))`). Array literals
//! and `for-of` keep their own `x is not iterable` (`node_host_destructure.rs`).

use super::super::*;
use super::node_host_expr_text::expr_text;

/// Whether `operand` is the only spread of `args` and the last argument.
pub(crate) fn is_lone_last_spread(args: &[Expression], operand: &Expression) -> bool {
    let spreads = args
        .iter()
        .filter(|arg| matches!(arg, Expression::Spread(..)))
        .count();
    spreads == 1
        && matches!(args.last(), Some(Expression::Spread(last, _)) if std::ptr::eq(&**last, operand))
}

impl Interpreter {
    /// The tree-walker's spread of `inner` (whose value is `value`) in call
    /// arguments.
    pub(crate) fn call_spread_items(
        &mut self,
        inner: &Expression,
        value: &JsValue,
        args: &[Expression],
    ) -> Result<Vec<JsValue>, JsValue> {
        match self.spread_values(inner, value) {
            Err(e) if self.node_host.stack.installed => {
                let text = is_lone_last_spread(args, inner).then(|| expr_text(inner));
                Err(self.reword_spread_text(e, value, text))
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
        let Some(shown) = text else {
            return error;
        };
        let message = if value.is_nullish() {
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
