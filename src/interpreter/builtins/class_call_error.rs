//! The TypeError for calling a class constructor without `new` (#1336).
//!
//! The text is implementation-defined; this follows V8: `Class constructor A
//! cannot be invoked without 'new'`, and `Class constructors cannot be invoked
//! without 'new'` when the class has no name.

use super::super::Interpreter;
use crate::interpreter::types::JsFunction;
use crate::types::JsValue;

impl Interpreter {
    /// V8 prints the name the class was declared or inferred with, so a later
    /// redefinition of its own `name` property does not show; the own `name`
    /// data property only fills in for classes the parser saw unnamed
    /// (`const A = class {}`).
    fn class_display_name(&self, class: u64) -> String {
        let Some(obj) = self.get_object(class) else {
            return String::new();
        };
        let obj = obj.borrow();
        if let Some(JsFunction::User {
            name: Some(name), ..
        }) = obj.callable.as_deref()
            && !name.is_empty()
        {
            return name.clone();
        }
        obj.get_own_property("name")
            .and_then(|p| p.value)
            .and_then(|v| v.as_string().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    pub(crate) fn class_call_error(&mut self, class: u64) -> JsValue {
        let name = self.class_display_name(class);
        let text = if name.is_empty() {
            "Class constructors cannot be invoked without 'new'".to_string()
        } else {
            format!("Class constructor {name} cannot be invoked without 'new'")
        };
        self.create_type_error(&text)
    }
}
