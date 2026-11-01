//! Chunked arena for `JsObjectData`.
//!
//! Logical object ids live in chunked slots, while their `JsObjectData`
//! allocations come from a second fixed-block arena. [`ObjectHandle`] keeps
//! the ownership and re-entrancy behaviour of the old
//! `Rc<RefCell<JsObjectData>>` representation without asking the system
//! allocator for every object. Logical ids may be reused as soon as GC clears
//! a slot; physical blocks are reused only after the last temporary handle to
//! the old object is dropped. Collector metadata lives beside each physical
//! allocation so the nursery keeps the same stable object ids and handles.

use super::types::JsObjectData;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::mem::MaybeUninit;
use std::ptr::{self, NonNull};
use std::rc::Rc;

pub(crate) const CHUNK_SIZE: usize = 1024;

type Slot = Option<ObjectHandle>;
type Chunk = Box<[Slot; CHUNK_SIZE]>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Generation {
    Young,
    Old,
    Dead,
}

/// Owned pointer to arena-allocated object data.
///
/// This intentionally exposes only the subset of `Rc` used for JS objects:
/// clone an owned handle and borrow the payload through the existing API.
/// Mutable borrows form the conservative old-to-young write barrier. The
/// pointer is stable because [`ObjectStorage`] never moves its chunks.
pub(crate) struct ObjectHandle {
    allocation: NonNull<ObjectAllocation>,
}

impl ObjectHandle {
    fn new(storage: &Rc<ObjectStorage>, id: u64, data: JsObjectData) -> Self {
        let allocation = storage.take_block();
        // SAFETY: `take_block` returns an aligned, currently uninitialized
        // block owned by `storage`. No handle can reference it until this
        // complete value has been written.
        unsafe {
            allocation.as_ptr().write(ObjectAllocation {
                strong: Cell::new(1),
                id,
                generation: Cell::new(Generation::Young),
                nursery_age: Cell::new(0),
                young_mark: Cell::new(false),
                remembered: Cell::new(0),
                storage: Rc::clone(storage),
                value: RefCell::new(data),
            });
        }
        Self { allocation }
    }

    #[inline(always)]
    fn allocation(&self) -> &ObjectAllocation {
        // SAFETY: a live handle contributes to `strong`, so the allocation is
        // initialized for the lifetime of the handle.
        unsafe { self.allocation.as_ref() }
    }

    #[inline(always)]
    pub(crate) fn borrow(&self) -> Ref<'_, JsObjectData> {
        self.allocation().value.borrow()
    }

    #[inline(always)]
    pub(crate) fn borrow_mut(&self) -> RefMut<'_, JsObjectData> {
        self.remember_if_old();
        self.allocation().value.borrow_mut()
    }

    #[inline(always)]
    pub(crate) fn borrow_untracked(&self) -> Ref<'_, JsObjectData> {
        self.allocation().value.borrow()
    }

    #[inline(always)]
    pub(crate) fn borrow_mut_untracked(&self) -> RefMut<'_, JsObjectData> {
        self.allocation().value.borrow_mut()
    }

    #[inline(always)]
    pub(crate) fn is_young(&self) -> bool {
        self.allocation().generation.get() == Generation::Young
    }

    #[inline(always)]
    pub(crate) fn is_old(&self) -> bool {
        self.allocation().generation.get() == Generation::Old
    }

    #[inline(always)]
    pub(crate) fn is_young_marked(&self) -> bool {
        self.allocation().young_mark.get()
    }

    /// Mark a nursery object, returning true only for its first admission to
    /// the minor collector's worklist.
    #[inline(always)]
    pub(crate) fn mark_young(&self) -> bool {
        self.is_young() && !self.allocation().young_mark.replace(true)
    }

    #[inline(always)]
    pub(crate) fn clear_young_mark(&self) {
        self.allocation().young_mark.set(false);
    }

    /// Age one nursery survivor. Returns true when it reaches promotion age.
    pub(crate) fn age_survivor(&self, promotion_age: u8) -> bool {
        debug_assert!(self.is_young());
        let age = self.allocation().nursery_age.get().saturating_add(1);
        self.allocation().nursery_age.set(age);
        age >= promotion_age
    }

    /// Promote a survivor. The collector decides whether it still needs to be
    /// remembered, since only it knows whether the survivor holds nursery
    /// edges.
    pub(crate) fn promote(&self) {
        self.allocation().generation.set(Generation::Old);
        self.allocation().nursery_age.set(0);
        self.allocation().young_mark.set(false);
        // A promoted Array has never been scanned as an old object, so it
        // has no card bitmap yet (a nursery Array never has one either).
        if let Ok(mut data) = self.allocation().value.try_borrow_mut() {
            data.mark_all_cards_dirty();
        }
    }

    /// Reset an Array's card bitmap after a major collection, which leaves no
    /// nursery object for it to describe. Skipped, harmlessly, while the object
    /// is borrowed: any bitmap an Array holds then is still a sound superset.
    pub(crate) fn reset_cards_after_major(&self) {
        if let Ok(mut data) = self.allocation().value.try_borrow_mut() {
            data.reset_cards_after_major();
        }
    }

    /// Tenure a full-collection survivor and clear its stale remembered bit.
    /// The collector re-adds owners of independently mutable environments.
    pub(crate) fn tenure_after_major(&self) {
        self.allocation().generation.set(Generation::Old);
        self.allocation().nursery_age.set(0);
        self.allocation().young_mark.set(false);
        self.allocation().remembered.set(0);
    }

    /// Mark element `index`'s card on an Array, returning false (the caller
    /// then falls back to the conservative barrier) if the object is borrowed.
    #[inline]
    pub(crate) fn mark_element_card(&self, index: usize) -> bool {
        match self.allocation().value.try_borrow_mut() {
            Ok(mut data) => {
                data.mark_element_card(index);
                true
            }
            Err(_) => false,
        }
    }

    /// Conservative old-to-young barrier: the non-element fields are
    /// re-scanned by the next minor collection. An Array's elements are
    /// covered only through its dirty cards (see `remember_all_if_old`).
    #[inline]
    pub(crate) fn remember_if_old(&self) {
        if self.is_old() {
            self.remember(REMEMBERED_FIELDS);
        }
    }

    /// Fully conservative barrier: the next minor collection re-scans every
    /// field and every element, discarding any card bitmap.
    #[inline]
    pub(crate) fn remember_all_if_old(&self) {
        if self.is_old() {
            self.remember(REMEMBERED_FIELDS | REMEMBERED_ALL_ELEMENTS);
        }
    }

    /// Queue an old Array whose dirty-card bitmap, and nothing else, records
    /// a new old-to-young edge.
    #[inline]
    pub(crate) fn remember_elements_if_old(&self) {
        if self.is_old() {
            self.remember(REMEMBERED_QUEUED);
        }
    }

    #[inline]
    fn remember(&self, bits: u8) {
        let allocation = self.allocation();
        let state = allocation.remembered.get();
        let wanted = state | bits | REMEMBERED_QUEUED;
        if wanted != state {
            allocation.remembered.set(wanted);
            if state & REMEMBERED_QUEUED == 0 {
                allocation
                    .storage
                    .remembered_ids
                    .borrow_mut()
                    .push(allocation.id);
            }
        }
    }

    /// Read and reset the remembered state: whether the object was queued and
    /// whether its non-element fields (rather than only dirty cards) must be
    /// re-scanned.
    fn take_remembered_state(&self) -> RememberedState {
        let bits = self.allocation().remembered.replace(0);
        RememberedState {
            queued: bits & REMEMBERED_QUEUED != 0,
            fields: bits & REMEMBERED_FIELDS != 0,
            all_elements: bits & REMEMBERED_ALL_ELEMENTS != 0,
        }
    }

    fn retire(&self) {
        self.allocation().generation.set(Generation::Dead);
        self.allocation().young_mark.set(false);
        self.allocation().remembered.set(0);
    }

    #[cold]
    #[inline(never)]
    unsafe fn drop_slow(allocation: NonNull<ObjectAllocation>) {
        // SAFETY: the caller established that this is the unique final handle.
        let allocation_ref = unsafe { allocation.as_ref() };
        let allocation_ptr = allocation.as_ptr();
        // Keep the storage alive while removing its reference from the block.
        let storage = Rc::clone(&allocation_ref.storage);
        // SAFETY: drop each non-trivial field exactly once, then return the
        // now-uninitialized block to its owner.
        unsafe {
            ptr::drop_in_place(ptr::addr_of_mut!((*allocation_ptr).value));
            ptr::drop_in_place(ptr::addr_of_mut!((*allocation_ptr).storage));
        }
        storage.return_block(allocation);
    }
}

impl Clone for ObjectHandle {
    #[inline(always)]
    fn clone(&self) -> Self {
        // SAFETY: every live handle contributes one to `strong`, which keeps
        // this initialized allocation and its storage alive.
        let allocation = self.allocation();
        let strong = allocation
            .strong
            .get()
            .checked_add(1)
            .expect("ObjectHandle strong count overflow");
        allocation.strong.set(strong);
        Self {
            allocation: self.allocation,
        }
    }
}

impl Drop for ObjectHandle {
    #[inline(always)]
    fn drop(&mut self) {
        // SAFETY: `self` owns one strong reference, so the allocation is live.
        let allocation = self.allocation();
        let strong = allocation.strong.get();
        debug_assert!(strong > 0, "ObjectHandle strong count underflow");
        if strong > 1 {
            allocation.strong.set(strong - 1);
            return;
        }

        // SAFETY: `strong == 1`, so this is the unique final handle.
        unsafe { Self::drop_slow(self.allocation) };
    }
}

/// The object is in the storage's remembered list.
const REMEMBERED_QUEUED: u8 = 1;
/// Its non-element fields may reference the nursery, so the whole object is
/// re-scanned. Without it, only an Array's dirty cards are.
const REMEMBERED_FIELDS: u8 = 2;
/// An Array's card bitmap could not be updated, so every element is re-scanned.
const REMEMBERED_ALL_ELEMENTS: u8 = 4;

/// What a minor collection must re-scan of one remembered object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RememberedState {
    pub(crate) queued: bool,
    pub(crate) fields: bool,
    pub(crate) all_elements: bool,
}

struct ObjectAllocation {
    strong: Cell<usize>,
    id: u64,
    generation: Cell<Generation>,
    nursery_age: Cell<u8>,
    young_mark: Cell<bool>,
    remembered: Cell<u8>,
    storage: Rc<ObjectStorage>,
    value: RefCell<JsObjectData>,
}

/// Physical backing store for [`ObjectAllocation`] blocks.
///
/// Chunks contain `MaybeUninit` so dropping the storage never double-drops
/// blocks already finalized by `ObjectHandle::drop`.
struct ObjectStorage {
    state: RefCell<ObjectStorageState>,
    recycle_blocks: Cell<bool>,
    remembered_ids: RefCell<Vec<u64>>,
}

struct ObjectStorageState {
    chunks: Vec<Box<[MaybeUninit<ObjectAllocation>]>>,
    free_list: Vec<NonNull<ObjectAllocation>>,
    next_block: usize,
}

impl ObjectStorage {
    fn new() -> Self {
        Self {
            state: RefCell::new(ObjectStorageState {
                chunks: Vec::new(),
                free_list: Vec::new(),
                next_block: 0,
            }),
            recycle_blocks: Cell::new(true),
            remembered_ids: RefCell::new(Vec::new()),
        }
    }

    fn take_block(&self) -> NonNull<ObjectAllocation> {
        let mut state = self.state.borrow_mut();
        if let Some(block) = state.free_list.pop() {
            return block;
        }
        if state.next_block == state.chunks.len() * CHUNK_SIZE {
            Self::grow_one_chunk(&mut state);
        }
        let block_idx = state.next_block;
        state.next_block += 1;
        let chunk_idx = block_idx / CHUNK_SIZE;
        let slot_idx = block_idx % CHUNK_SIZE;
        NonNull::from(&mut state.chunks[chunk_idx][slot_idx]).cast()
    }

    fn return_block(&self, block: NonNull<ObjectAllocation>) {
        if self.recycle_blocks.get() {
            self.state.borrow_mut().free_list.push(block);
        }
    }

    fn grow_one_chunk(state: &mut ObjectStorageState) {
        let chunk: Vec<MaybeUninit<ObjectAllocation>> = std::iter::repeat_with(MaybeUninit::uninit)
            .take(CHUNK_SIZE)
            .collect();
        state.chunks.push(chunk.into_boxed_slice());
    }
}

pub(crate) struct ObjectArena {
    chunks: Vec<Chunk>,
    free_list: Vec<u64>,
    next_slot: u64,
    live_count: usize,
    storage: Rc<ObjectStorage>,
    nursery: Vec<u64>,
}

impl ObjectArena {
    pub(crate) fn new() -> Self {
        Self {
            chunks: Vec::new(),
            free_list: Vec::new(),
            next_slot: 0,
            live_count: 0,
            storage: Rc::new(ObjectStorage::new()),
            nursery: Vec::new(),
        }
    }

    /// Allocate a slot for `data`. Writes `data.id = Some(id)` before placing
    /// it in the physical arena. Returns `(id, was_reuse)` so callers can
    /// distinguish logical slot reuse from fresh chunk growth.
    pub(crate) fn alloc(&mut self, mut data: JsObjectData) -> (u64, bool) {
        let (id, was_reuse) = if let Some(idx) = self.free_list.pop() {
            (idx, true)
        } else {
            if self.next_slot == self.capacity() {
                self.grow_one_chunk();
            }
            let idx = self.next_slot;
            self.next_slot += 1;
            (idx, false)
        };
        data.id = Some(id);
        let handle = ObjectHandle::new(&self.storage, id, data);
        self.set_slot(id, Some(handle));
        self.live_count += 1;
        self.nursery.push(id);
        (id, was_reuse)
    }

    /// Return an owned clone of the slot's handle if live. Used by the legacy
    /// `Interpreter::get_object` API; new callers should use `get_cell` /
    /// `get_cell_expect` instead.
    #[inline(always)]
    pub(crate) fn get(&self, id: u64) -> Option<ObjectHandle> {
        self.slot_at(id).and_then(|s| s.clone())
    }

    /// Borrow the slot's handle if live, else `None`. Lifetime is tied to
    /// `&self`; callers must drop payload borrows before any `&mut self` call.
    #[allow(dead_code)] // get_cell isn't yet hot; get_cell_expect is
    pub(crate) fn get_cell(&self, id: u64) -> Option<&ObjectHandle> {
        self.slot_at(id).and_then(Option::as_ref)
    }

    /// Like `get_cell`, but panics for dead ids.
    pub(crate) fn get_cell_expect(&self, id: u64) -> &ObjectHandle {
        self.get_cell(id).expect("dead object id")
    }

    /// Drop the slot at `id`. Caller is responsible for any external
    /// bookkeeping (e.g. ArrayBuffer external bytes) before calling.
    pub(crate) fn free(&mut self, id: u64) {
        debug_assert!(
            self.slot_at(id).is_some_and(|s| s.is_some()),
            "ObjectArena::free called on dead id {id}"
        );
        if let Some(handle) = self.get_cell(id) {
            handle.retire();
        }
        self.set_slot(id, None);
        self.free_list.push(id);
        self.live_count -= 1;
    }

    /// Take the current nursery so the collector can rebuild it from
    /// survivors without retaining dead or promoted ids.
    pub(crate) fn take_nursery(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.nursery)
    }

    pub(crate) fn replace_nursery(&mut self, nursery: Vec<u64>) {
        debug_assert!(
            nursery
                .iter()
                .all(|&id| self.get_cell(id).is_some_and(ObjectHandle::is_young))
        );
        self.nursery = nursery;
    }

    /// Drain the remembered set and reset its membership bits. Objects that
    /// still contain young edges are explicitly re-enqueued by the collector.
    /// An id appears once: a slot recycled while queued is reported only for
    /// the object that now owns it.
    pub(crate) fn take_remembered(&self) -> Vec<(u64, RememberedState)> {
        let ids = std::mem::take(&mut *self.storage.remembered_ids.borrow_mut());
        let mut taken = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(handle) = self.get_cell(id) {
                let state = handle.take_remembered_state();
                if state.queued {
                    taken.push((id, state));
                }
            }
        }
        taken
    }

    #[cfg(test)]
    pub(crate) fn take_remembered_ids(&self) -> Vec<u64> {
        self.take_remembered()
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    /// Ids of every object currently in the nursery.
    pub(crate) fn nursery_ids(&self) -> &[u64] {
        &self.nursery
    }

    pub(crate) fn remembered_len(&self) -> usize {
        self.storage.remembered_ids.borrow().len()
    }

    /// Clear nursery and remembered-set tracking after a major sweep. The
    /// collector rebuilds persistent remembered membership while tenuring
    /// survivors.
    pub(crate) fn reset_generations_after_major(&mut self) {
        self.nursery.clear();
        let remembered = std::mem::take(&mut *self.storage.remembered_ids.borrow_mut());
        for id in remembered {
            if let Some(handle) = self.get_cell(id) {
                handle.take_remembered_state();
            }
        }
    }

    /// Number of currently-occupied slots.
    pub(crate) fn live_count(&self) -> usize {
        self.live_count
    }

    /// Total slot capacity (live + dead). Used as the upper bound for
    /// sweep iteration and sizing the `marks` vector.
    pub(crate) fn capacity(&self) -> u64 {
        (self.chunks.len() * CHUNK_SIZE) as u64
    }

    /// Direct slot inspection. Used by sweep to test `is_some()` without
    /// cloning the underlying handle.
    pub(crate) fn slot_at(&self, id: u64) -> Option<&Slot> {
        let (chunk_idx, slot_idx) = Self::split(id);
        self.chunks.get(chunk_idx).map(|c| &c[slot_idx])
    }

    fn split(id: u64) -> (usize, usize) {
        let id = id as usize;
        (id / CHUNK_SIZE, id % CHUNK_SIZE)
    }

    fn set_slot(&mut self, id: u64, slot: Slot) {
        let (chunk_idx, slot_idx) = Self::split(id);
        self.chunks[chunk_idx][slot_idx] = slot;
    }

    fn grow_one_chunk(&mut self) {
        let chunk: Vec<Slot> = (0..CHUNK_SIZE).map(|_| None).collect();
        let boxed: Box<[Slot]> = chunk.into_boxed_slice();
        let array: Chunk = boxed
            .try_into()
            .unwrap_or_else(|_| panic!("ObjectArena: chunk size {CHUNK_SIZE} mismatch"));
        self.chunks.push(array);
    }

    #[cfg(test)]
    fn storage_stats(&self) -> (usize, usize, usize) {
        let storage = self.storage.state.borrow();
        (
            storage.chunks.len(),
            storage.next_block,
            storage.free_list.len(),
        )
    }
}

impl Drop for ObjectArena {
    fn drop(&mut self) {
        // No future allocation can reuse blocks after the arena starts
        // dropping. Avoid filling a free list while its live slots are torn
        // down; escaped handles still keep the backing chunks alive until they
        // are released.
        self.storage.recycle_blocks.set(false);
    }
}

impl Default for ObjectArena {
    fn default() -> Self {
        Self::new()
    }
}

mod prefetch;
#[cfg(test)]
mod tests;
