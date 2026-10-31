use super::*;

impl Interpreter {
    /// Message of the TypeError for `base[key]` with a nullish base. The key is not converted (ToObject
    /// precedes ToPropertyKey); like V8, the well-known `Symbol.iterator` gets the "not iterable" wording.
    pub(crate) fn nullish_computed_read_message(&self, base: &JsValue, key: &JsValue) -> String {
        match key.as_symbol() {
            Some(sym) if self.get_symbol_iterator_key() == Some(sym.to_property_key()) => {
                Self::not_iterable_read_message(base)
            }
            _ => format!(
                "Cannot read properties of {base} (reading {})",
                self.node_key_text(key).unwrap_or_else(|| "property".into())
            ),
        }
    }

    /// V8 names a primitive key in the message (`reading '0'`); the key is
    /// never converted, so an object key stays unnamed. `--node` only.
    fn node_key_text(&self, key: &JsValue) -> Option<String> {
        let named = self.node_host.stack.installed && !key.is_object() && !key.is_symbol();
        named.then(|| format!("'{key}'"))
    }

    pub(crate) fn nullish_read_error(&mut self, base: &JsValue, key: &JsValue) -> JsValue {
        let msg = self.nullish_computed_read_message(base, key);
        self.create_type_error(&msg)
    }

    /// Same as `nullish_computed_read_message` for a call site that has already converted the key.
    pub(crate) fn nullish_keyed_read_message(&self, base: &JsValue, key: &JsPropertyKey) -> String {
        if self.get_symbol_iterator_key().as_ref() == Some(key) {
            Self::not_iterable_read_message(base)
        } else {
            format!("Cannot read properties of {base} (reading '{key}')")
        }
    }

    fn not_iterable_read_message(base: &JsValue) -> String {
        let shown = if base.is_null() {
            "object null"
        } else {
            "undefined"
        };
        format!("{shown} is not iterable (cannot read property Symbol(Symbol.iterator))")
    }
}
