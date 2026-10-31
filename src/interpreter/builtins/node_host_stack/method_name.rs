//! The method name of a `--node` stack frame (issue #1508): V8's
//! `CallSiteInfo::GetMethodName`, which decides the ` [as method]` suffix of
//! `at Timeout.tm [as _onTimeout]` and what `CallSite.getMethodName()` returns.
//! It only reads property records: no getter, proxy trap or script runs.

use super::*;

const CHAIN_LIMIT: usize = 16;
const KEYS_PER_OBJECT: usize = 128;

fn descriptor_holds(descriptor: &PropertyDescriptor, func: u64) -> bool {
    [&descriptor.value, &descriptor.get, &descriptor.set]
        .into_iter()
        .flatten()
        .any(|value| value.as_object_id() == Some(func))
}

/// V8 keeps element keys apart from names, so they never name a method.
fn is_array_index(key: &str) -> bool {
    key.parse::<u32>()
        .is_ok_and(|index| index != u32::MAX && index.to_string() == key)
}

impl Interpreter {
    /// `receiver[key]`, found along the prototype chain, is `func` or one half
    /// of an accessor pair that holds it.
    fn chain_property_holds(&self, receiver: u64, key: &str, func: u64) -> bool {
        let mut id = Some(receiver);
        for _ in 0..CHAIN_LIMIT {
            let Some(object) = id.and_then(|id| self.get_object(id)) else {
                return false;
            };
            let object = object.borrow();
            if object.is_proxy() {
                return false;
            }
            if let Some(descriptor) = object.get_own_property(key) {
                return descriptor_holds(&descriptor, func);
            }
            id = object.prototype_id;
        }
        false
    }

    /// The property name V8 reports as the method `func` was called through:
    /// its own name (without a `get `/`set ` prefix) if the receiver has a
    /// property of that name that holds it, else the one enumerable key on
    /// the receiver's prototype chain that holds it.
    pub(super) fn v8_method_name(
        &self,
        receiver: &JsValue,
        func: u64,
        function_name: &str,
    ) -> Option<String> {
        let receiver = receiver.as_object_id()?;
        let own_name = ["get ", "set "]
            .iter()
            .find_map(|prefix| function_name.strip_prefix(prefix))
            .unwrap_or(function_name);
        if self.chain_property_holds(receiver, own_name, func) {
            return Some(own_name.to_string());
        }
        let mut found = None;
        let mut id = Some(receiver);
        for _ in 0..CHAIN_LIMIT {
            let Some(object) = id.and_then(|id| self.get_object(id)) else {
                break;
            };
            let object = object.borrow();
            if object.is_proxy() {
                break;
            }
            for (key, record) in object.properties.iter().take(KEYS_PER_OBJECT) {
                let Some(key) = key
                    .as_str()
                    .filter(|key| record.enumerable() == Some(true) && !is_array_index(key))
                else {
                    continue;
                };
                if !descriptor_holds(&record.to_descriptor(), func) {
                    continue;
                }
                if found.is_some() {
                    return None;
                }
                found = Some(key.to_string());
            }
            id = object.prototype_id;
        }
        found
    }

    /// The method-name slot of the frame record being built.
    pub(super) fn frame_method(
        &self,
        raw: &RawFrame,
        qualified: bool,
        name: &Option<String>,
    ) -> JsValue {
        let method = match &raw.receiver {
            Some(receiver) if qualified && !raw.constructor => {
                self.v8_method_name(receiver, raw.func, name.as_deref().unwrap_or(""))
            }
            _ => None,
        };
        method.map_or(JsValue::UNDEFINED, |method| string_value(&method))
    }
}
