//! Cache prefetch of nursery objects ahead of the minor collector's sweep.
//!
//! A dead nursery object is touched for the first time by the sweep (the trace
//! only visits survivors), so freeing it starts with a cache miss on its
//! allocation and another on each heap block it owns. The sweep walks the
//! nursery in allocation order, which is not address order of the blocks, so
//! the hardware prefetcher cannot predict it. The sweep therefore prefetches
//! in two stages: the allocation of an object far ahead, then, once that has
//! arrived, the blocks the object points to.

use super::ObjectArena;
use crate::interpreter::types::{IteratorState, JsFunction, JsObjectData, ObjectKind};

/// Bytes of an allocation worth prefetching: its header and the whole payload
/// (`JsObjectData` is capped at 216 bytes).
const ALLOCATION_BYTES: usize = 320;

/// Bytes of a function's `JsFunction` worth prefetching.
const FUNCTION_BYTES: usize = 128;

#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn prefetch(address: *const u8) {
    use std::arch::x86_64::{_MM_HINT_T0, _mm_prefetch};
    // SAFETY: a prefetch is a hint that cannot fault or change memory,
    // whatever the address.
    unsafe { _mm_prefetch::<_MM_HINT_T0>(address.cast::<i8>()) };
}

#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
fn prefetch(_address: *const u8) {}

fn prefetch_lines(base: *const u8, bytes: usize) {
    for offset in (0..bytes).step_by(64) {
        prefetch(base.wrapping_add(offset));
    }
}

/// The heap blocks `obj` owns that dropping it must touch and that are most
/// often dead nursery weight: a function's record and closure environment, an
/// iterator's or an Arguments object's state box, an Array's elements.
fn prefetch_owned(obj: &JsObjectData) {
    if let Some(function) = &obj.callable {
        prefetch_lines(
            std::ptr::from_ref::<JsFunction>(function).cast(),
            FUNCTION_BYTES,
        );
        if let JsFunction::User { closure, .. } = &**function {
            prefetch(std::rc::Rc::as_ptr(closure).cast());
        }
    }
    match &obj.kind {
        ObjectKind::Iterator(state) => prefetch(std::ptr::from_ref::<IteratorState>(state).cast()),
        ObjectKind::Arguments(map) => prefetch(std::ptr::from_ref(&**map).cast()),
        ObjectKind::Array(_) => {
            if let Some(elements) = obj.array_elements() {
                prefetch(elements.as_ptr().cast());
            }
        }
        _ => {}
    }
}

impl ObjectArena {
    /// Hint that the object `id` is about to be read and dropped. No effect
    /// on a dead id.
    #[inline(always)]
    pub(crate) fn prefetch_object(&self, id: u64) {
        if let Some(Some(handle)) = self.slot_at(id) {
            prefetch_lines(handle.allocation.as_ptr().cast(), ALLOCATION_BYTES);
        }
    }

    /// Hint that the heap blocks of the object `id` are about to be dropped
    /// with it. Reads the object, so call it once `prefetch_object` has had
    /// time to land.
    #[inline(always)]
    pub(crate) fn prefetch_owned(&self, id: u64) {
        if let Some(Some(handle)) = self.slot_at(id) {
            // SAFETY: the reference does not outlive this call, in which
            // nothing borrows the payload mutably (the collector runs with no
            // object borrowed).
            if let Ok(obj) = unsafe { handle.allocation().value.try_borrow_unguarded() } {
                prefetch_owned(obj);
            }
        }
    }
}
