//! Accessors for `JsObjectData` slots stored compactly (#1401): the primitive
//! value of a wrapper, Date or FinalizationRegistry lives in
//! [`ObjectExtras`](super::ObjectExtras), and the [[Prototype]] object id is a
//! 32-bit slab index (object ids are slab indices and stay below `u32::MAX`).

use super::{JsObjectData, NO_PROTOTYPE};
use crate::types::JsValue;

impl JsObjectData {
    #[inline]
    pub(crate) fn primitive_value(&self) -> Option<&JsValue> {
        self.extras.as_ref()?.primitive_value.as_ref()
    }

    pub(crate) fn set_primitive_value(&mut self, value: JsValue) {
        self.extras_mut().primitive_value = Some(value);
    }
}

impl JsObjectData {
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
}
