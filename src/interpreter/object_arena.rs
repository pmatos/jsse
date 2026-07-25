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
                remembered: Cell::new(false),
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

    /// Promote a survivor and conservatively scan it at the next minor GC.
    pub(crate) fn promote(&self) {
        self.allocation().generation.set(Generation::Old);
        self.allocation().nursery_age.set(0);
        self.allocation().young_mark.set(false);
        self.remember_if_old();
    }

    /// Tenure a full-collection survivor. A major collection leaves no young
    /// objects, so it does not need to enter the remembered set.
    pub(crate) fn tenure_after_major(&self) {
        self.allocation().generation.set(Generation::Old);
        self.allocation().nursery_age.set(0);
        self.allocation().young_mark.set(false);
        self.allocation().remembered.set(false);
    }

    pub(crate) fn remember_if_old(&self) {
        let allocation = self.allocation();
        if self.is_old() && !allocation.remembered.replace(true) {
            allocation
                .storage
                .remembered_ids
                .borrow_mut()
                .push(allocation.id);
        }
    }

    fn clear_remembered(&self) {
        self.allocation().remembered.set(false);
    }

    fn retire(&self) {
        self.allocation().generation.set(Generation::Dead);
        self.allocation().young_mark.set(false);
        self.allocation().remembered.set(false);
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

struct ObjectAllocation {
    strong: Cell<usize>,
    id: u64,
    generation: Cell<Generation>,
    nursery_age: Cell<u8>,
    young_mark: Cell<bool>,
    remembered: Cell<bool>,
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
    pub(crate) fn take_remembered(&self) -> Vec<u64> {
        let ids = std::mem::take(&mut *self.storage.remembered_ids.borrow_mut());
        for &id in &ids {
            if let Some(handle) = self.get_cell(id) {
                handle.clear_remembered();
            }
        }
        ids
    }

    pub(crate) fn remembered_len(&self) -> usize {
        self.storage.remembered_ids.borrow().len()
    }

    /// A major collection tenures every survivor and starts with empty
    /// nursery/remembered sets.
    pub(crate) fn reset_generations_after_major(&mut self) {
        self.nursery.clear();
        let remembered = std::mem::take(&mut *self.storage.remembered_ids.borrow_mut());
        for id in remembered {
            if let Some(handle) = self.get_cell(id) {
                handle.clear_remembered();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_allocations_fill_current_chunk_before_growing() {
        let mut arena = ObjectArena::new();

        for expected in 0..CHUNK_SIZE as u64 {
            let (id, was_reuse) = arena.alloc(JsObjectData::new());
            assert_eq!(id, expected);
            assert!(!was_reuse);
            assert_eq!(arena.capacity(), CHUNK_SIZE as u64);
        }

        let (id, was_reuse) = arena.alloc(JsObjectData::new());
        assert_eq!(id, CHUNK_SIZE as u64);
        assert!(!was_reuse);
        assert_eq!(arena.capacity(), (CHUNK_SIZE * 2) as u64);
    }

    #[test]
    fn freed_slots_are_reused_before_next_fresh_slot() {
        let mut arena = ObjectArena::new();
        let (first, _) = arena.alloc(JsObjectData::new());
        let (second, _) = arena.alloc(JsObjectData::new());

        arena.free(first);

        let (reused, was_reuse) = arena.alloc(JsObjectData::new());
        assert_eq!(reused, first);
        assert!(was_reuse);
        assert_eq!(arena.live_count(), 2);

        let (fresh, was_reuse) = arena.alloc(JsObjectData::new());
        assert_eq!(fresh, second + 1);
        assert!(!was_reuse);
    }

    #[test]
    fn mutable_borrow_remembers_old_object_once() {
        let mut arena = ObjectArena::new();
        let (id, _) = arena.alloc(JsObjectData::new());
        let handle = arena.get_cell_expect(id);
        handle.promote();

        // Promotion itself schedules one conservative scan.
        assert_eq!(arena.take_remembered(), vec![id]);
        handle.borrow_mut().extensible = false;
        handle.borrow_mut().extensible = true;
        assert_eq!(arena.take_remembered(), vec![id]);
        assert!(arena.take_remembered().is_empty());
    }

    #[test]
    fn immutable_borrow_does_not_trigger_write_barrier() {
        let mut arena = ObjectArena::new();
        let (id, _) = arena.alloc(JsObjectData::new());
        let handle = arena.get_cell_expect(id);
        handle.promote();
        arena.take_remembered();

        assert!(handle.borrow().extensible);
        assert!(arena.take_remembered().is_empty());
    }

    #[test]
    fn object_payloads_are_allocated_in_chunks() {
        let mut arena = ObjectArena::new();

        for _ in 0..CHUNK_SIZE {
            arena.alloc(JsObjectData::new());
        }
        assert_eq!(arena.storage_stats(), (1, CHUNK_SIZE, 0));

        arena.alloc(JsObjectData::new());
        assert_eq!(arena.storage_stats(), (2, CHUNK_SIZE + 1, 0));
        assert_eq!(
            std::mem::size_of::<ObjectHandle>(),
            std::mem::size_of::<usize>()
        );
    }

    #[test]
    fn released_payload_blocks_are_reused() {
        let mut arena = ObjectArena::new();
        let (id, _) = arena.alloc(JsObjectData::new());
        assert_eq!(arena.storage_stats(), (1, 1, 0));

        arena.free(id);
        assert_eq!(arena.storage_stats(), (1, 1, 1));

        arena.alloc(JsObjectData::new());
        assert_eq!(arena.storage_stats(), (1, 1, 0));
    }

    #[test]
    fn cloned_handle_delays_physical_reuse_after_logical_free() {
        let mut arena = ObjectArena::new();
        let (id, _) = arena.alloc(JsObjectData::new());
        let pinned = arena.get(id).unwrap();

        arena.free(id);
        assert_eq!(arena.storage_stats(), (1, 1, 0));

        let (reused_id, was_reuse) = arena.alloc(JsObjectData::new());
        assert_eq!(reused_id, id);
        assert!(was_reuse);
        assert_eq!(arena.storage_stats(), (1, 2, 0));

        drop(pinned);
        assert_eq!(arena.storage_stats(), (1, 2, 1));
    }

    #[test]
    fn cloned_handle_keeps_storage_alive_after_arena_drop() {
        let (id, pinned, storage) = {
            let mut arena = ObjectArena::new();
            let (id, _) = arena.alloc(JsObjectData::new());
            (id, arena.get(id).unwrap(), Rc::clone(&arena.storage))
        };

        assert!(!storage.recycle_blocks.get());
        assert_eq!(pinned.borrow().id, Some(id));
        drop(pinned);
        assert!(storage.state.borrow().free_list.is_empty());
    }
}
