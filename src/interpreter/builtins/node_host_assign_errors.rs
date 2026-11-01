//! V8's text for a strict-mode write to a binding that cannot change, under
//! `--node` only (#1532). The engine models the global `undefined`, `NaN` and
//! `Infinity` as immutable bindings, so it reports a constant assignment; V8
//! sees read-only properties of the global object.

use super::super::*;

impl Interpreter {
    pub(crate) fn const_assign_error(&mut self, name: &str, scope: &EnvRef) -> JsValue {
        match std::mem::take(&mut self.node_host.assign_pos) {
            0 => {}
            at => self.node_host.call_pos = at,
        }
        let engine = "Assignment to constant variable.";
        let global_value =
            matches!(name, "undefined" | "NaN" | "Infinity") && scope.borrow().parent.is_none();
        let text = self.v8_message(engine, |_| {
            if global_value {
                format!("Cannot assign to read only property '{name}' of object '#<Object>'")
            } else {
                engine.to_string()
            }
        });
        self.create_type_error(&text)
    }
}
