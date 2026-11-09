//! Accessor calls: a getter or setter whose whole body is a single `this.name`
//! access runs without a call frame when the access completes in place.

use super::*;
use crate::interpreter::bytecode::chunk::TrivialAccessor;
use crate::interpreter::property::NamedGet;

/// Not a Proxy, wrapped function or class constructor.
fn is_plain_function(b: &JsObjectData) -> bool {
    !(b.is_proxy() || b.is_proxy_revoked() || b.wrapped().is_some() || b.is_class_constructor())
}

impl Interpreter {
    /// [[Call]] of an accessor's getter with no arguments. A getter whose
    /// whole body is `return this.#name;` or `return this.name;` runs no user
    /// code of its own, so the frame a call would build is unobservable.
    pub(crate) fn call_getter(&mut self, getter: &JsValue, receiver: &JsValue) -> Completion {
        if !self.bytecode_enabled {
            return self.call_function(getter, receiver, &[]);
        }
        if let Some(result) = self.call_trivial_private_getter(getter, receiver) {
            return result;
        }
        if let Some(value) = self.call_trivial_named_getter(getter, receiver) {
            return Completion::Normal(value);
        }
        self.call_function(getter, receiver, &[])
    }

    /// [[Call]] of an accessor's setter with the assigned value.
    pub(crate) fn call_setter(
        &mut self,
        setter: &JsValue,
        receiver: &JsValue,
        value: JsValue,
    ) -> Completion {
        if self.bytecode_enabled && self.call_trivial_named_setter(setter, receiver, &value) {
            return Completion::Normal(JsValue::UNDEFINED);
        }
        self.call_function(setter, receiver, &[value])
    }

    /// The chunk of an accessor function that may be served without a call.
    /// Callers also require an object receiver, which a call leaves as `this`.
    fn trivial_accessor_chunk(&self, function: &JsValue) -> Option<(Rc<Chunk>, TrivialAccessor)> {
        let obj = self.get_object_cell(function.as_object_id()?)?;
        let b = obj.borrow();
        let Some(JsFunction::User {
            is_arrow: false,
            is_generator: false,
            is_async: false,
            has_simple_params: true,
            bytecode_cache: BytecodeCacheState::Compiled(chunk),
            ..
        }) = b.callable.as_deref()
        else {
            return None;
        };
        let shape = chunk.trivial_accessor?;
        is_plain_function(&b).then(|| (chunk.clone(), shape))
    }

    fn call_trivial_named_getter(
        &mut self,
        getter: &JsValue,
        receiver: &JsValue,
    ) -> Option<JsValue> {
        receiver.as_object_id()?;
        let (chunk, TrivialAccessor::Getter(idx)) = self.trivial_accessor_chunk(getter)? else {
            return None;
        };
        let name = chunk.names.get(usize::from(idx))?;
        match self.named_get_fast(receiver, name.property_key(), chunk.prop_hint(idx)) {
            NamedGet::Value(value) => {
                self.count_trivial_hit();
                Some(value)
            }
            NamedGet::Getter(_) | NamedGet::Slow => None,
        }
    }

    fn call_trivial_named_setter(
        &mut self,
        setter: &JsValue,
        receiver: &JsValue,
        value: &JsValue,
    ) -> bool {
        if receiver.as_object_id().is_none() {
            return false;
        }
        let Some((chunk, TrivialAccessor::Setter(idx))) = self.trivial_accessor_chunk(setter)
        else {
            return false;
        };
        let Some(name) = chunk.names.get(usize::from(idx)) else {
            return false;
        };
        let stored =
            self.named_store_fast(receiver, name.property_key(), value, chunk.prop_hint(idx));
        if stored {
            self.count_trivial_hit();
        }
        stored
    }

    #[inline(always)]
    fn count_trivial_hit(&mut self) {
        #[cfg(feature = "perf-counters")]
        {
            self.perf.trivial_accessor_hits += 1;
        }
    }

    fn call_trivial_private_getter(
        &mut self,
        getter: &JsValue,
        receiver: &JsValue,
    ) -> Option<Completion> {
        let obj = self.get_object_cell(getter.as_object_id()?)?;
        let (chunk, idx, closure, function_realm) = {
            let b = obj.borrow();
            let Some(JsFunction::User {
                closure,
                is_arrow: false,
                is_strict: true,
                is_generator: false,
                is_async: false,
                has_simple_params: true,
                bytecode_cache: BytecodeCacheState::Compiled(chunk),
                ..
            }) = b.callable.as_deref()
            else {
                return None;
            };
            let idx = chunk.trivial_private_getter?;
            if !is_plain_function(&b) {
                return None;
            }
            (chunk.clone(), idx, closure.clone(), b.function_realm())
        };
        let name = chunk.private_names.get(usize::from(idx))?;
        self.count_trivial_hit();
        let id = self.resolve_private_name(name, &closure);
        Some(self.with_realm(function_realm, |interp| {
            interp.private_get_resolved(receiver, id, name)
        }))
    }
}
