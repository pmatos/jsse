//! Accessors for `JsObjectData` slots stored compactly (#1401): the primitive
//! value of a wrapper, Date or FinalizationRegistry lives in
//! [`ObjectExtras`](super::ObjectExtras); the [[Prototype]] object id is a
//! 32-bit slab index (object ids stay below `u32::MAX`); and the function
//! realm and the generator realm share one slot.

use super::{JsObjectData, NO_PROTOTYPE, NO_REALM};
use crate::types::JsValue;

/// Set in `JsObjectData::realm` when it holds a generator's realm rather than
/// a function's; an object is one or the other, never both.
const GENERATOR_REALM: u32 = 1 << 31;

impl JsObjectData {
    #[inline]
    pub(crate) fn primitive_value(&self) -> Option<&JsValue> {
        self.extras.as_ref()?.primitive_value.as_ref()
    }

    pub(crate) fn set_primitive_value(&mut self, value: JsValue) {
        self.extras_mut().primitive_value = Some(value);
    }

    /// The [[Prototype]] object id, `None` for null.
    #[inline(always)]
    pub(crate) fn proto_id(&self) -> Option<u64> {
        (self.prototype != NO_PROTOTYPE).then_some(u64::from(self.prototype))
    }

    #[inline(always)]
    pub(crate) fn set_proto(&mut self, id: Option<u64>) {
        self.prototype = match id {
            Some(id) => {
                debug_assert!(id < u64::from(NO_PROTOTYPE));
                id as u32
            }
            None => NO_PROTOTYPE,
        };
    }

    #[inline]
    pub(crate) fn function_realm(&self) -> Option<usize> {
        (self.realm & GENERATOR_REALM == 0 && self.realm != NO_REALM).then_some(self.realm as usize)
    }

    #[inline]
    pub(crate) fn set_function_realm(&mut self, realm_id: usize) {
        debug_assert!(realm_id < GENERATOR_REALM as usize);
        self.realm = realm_id as u32;
    }

    #[inline]
    pub(crate) fn generator_realm_id(&self) -> Option<usize> {
        (self.realm & GENERATOR_REALM != 0 && self.realm != NO_REALM)
            .then_some((self.realm & !GENERATOR_REALM) as usize)
    }

    #[inline]
    pub(crate) fn set_generator_realm_id(&mut self, realm_id: usize) {
        debug_assert!(realm_id < GENERATOR_REALM as usize);
        self.realm = realm_id as u32 | GENERATOR_REALM;
    }
}
