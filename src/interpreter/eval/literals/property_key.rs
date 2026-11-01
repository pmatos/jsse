use super::*;

impl Interpreter {
    /// `ToPropertyKey`, but returns the converted primitive as a `JsValue`
    /// (string or symbol) instead of the engine's internal `JsPropertyKey`
    /// representation — for the `ToPropertyKey` state-machine terminator,
    /// which stores the converted key back into a temp var for later reuse
    /// (see `docs/adr/` for issue #771: converting once and reusing the
    /// primitive avoids a second, user-observable `ToPropertyKey` call on a
    /// computed key ahead of an object-pattern rest).
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_property_key_value(&mut self, val: &JsValue) -> Result<JsValue, JsValue> {
        let key = self.to_property_key(val)?;
        Ok(self.symbol_key_to_jsvalue(&key))
    }
}
