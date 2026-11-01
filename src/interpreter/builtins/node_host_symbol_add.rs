//! V8's wording for a Symbol operand of `+`, under `--node` only (#1511).

use super::super::Interpreter;
use crate::interpreter::helpers::is_string;
use crate::types::JsValue;

impl Interpreter {
    /// `Symbol() + ''` converts the symbol to a string, `Symbol() + 1`
    /// converts it to a number: the engine says "string" for both.
    pub(crate) fn symbol_add_error(&mut self, left: &JsValue, right: &JsValue) -> JsValue {
        let engine = "Cannot convert a Symbol value to a string";
        let text = self.v8_message(engine, |_| {
            if is_string(left) || is_string(right) {
                engine.to_string()
            } else {
                "Cannot convert a Symbol value to a number".to_string()
            }
        });
        self.create_type_error(&text)
    }
}
