//! V8's wording for a few operator TypeErrors, under `--node` only (#1419).
//! Without the stack hooks each keeps the engine's own text.

use super::super::Interpreter;
use crate::types::JsValue;

impl Interpreter {
    /// How V8 prints a value inside a message: a string bare, a primitive as
    /// written, an object as `#<Class>`. Never runs user code.
    pub(super) fn v8_value_text(&self, value: &JsValue) -> String {
        // V8 names an object by its constructor, not its class (`#<Object>`
        // for `Math`).
        let ctor = value
            .as_object_id()
            .and_then(|id| self.default_to_string_constructor(id));
        match (value.as_string(), ctor) {
            (Some(s), _) => s.to_string(),
            (None, Some(name)) => format!("#<{name}>"),
            (None, None) => self.describe_non_callable(value),
        }
    }

    /// `key in target` where `target` is not an object.
    pub(crate) fn in_operand_error(&mut self, key: &JsValue, target: &JsValue) -> JsValue {
        let text = if self.node_host.stack.installed && !key.is_object() {
            format!(
                "Cannot use 'in' operator to search for '{}' in {}",
                self.v8_value_text(key),
                self.v8_value_text(target)
            )
        } else {
            "Cannot use 'in' operator to search for a property in a non-object".to_string()
        };
        self.create_type_error(&text)
    }

    /// `x instanceof y` where `y` is not an object, or (`not_callable`) is
    /// an object that is not callable.
    pub(crate) fn instanceof_error(&mut self, not_callable: bool) -> JsValue {
        let quote = if self.node_host.stack.installed {
            "'instanceof'"
        } else {
            "instanceof"
        };
        let what = if not_callable {
            "callable"
        } else {
            "an object"
        };
        self.create_type_error(&format!("Right-hand side of {quote} is not {what}"))
    }

    /// `class C extends value` where `value` is not a constructor or null.
    pub(crate) fn class_extends_error(
        &mut self,
        value: &JsValue,
        class: &Option<crate::ast::SourceText>,
    ) -> JsValue {
        self.note_heritage_pos(class);
        let shown = if self.node_host.stack.installed {
            format!(" {}", self.v8_value_text(value))
        } else {
            String::new()
        };
        self.create_type_error(&format!(
            "Class extends value{shown} is not a constructor or null"
        ))
    }
}
