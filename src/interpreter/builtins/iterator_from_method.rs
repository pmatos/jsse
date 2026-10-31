//! `GetIteratorFromMethod`, split out of `iterators.rs`.

use super::super::*;

impl Interpreter {
    /// `GetMethod(items, @@iterator)` of `Array.from`; under `--node` a
    /// method that is not callable is V8's `%Array%.from requires ...`.
    pub(crate) fn items_iterator_method(
        &mut self,
        items: &JsValue,
    ) -> Result<Option<JsValue>, Completion> {
        let Some(key) = self.get_symbol_key("iterator") else {
            return Ok(None);
        };
        match self.access_property_on_value(items, &key) {
            Completion::Normal(v) if v.is_nullish() => Ok(None),
            Completion::Normal(v) => {
                if self.node_host.stack.installed && !self.is_callable(&v) {
                    let text = "%Array%.from requires that the property of the first argument, items[Symbol.iterator], when exists, be a function";
                    return Err(Completion::Throw(self.create_type_error(text)));
                }
                Ok(Some(v))
            }
            other => Err(other),
        }
    }

    /// GetIteratorFromMethod (§7.4.3): calls `method` on `obj`, then reads
    /// `next` once and keeps it as the iterator's `[[NextMethod]]`.
    pub(crate) fn get_iterator_from_method(
        &mut self,
        method: &JsValue,
        obj: &JsValue,
    ) -> Result<JsValue, JsValue> {
        if self.node_host.stack.installed && !self.is_callable(method) {
            return Err(self.not_iterable_error(obj));
        }
        match self.call_function(method, obj, &[]) {
            Completion::Normal(v) => {
                if let Some(v_id) = v.as_object_id() {
                    let next_method = match self.get_object_property(v_id, "next", &v) {
                        Completion::Normal(n) => n,
                        Completion::Throw(e) => return Err(e),
                        _ => JsValue::UNDEFINED,
                    };
                    self.iterator_next_cache.insert(v_id, next_method);
                    Ok(v)
                } else {
                    Err(self
                        .create_type_error("Result of the Symbol.iterator method is not an object"))
                }
            }
            Completion::Throw(e) => Err(e),
            _ => Err(self.not_iterable_error(obj)),
        }
    }
}
