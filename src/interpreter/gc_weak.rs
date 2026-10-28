//! Weak slots of WeakRef and FinalizationRegistry (ADR-2026-10-28-0200).

use super::*;

impl Interpreter {
    /// A minor collection treats WeakRef targets and FinalizationRegistry
    /// targets and tokens as strong: only a major collection decides they are
    /// dead, so no weak slot ever names a freed (and recyclable) id.
    pub(super) fn trace_weak_slots_strongly(obj: &JsObjectData, worklist: &mut Vec<u64>) {
        use crate::interpreter::types::ObjectKind;
        match &obj.kind {
            ObjectKind::WeakRef(target) => Self::collect_value_roots(target, worklist),
            ObjectKind::FinalizationRegistry(data) => {
                for (target, _) in data.cells.iter().flatten() {
                    Self::collect_value_roots(target, worklist);
                }
                for token in data.tokens.iter().flatten() {
                    Self::collect_value_roots(token, worklist);
                }
            }
            _ => {}
        }
    }

    /// After a major mark: empty the WeakRef target, and the FinalizationRegistry
    /// targets and tokens, whose object was not marked. Returns whether a
    /// registry gained a cell with an empty target and has no cleanup job yet.
    pub(super) fn clear_dead_weak_slots(obj: &mut JsObjectData, marks: &[bool]) -> bool {
        use crate::interpreter::types::ObjectKind;
        let dead = |value: &JsValue| {
            value
                .as_object_id()
                .is_some_and(|id| !marks.get(id as usize).copied().unwrap_or(false))
        };
        match &mut obj.kind {
            ObjectKind::WeakRef(target) => {
                if dead(target) {
                    *target = JsValue::UNDEFINED;
                }
                false
            }
            ObjectKind::FinalizationRegistry(data) => {
                let mut emptied = false;
                for cell in data.cells.iter_mut().flatten() {
                    if dead(&cell.0) {
                        cell.0 = JsValue::UNDEFINED;
                    }
                    emptied |= cell.0.is_undefined();
                }
                for token in &mut data.tokens {
                    if token.as_ref().is_some_and(dead) {
                        *token = None;
                    }
                }
                let queue = emptied && !data.cleanup_queued;
                data.cleanup_queued |= queue;
                queue
            }
            _ => false,
        }
    }
}
