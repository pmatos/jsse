//! The part of `BindingMap` that talks to the collector's environment
//! registry (#1490): the write barrier's slow path and the registry slot.

use super::{BindingMap, NO_SLOT};
use crate::interpreter::gc::env_registry;

impl Drop for BindingMap {
    #[inline]
    fn drop(&mut self) {
        let slot = self.slot.get();
        if slot != NO_SLOT {
            env_registry::release(slot);
        }
    }
}

impl BindingMap {
    /// Every write turns the clean mark off; the first one after a collection
    /// found the environment clean also queues it for the next minor one.
    #[inline(always)]
    pub(super) fn dirty(&self) {
        if self.clean.get() {
            self.clean.set(false);
            env_registry::note_dirty(self.slot.get());
        }
    }

    #[inline]
    pub(crate) fn slot(&self) -> u32 {
        self.slot.get()
    }

    pub(crate) fn set_slot(&self, slot: u32) {
        self.slot.set(slot);
    }

    /// The collector's: the environment still holds a nursery object, so the
    /// next minor collection must scan it again.
    pub(crate) fn rearm(&self) {
        self.clean.set(false);
        env_registry::note_dirty(self.slot.get());
    }
}
