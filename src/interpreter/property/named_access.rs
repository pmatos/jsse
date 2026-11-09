//! Named property reads and setter lookups that complete without the generic
//! [[Get]]/[[Set]] when every object on the way is ordinary.

use super::*;

impl Interpreter {
    /// The named read `base.key` resolved without the generic [[Get]] when the
    /// receiver and every object up to the property's holder are ordinary:
    /// the value, or the getter to call with `base`. `Slow` when the generic
    /// path must run (a primitive base, a Proxy or other exotic object on the
    /// way, an Array-index-like name).
    #[inline(always)]
    pub(in crate::interpreter) fn named_get_fast(
        &self,
        base: &JsValue,
        key: &JsPropertyKey,
        hint: Option<&Cell<u32>>,
    ) -> NamedGet {
        let Some(mut object_id) = base.as_object_id() else {
            return NamedGet::Slow;
        };
        if key_may_be_array_index(key) {
            return NamedGet::Slow;
        }
        for _ in 0..PROTO_WALK_LIMIT {
            let Some(cell) = self.get_object_cell(object_id) else {
                return NamedGet::Slow;
            };
            let object = cell.borrow_untracked();
            if object.kind.is_named_access_exotic() {
                return NamedGet::Slow;
            }
            if let Some(record) = object.properties.get_hinted(key, hint) {
                return match record.read() {
                    PropertyRead::Value(value) => NamedGet::Value(value.clone()),
                    PropertyRead::Getter(getter) => NamedGet::Getter(getter.clone()),
                };
            }
            if !object.named_miss_is_ordinary() {
                return NamedGet::Slow;
            }
            match object.proto_id() {
                Some(proto_id) => object_id = proto_id,
                None => return NamedGet::Value(JsValue::UNDEFINED),
            }
        }
        NamedGet::Slow
    }

    /// The setter that `base.key = v` calls, found by walking ordinary objects
    /// up the chain from `base`: the first object owning `key` must hold an
    /// accessor with a callable setter. `None` when anything else is
    /// involved (a data property, an accessor without a setter, a primitive
    /// base, a Proxy or other exotic object on the way), so the generic [[Set]]
    /// runs. It reads only, so a `None` leaves nothing changed.
    #[inline(always)]
    pub(in crate::interpreter) fn named_setter_fast(
        &self,
        base: &JsValue,
        key: &JsPropertyKey,
        hint: Option<&Cell<u32>>,
    ) -> Option<JsValue> {
        let mut object_id = base.as_object_id()?;
        if key_may_be_array_index(key) {
            return None;
        }
        for _ in 0..PROTO_WALK_LIMIT {
            let cell = self.get_object_cell(object_id)?;
            let object = cell.borrow_untracked();
            if object.kind.is_named_access_exotic() {
                return None;
            }
            if let Some(record) = object.properties.get_hinted(key, hint) {
                return record
                    .setter()
                    .filter(|setter| !setter.is_undefined())
                    .cloned();
            }
            if !object.named_miss_is_ordinary() {
                return None;
            }
            object_id = object.proto_id()?;
        }
        None
    }
}
