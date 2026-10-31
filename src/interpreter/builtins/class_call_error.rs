//! The TypeError for calling a class constructor without `new` (#1336), worded
//! like V8 (the text is implementation-defined).

use super::super::Interpreter;
use crate::interpreter::types::JsFunction;
use crate::types::JsValue;

impl Interpreter {
    pub(crate) fn class_call_error(&mut self, class: u64) -> JsValue {
        let declared = self
            .get_object(class)
            .and_then(|o| match o.borrow().callable.as_deref() {
                Some(JsFunction::User { name, .. }) => name.clone(),
                _ => None,
            });
        let name = declared
            .filter(|n| !n.is_empty())
            .or_else(|| self.own_data_string(class, "name"))
            .unwrap_or_default();
        let text = if name.is_empty() {
            "Class constructors cannot be invoked without 'new'".to_string()
        } else {
            format!("Class constructor {name} cannot be invoked without 'new'")
        };
        self.create_type_error(&text)
    }
}
