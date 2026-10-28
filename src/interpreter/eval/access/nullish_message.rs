use super::*;

impl Interpreter {
    /// Message of the TypeError for `base[key]` with a nullish base. The key is not converted (ToObject
    /// precedes ToPropertyKey); like V8, the well-known `Symbol.iterator` gets the "not iterable" wording.
    pub(crate) fn nullish_computed_read_message(&self, base: &JsValue, key: &JsValue) -> String {
        if let Some(sym) = key.as_symbol()
            && self.get_symbol_iterator_key() == Some(sym.to_property_key())
        {
            let shown = if base.is_null() {
                "object null".to_string()
            } else {
                base.to_string()
            };
            return format!(
                "{shown} is not iterable (cannot read property Symbol(Symbol.iterator))"
            );
        }
        format!("Cannot read properties of {base} (reading property)")
    }
}
