//! V8's text, under `--node` only, for a few builtins whose engine message
//! names no method: symbol-keyed methods called on the wrong receiver, a
//! non-callable `@@iterator` handed to a typed array, and an out-of-range
//! `Array.prototype.with` index (#1544). `frame_wording` finds the running
//! builtin by its string key, so the symbol-keyed ones are matched by name.

use super::super::*;

impl Interpreter {
    /// `frame_wording` for the builtins named `[Symbol.x]`.
    pub(super) fn symbol_frame_wording(
        &self,
        msg: &str,
        name: &str,
        receiver: &JsValue,
    ) -> Option<String> {
        match (name, msg) {
            ("[Symbol.iterator]", "String.prototype method called on null or undefined") => {
                Some("String.prototype[Symbol.iterator] called on null or undefined".to_string())
            }
            ("[Symbol.toPrimitive]", "this is not an object") => {
                Some(self.incompatible("Date.prototype [ @@toPrimitive ]", receiver))
            }
            ("[Symbol.toPrimitive]", "Symbol[Symbol.toPrimitive] requires a Symbol") => Some(
                "Symbol.prototype [ @@toPrimitive ] requires that 'this' be a Symbol".to_string(),
            ),
            _ => None,
        }
    }

    /// `new Uint8Array(items)` (`constructor`) or `Uint8Array.from(items)`
    /// where `items[Symbol.iterator]` is neither nullish nor callable.
    pub(crate) fn typed_array_iterator_error(&mut self, constructor: bool) -> JsValue {
        let engine = "@@iterator is not a function";
        let text = self.v8_message(engine, |_| {
            let who = if constructor {
                "TypedArray's constructor"
            } else {
                "%TypedArray%.from"
            };
            format!(
                "{who} requires that the property of the first argument, items[Symbol.iterator], when exists, be a function"
            )
        });
        self.create_type_error(&text)
    }

    /// `Array.prototype.with(index, value)` with an index outside the array;
    /// V8 prints the argument as it was passed.
    pub(crate) fn with_index_error(&mut self, args: &[JsValue]) -> JsValue {
        let text = self.v8_message("Invalid index", |i| {
            let shown = args
                .first()
                .map_or_else(|| "undefined".to_string(), |v| i.v8_plain_text(v));
            format!("Invalid index : {shown}")
        });
        self.create_range_error(&text)
    }

    /// `Date.prototype[Symbol.toPrimitive](hint)` with a hint that is none of
    /// `"string"`, `"number"` and `"default"`.
    pub(crate) fn invalid_hint_error(&mut self, args: &[JsValue]) -> JsValue {
        let text = self.v8_message("Invalid hint", |i| {
            let shown = args
                .first()
                .map_or_else(|| "undefined".to_string(), |v| i.v8_plain_text(v));
            format!("Invalid hint: {shown}")
        });
        self.create_type_error(&text)
    }
}
