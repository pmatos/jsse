//! Chunked arena for `JsObjectData` slots.
//!
//! Replaces the prior `Vec<Option<Rc<RefCell<JsObjectData>>>>` slab with
//! `Vec<Box<[Option<Rc<RefCell<JsObjectData>>>; CHUNK_SIZE]>>`. Per-slot
//! `Rc<RefCell<…>>` storage is preserved (PR 2b.1) so existing call sites
//! keep their `Rc<RefCell<JsObjectData>>` API. The chunked layout makes
//! slot addresses stable across `Vec` growth, paving the way for a future
//! PR 2b.2 that drops the per-slot `Rc` and yields `&RefCell<JsObjectData>`
//! borrows directly.

use super::types::JsObjectData;
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) const CHUNK_SIZE: usize = 1024;

type Slot = Option<Rc<RefCell<JsObjectData>>>;
type Chunk = Box<[Slot; CHUNK_SIZE]>;

pub(crate) struct ObjectArena {
    chunks: Vec<Chunk>,
    free_list: Vec<u64>,
    next_slot: u64,
    live_count: usize,
}

impl ObjectArena {
    pub(crate) fn new() -> Self {
        Self {
            chunks: Vec::new(),
            free_list: Vec::new(),
            next_slot: 0,
            live_count: 0,
        }
    }

    /// Allocate a slot for `data`. Writes `data.id = Some(id)` before wrapping
    /// in `Rc<RefCell<>>`. Returns `(id, was_reuse)` so callers can adjust GC
    /// pressure accounting (a reused slot is cheaper than a fresh chunk
    /// growth).
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
        let rc = Rc::new(RefCell::new(data));
        self.set_slot(id, Some(rc));
        self.live_count += 1;
        (id, was_reuse)
    }

    /// Return a fresh `Rc::clone` of the slot's `Rc<RefCell<…>>` if live.
    /// Used by the legacy `Interpreter::get_object` API; new callers should
    /// use `get_cell` / `get_cell_expect` instead.
    pub(crate) fn get(&self, id: u64) -> Option<Rc<RefCell<JsObjectData>>> {
        self.slot_at(id).and_then(|s| s.clone())
    }

    /// Borrow the slot's `RefCell` if live, else `None`. Lifetime is
    /// tied to `&self`; callers must drop the borrow before any
    /// `&mut self` call.
    #[allow(dead_code)] // get_cell isn't yet hot; get_cell_expect is
    pub(crate) fn get_cell(&self, id: u64) -> Option<&RefCell<JsObjectData>> {
        self.slot_at(id)
            .and_then(|s| s.as_ref().map(|rc| rc.as_ref()))
    }

    /// Like `get_cell`, but panics for dead ids.
    pub(crate) fn get_cell_expect(&self, id: u64) -> &RefCell<JsObjectData> {
        self.get_cell(id).expect("dead object id")
    }

    /// Drop the slot at `id`. Caller is responsible for any external
    /// bookkeeping (e.g. ArrayBuffer external bytes) before calling.
    pub(crate) fn free(&mut self, id: u64) {
        debug_assert!(
            self.slot_at(id).is_some_and(|s| s.is_some()),
            "ObjectArena::free called on dead id {id}"
        );
        self.set_slot(id, None);
        self.free_list.push(id);
        self.live_count -= 1;
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

    /// Direct slot inspection — returns `&Slot` for the underlying
    /// `Option<Rc<…>>`. Used by sweep to test `is_some()` and to access
    /// the inner `Rc` without cloning.
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
}
