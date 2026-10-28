//! FinalizationRegistry cells and the weak-slot accessors of `JsObjectData`
//! (ADR-2026-10-28-0200).

use super::types::{JsObjectData, ObjectKind};
use crate::types::JsValue;

/// See [`ObjectKind::FinalizationRegistry`].
#[derive(Clone, Default)]
pub(crate) struct FinalizationRegistryData {
    /// A cell whose target is `undefined` has an empty [[WeakRefTarget]]: the
    /// collector found the target dead and its held value awaits cleanup.
    pub(crate) cells: Vec<Option<(JsValue, JsValue)>>,
    pub(crate) tokens: Vec<Option<JsValue>>,
    /// A cleanup job for this registry is already in the microtask queue.
    pub(crate) cleanup_queued: bool,
}

impl JsObjectData {
    #[inline]
    pub(crate) fn has_weak_slots(&self) -> bool {
        matches!(
            self.kind,
            ObjectKind::WeakRef(_) | ObjectKind::FinalizationRegistry(_)
        )
    }

    /// The target of a WeakRef (`undefined` once cleared).
    pub(crate) fn weak_ref_target(&self) -> Option<&JsValue> {
        if let ObjectKind::WeakRef(ref target) = self.kind {
            Some(target)
        } else {
            None
        }
    }

    pub(crate) fn finalization_registry_data_mut(
        &mut self,
    ) -> Option<&mut FinalizationRegistryData> {
        if let ObjectKind::FinalizationRegistry(ref mut data) = self.kind {
            Some(data)
        } else {
            None
        }
    }

    /// FinalizationRegistry cells (target+heldValue) + tokens (unregister keys).
    pub(crate) fn finalization_registry(
        &self,
    ) -> Option<(&Vec<Option<(JsValue, JsValue)>>, &Vec<Option<JsValue>>)> {
        if let ObjectKind::FinalizationRegistry(ref data) = self.kind {
            Some((&data.cells, &data.tokens))
        } else {
            None
        }
    }

    /// FinalizationRegistry cells + tokens — mutable view.
    pub(crate) fn finalization_registry_mut(
        &mut self,
    ) -> Option<(
        &mut Vec<Option<(JsValue, JsValue)>>,
        &mut Vec<Option<JsValue>>,
    )> {
        if let ObjectKind::FinalizationRegistry(ref mut data) = self.kind {
            let data = &mut **data;
            Some((&mut data.cells, &mut data.tokens))
        } else {
            None
        }
    }
}
