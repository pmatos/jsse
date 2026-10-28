use super::*;

impl Interpreter {
    /// Message of the TypeError for `base[key]` with a nullish base. The key is not converted (ToObject
    /// precedes ToPropertyKey); like V8, the well-known `Symbol.iterator` gets the "not iterable" wording.
    pub(crate) fn nullish_computed_read_message(&self, base: &JsValue, key: &JsValue) -> String {
        match key.as_symbol() {
            Some(sym) if self.get_symbol_iterator_key() == Some(sym.to_property_key()) => {
                Self::not_iterable_read_message(base)
            }
            _ => format!("Cannot read properties of {base} (reading property)"),
        }
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
            "object null".to_string()
        } else {
            base.to_string()
        };
        format!("{shown} is not iterable (cannot read property Symbol(Symbol.iterator))")
    }
}
