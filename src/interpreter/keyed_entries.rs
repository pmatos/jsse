//! [[MapData]] / [[SetData]] storage: the spec's insertion-ordered entry list
//! with a hash index over it, so lookup, insertion and deletion are
//! amortised O(1) (24.1, 24.2 require sublinear access).
//!
//! The list is append-only in spec terms. A deleted entry leaves an empty slot
//! so a suspended iterator's position keeps naming the same entry (24.1.3.3,
//! 24.2.4.5), and `clear` empties every entry while the spec list keeps its
//! length, so an iterator suspended before the clear goes on to visit entries
//! added after it (24.1.3.1, 24.2.4.2).
//!
//! Physical storage differs from that picture in two ways, neither observable:
//!
//! * Every slot carries its spec index (`seq`), a number that is never reused
//!   and increases along the list. Iterators hold such a spec-index cursor and
//!   step it with [`KeyedEntries::next_live`], which locates the first live
//!   slot at or after the cursor. Because positions are named by `seq` and not
//!   by physical offset, empty slots can be compacted away and `clear` can
//!   release storage without any live iterator needing a fix-up.
//! * Beyond a handful of entries a hash index maps a key (hashed under
//!   SameValueZero: numbers with ±0 and NaN canonicalised, strings by content,
//!   BigInts by value, objects and symbols by identity) to its slot. The index
//!   is open-addressed over slot positions, never owns keys, and is rebuilt
//!   from the slots when it grows or the slots are compacted.

use crate::interpreter::helpers::same_value_zero;
use crate::interpreter::types::{CARD_ELEMENTS, fill_card_bits, mark_card_bit};
use crate::types::{JsValue, ValueKind};
use std::hash::{BuildHasher, RandomState};
use std::sync::OnceLock;

/// An entry of a keyed collection: the value hashed and compared as the key.
pub(crate) trait KeyedEntry {
    fn key(&self) -> &JsValue;
}

impl KeyedEntry for JsValue {
    fn key(&self) -> &JsValue {
        self
    }
}

impl KeyedEntry for (JsValue, JsValue) {
    fn key(&self) -> &JsValue {
        &self.0
    }
}

/// Slot count above which the hash index is built; smaller collections are
/// scanned linearly, which is faster than hashing and costs no index.
const INDEX_THRESHOLD: usize = 8;
/// Fewest empty slots worth compacting away.
const COMPACT_MIN_EMPTY: usize = 16;
const MIN_TABLE_CAPACITY: usize = 32;

static KEY_SEED: OnceLock<u64> = OnceLock::new();
static TEXT_HASHER: OnceLock<RandomState> = OnceLock::new();

fn key_seed() -> u64 {
    *KEY_SEED.get_or_init(|| RandomState::new().hash_one(0x6B65_7965_645F_7365_u64))
}

fn text_hasher() -> &'static RandomState {
    TEXT_HASHER.get_or_init(RandomState::new)
}

fn fmix64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    x = x.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    x ^ (x >> 33)
}

/// Hash of `key` that agrees with [`same_value_zero`]: keys that compare equal
/// hash equal. The per-process seed is randomised so that key choice cannot be
/// used to force collisions.
fn key_hash(key: &JsValue) -> u32 {
    const NAN_BITS: u64 = 0x7FF8_0000_0000_0000;
    let raw = match key.kind() {
        ValueKind::Undefined => 0x0101,
        ValueKind::Null => 0x0202,
        ValueKind::Boolean => 0x0303 + u64::from(key.as_boolean() == Some(true)),
        ValueKind::Number => {
            let n = key.as_number().unwrap_or(0.0);
            if n == 0.0 {
                0
            } else if n.is_nan() {
                NAN_BITS
            } else {
                n.to_bits()
            }
        }
        ValueKind::String => key
            .with_string(|units| text_hasher().hash_one(units))
            .unwrap_or(0),
        ValueKind::BigInt => key
            .with_bigint(|value| text_hasher().hash_one(value))
            .unwrap_or(0),
        ValueKind::Symbol => key
            .with_symbol(|symbol| symbol.id())
            .unwrap_or(0)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0x0404),
        ValueKind::Object => key
            .as_object_id()
            .unwrap_or(0)
            .wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            .wrapping_add(0x0505),
    };
    let mixed = fmix64(raw ^ key_seed() ^ ((key.kind() as u64) << 56));
    (mixed ^ (mixed >> 32)) as u32
}

#[derive(Debug, Clone)]
struct Slot<T> {
    /// Spec index of this entry in the (conceptually append-only) list.
    seq: usize,
    value: Option<T>,
}

#[derive(Debug)]
pub(crate) struct KeyedEntries<T> {
    /// Ascending by `seq`; `None` values are deleted entries awaiting
    /// compaction.
    slots: Vec<Slot<T>>,
    /// `seq` the next appended entry receives; survives `clear` and
    /// compaction.
    next_seq: usize,
    live: usize,
    /// No slot before this offset is live.
    first_live: usize,
    /// Open-addressed hash index over `slots`, empty while the collection is
    /// small. A cell packs the key's 32-bit hash above the slot offset plus
    /// one; zero is an empty cell. Cells of deleted slots stay as probe-chain
    /// members until the next rebuild; they never match because the slot is
    /// empty. The cell count is a power of two, at least twice `slots.len()`.
    table: Vec<u64>,
    /// Dirty-card bitmap for the generational collector: bit `c` is set when
    /// slots `c * CARD_ELEMENTS ..` (one card) may hold a nursery reference.
    /// `None` means every slot may, the safe state: a new collection starts
    /// there, and every method that adds an edge without naming its slot
    /// ([`Self::upsert`]) or moves slots ([`Self::compact`], `clear`) returns
    /// to it, so no structural change can leave a stale bitmap behind. Only a
    /// minor collection that has just scanned every slot replaces it with a
    /// bitmap, and only [`Self::upsert_marking`] sets a bit.
    #[allow(clippy::box_collection)] // a pointer per collection, not the 24-byte Vec
    dirty_cards: Option<Box<Vec<u64>>>,
}

impl<T: Clone> Clone for KeyedEntries<T> {
    // A copy is a new object no collection has scanned: every card starts dirty.
    fn clone(&self) -> Self {
        Self {
            slots: self.slots.clone(),
            next_seq: self.next_seq,
            live: self.live,
            first_live: self.first_live,
            table: self.table.clone(),
            dirty_cards: None,
        }
    }
}

impl<T> Default for KeyedEntries<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            next_seq: 0,
            live: 0,
            first_live: 0,
            table: Vec::new(),
            dirty_cards: None,
        }
    }
}

fn cell(hash: u32, slot: usize) -> u64 {
    (u64::from(hash) << 32) | (slot as u64 + 1)
}

fn cell_slot(cell: u64) -> usize {
    (cell & 0xFFFF_FFFF) as usize - 1
}

fn place(table: &mut [u64], hash: u32, slot: usize) {
    let mask = table.len() - 1;
    let mut pos = hash as usize & mask;
    while table[pos] != 0 {
        pos = (pos + 1) & mask;
    }
    table[pos] = cell(hash, slot);
}

fn table_capacity_for(slots: usize) -> usize {
    (slots * 2).next_power_of_two().max(MIN_TABLE_CAPACITY)
}

impl<T> KeyedEntries<T> {
    pub(crate) fn live_len(&self) -> usize {
        self.live
    }

    /// The live entries in insertion order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.slots[self.first_live..]
            .iter()
            .filter_map(|slot| slot.value.as_ref())
    }

    /// The first live entry at or after spec index `*cursor`, moving `*cursor`
    /// just past it. Entries appended after the cursor was taken are visited;
    /// entries deleted since are skipped.
    pub(crate) fn next_live(&self, cursor: &mut usize) -> Option<&T> {
        let mut at = self.locate(*cursor).max(self.first_live);
        while let Some(slot) = self.slots.get(at) {
            if let Some(value) = slot.value.as_ref() {
                *cursor = slot.seq + 1;
                return Some(value);
            }
            at += 1;
        }
        None
    }

    /// Offset of the first slot whose `seq` is at least `seq`.
    fn locate(&self, seq: usize) -> usize {
        let Some(first) = self.slots.first() else {
            return 0;
        };
        // Spec indices are contiguous until a compaction opens a gap, which
        // makes the offset a direct computation in the common case.
        if let Some(guess) = seq.checked_sub(first.seq)
            && self.slots.get(guess).is_some_and(|slot| slot.seq == seq)
        {
            return guess;
        }
        self.slots.partition_point(|slot| slot.seq < seq)
    }

    /// Empty every entry (Map.prototype.clear, Set.prototype.clear). Storage
    /// is released; the spec list keeps its length, so cursors taken before
    /// the clear still name positions before any later entry.
    pub(crate) fn clear(&mut self) {
        self.slots = Vec::new();
        self.table = Vec::new();
        self.live = 0;
        self.first_live = 0;
        self.dirty_cards = None;
    }

    /// Delete every live entry for which `keep` is false.
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        for slot in &mut self.slots[self.first_live..] {
            if slot.value.as_ref().is_some_and(|value| !keep(value)) {
                slot.value = None;
                self.live -= 1;
            }
        }
        self.after_removal();
    }

    /// `clear` for a collection that just lost its last entry: a small slot
    /// buffer is kept so an add/delete cycle on a tiny set does not allocate
    /// every time.
    fn empty_in_place(&mut self) {
        if self.slots.capacity() > COMPACT_MIN_EMPTY {
            self.clear();
            return;
        }
        self.slots.clear();
        self.table = Vec::new();
        self.live = 0;
        self.first_live = 0;
        self.dirty_cards = None;
    }

    fn advance_first_live(&mut self) {
        while self
            .slots
            .get(self.first_live)
            .is_some_and(|slot| slot.value.is_none())
        {
            self.first_live += 1;
        }
    }

    fn after_removal(&mut self) {
        if self.live == 0 {
            self.empty_in_place();
            return;
        }
        self.advance_first_live();
        let empty = self.slots.len() - self.live;
        if empty >= COMPACT_MIN_EMPTY && empty > self.live {
            self.compact();
        }
    }

    /// Drop empty slots, keeping each entry's spec index.
    fn compact(&mut self) {
        // Slots move, so the bitmap no longer describes them.
        self.dirty_cards = None;
        let had_index = !self.table.is_empty();
        let mut moved_to = Vec::new();
        if had_index {
            let mut live_before = 0u32;
            moved_to = self
                .slots
                .iter()
                .map(|slot| {
                    if slot.value.is_none() {
                        return 0;
                    }
                    live_before += 1;
                    live_before
                })
                .collect();
        }
        self.slots.retain(|slot| slot.value.is_some());
        self.slots.shrink_to_fit();
        self.first_live = 0;
        if !had_index {
            return;
        }
        if self.slots.len() <= INDEX_THRESHOLD {
            self.table = Vec::new();
            return;
        }
        let mut table = vec![0u64; table_capacity_for(self.slots.len())];
        for &old in &self.table {
            if old != 0 && moved_to[cell_slot(old)] != 0 {
                place(
                    &mut table,
                    (old >> 32) as u32,
                    moved_to[cell_slot(old)] as usize - 1,
                );
            }
        }
        self.table = table;
    }

    /// Rebuild the index with room for `capacity` cells, keeping the cells of
    /// live slots.
    fn resize_table(&mut self, capacity: usize) {
        let mut table = vec![0u64; capacity];
        for &old in &self.table {
            if old != 0 && self.slots[cell_slot(old)].value.is_some() {
                place(&mut table, (old >> 32) as u32, cell_slot(old));
            }
        }
        self.table = table;
    }

    pub(crate) fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub(crate) fn dirty_card_bits(&self) -> Option<&[u64]> {
        self.dirty_cards.as_deref().map(Vec::as_slice)
    }

    /// The live entries held by slots `start..end`.
    pub(crate) fn live_in_slots(&self, start: usize, end: usize) -> impl Iterator<Item = &T> {
        self.slots[start..end]
            .iter()
            .filter_map(|slot| slot.value.as_ref())
    }

    /// Called after a scan of every slot: only `cards` still hold a young
    /// reference.
    pub(crate) fn set_dirty_cards(&mut self, cards: &[usize]) {
        let bits = self.dirty_cards.get_or_insert_with(Box::default);
        fill_card_bits(bits, self.slots.len(), cards);
    }

    pub(crate) fn mark_all_cards_dirty(&mut self) {
        self.dirty_cards = None;
    }

    /// After a major collection nothing is young, so a large collection's
    /// bitmap starts empty. Smaller ones are not tracked by card.
    pub(crate) fn reset_cards_after_major(&mut self) {
        self.dirty_cards = (self.slots.len() > CARD_ELEMENTS).then(Box::default);
    }

    fn mark_card(&mut self, slot: usize) {
        let Some(bits) = self.dirty_cards.as_deref_mut() else {
            return;
        };
        mark_card_bit(bits, slot);
    }
}

impl<T: KeyedEntry> KeyedEntries<T> {
    /// Offset of the live slot holding `key`, given its hash when an index is
    /// in use.
    fn find_slot(&self, key: &JsValue, hash: Option<u32>) -> Option<usize> {
        let matches = |slot: &Slot<T>| {
            slot.value
                .as_ref()
                .is_some_and(|entry| same_value_zero(entry.key(), key))
        };
        let Some(hash) = hash else {
            return self.slots.iter().position(matches);
        };
        let mask = self.table.len() - 1;
        let mut pos = hash as usize & mask;
        loop {
            let cell = self.table[pos];
            if cell == 0 {
                return None;
            }
            if (cell >> 32) as u32 == hash {
                let at = cell_slot(cell);
                if matches(&self.slots[at]) {
                    return Some(at);
                }
            }
            pos = (pos + 1) & mask;
        }
    }

    fn lookup(&self, key: &JsValue) -> Option<usize> {
        if self.table.is_empty() {
            self.find_slot(key, None)
        } else {
            self.find_slot(key, Some(key_hash(key)))
        }
    }

    pub(crate) fn get(&self, key: &JsValue) -> Option<&T> {
        self.slots[self.lookup(key)?].value.as_ref()
    }

    pub(crate) fn contains(&self, key: &JsValue) -> bool {
        self.lookup(key).is_some()
    }

    /// Append `entry` at the end of the list unless an entry with an equal
    /// key is present, in which case `update` receives that entry and the
    /// new one. Returns whether the entry was appended. The new references
    /// are not attributed to a slot, so every card becomes dirty; a caller
    /// that knows whether they are young uses [`Self::upsert_marking`].
    pub(crate) fn upsert(&mut self, entry: T, update: impl FnOnce(&mut T, T)) -> bool {
        self.dirty_cards = None;
        self.upsert_at(entry, update).0
    }

    /// [`Self::upsert`] that keeps the card bitmap: `young` says whether
    /// `entry` references a nursery object, and the caller has queued the
    /// owner for the next minor collection if so (see
    /// `Interpreter::gc_write_barrier_entry`). Only the slot written is
    /// marked.
    pub(crate) fn upsert_marking(
        &mut self,
        entry: T,
        update: impl FnOnce(&mut T, T),
        young: bool,
    ) -> bool {
        let (appended, slot) = self.upsert_at(entry, update);
        if young {
            self.mark_card(slot);
        }
        appended
    }

    /// Returns whether the entry was appended and the offset of the slot that
    /// now holds it.
    fn upsert_at(&mut self, entry: T, update: impl FnOnce(&mut T, T)) -> (bool, usize) {
        let hash = (!self.table.is_empty()).then(|| key_hash(entry.key()));
        if let Some(at) = self.find_slot(entry.key(), hash)
            && let Some(existing) = self.slots[at].value.as_mut()
        {
            update(existing, entry);
            return (false, at);
        }
        let at = self.slots.len();
        self.append(entry, hash);
        (true, at)
    }

    fn append(&mut self, entry: T, hash: Option<u32>) {
        let at = self.slots.len();
        match hash {
            Some(hash) => {
                if (at + 1) * 2 > self.table.len() {
                    self.resize_table(table_capacity_for(at + 1));
                }
                place(&mut self.table, hash, at);
                self.push_slot(entry);
            }
            None => {
                self.push_slot(entry);
                if self.slots.len() > INDEX_THRESHOLD {
                    self.build_index();
                }
            }
        }
    }

    fn push_slot(&mut self, entry: T) {
        self.slots.push(Slot {
            seq: self.next_seq,
            value: Some(entry),
        });
        self.next_seq += 1;
        self.live += 1;
    }

    fn build_index(&mut self) {
        let mut table = vec![0u64; table_capacity_for(self.slots.len())];
        for (at, slot) in self.slots.iter().enumerate() {
            if let Some(entry) = slot.value.as_ref() {
                place(&mut table, key_hash(entry.key()), at);
            }
        }
        self.table = table;
    }

    /// Delete the entry with `key`, returning whether there was one.
    pub(crate) fn remove(&mut self, key: &JsValue) -> bool {
        let Some(at) = self.lookup(key) else {
            return false;
        };
        self.slots[at].value = None;
        self.live -= 1;
        self.after_removal();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{JsBigInt, JsString};

    fn num(n: f64) -> JsValue {
        JsValue::number(n)
    }

    fn text(s: &str) -> JsValue {
        JsValue::string(JsString::from_str(s))
    }

    fn set_of(values: impl IntoIterator<Item = JsValue>) -> KeyedEntries<JsValue> {
        let mut entries = KeyedEntries::default();
        for value in values {
            entries.upsert(value, |_, _| {});
        }
        entries
    }

    fn visit(entries: &KeyedEntries<JsValue>, cursor: &mut usize) -> Option<f64> {
        entries.next_live(cursor).and_then(JsValue::as_number)
    }

    fn drain(entries: &KeyedEntries<JsValue>) -> Vec<f64> {
        let mut cursor = 0;
        std::iter::from_fn(|| visit(entries, &mut cursor)).collect()
    }

    #[test]
    fn lookup_matches_same_value_zero_in_small_and_indexed_modes() {
        for size in [3usize, 200] {
            let mut entries = set_of((0..size).map(|i| num(i as f64 + 1.0)));
            entries.upsert(num(0.0), |_, _| {});
            entries.upsert(num(f64::NAN), |_, _| {});
            entries.upsert(text("key"), |_, _| {});
            entries.upsert(JsValue::UNDEFINED, |_, _| {});
            entries.upsert(JsValue::NULL, |_, _| {});
            entries.upsert(JsValue::boolean(true), |_, _| {});

            assert!(entries.contains(&num(-0.0)));
            assert!(entries.contains(&num(0.0)));
            assert!(entries.contains(&num(f64::NAN)));
            assert!(entries.contains(&text("key")));
            assert!(!entries.contains(&text("ke")));
            assert!(entries.contains(&JsValue::UNDEFINED));
            assert!(entries.contains(&JsValue::NULL));
            assert!(entries.contains(&JsValue::boolean(true)));
            assert!(!entries.contains(&JsValue::boolean(false)));
            assert!(!entries.contains(&text("1")));
            assert_eq!(entries.live_len(), size + 6);
        }
    }

    #[test]
    fn upsert_updates_in_place_and_keeps_position() {
        let mut entries: KeyedEntries<(JsValue, JsValue)> = KeyedEntries::default();
        for i in 0..50 {
            let appended = entries.upsert((num(i as f64), num(0.0)), |old, new| old.1 = new.1);
            assert!(appended);
        }
        let appended = entries.upsert((num(7.0), num(99.0)), |old, new| old.1 = new.1);
        assert!(!appended);
        assert_eq!(entries.live_len(), 50);
        assert_eq!(
            entries.get(&num(7.0)).and_then(|e| e.1.as_number()),
            Some(99.0)
        );
        let order: Vec<f64> = entries.iter().filter_map(|e| e.0.as_number()).collect();
        assert_eq!(order, (0..50).map(f64::from).collect::<Vec<_>>());
    }

    #[test]
    fn bigints_and_strings_hash_by_value() {
        let big = |n: i64| JsValue::bigint(JsBigInt::new(n.into()));
        let mut entries = set_of((0..40).map(|i| big(i * 1_000_000_007)));
        entries.upsert(text("longer string key"), |_, _| {});
        assert!(entries.contains(&big(39 * 1_000_000_007)));
        assert!(!entries.contains(&big(-1)));
        assert!(!entries.contains(&num(0.0)));
        assert!(entries.contains(&text("longer string key")));
        assert!(!entries.contains(&text("longer string kex")));
    }

    #[test]
    fn deleted_entries_are_skipped_and_reinsertion_goes_last() {
        let mut entries = set_of((0..30).map(f64::from).map(num));
        assert!(entries.remove(&num(3.0)));
        assert!(!entries.remove(&num(3.0)));
        assert!(!entries.contains(&num(3.0)));
        assert!(entries.upsert(num(3.0), |_, _| {}));
        let order = drain(&entries);
        assert_eq!(order.len(), 30);
        assert_eq!(order.last(), Some(&3.0));
        assert_eq!(order[3], 4.0);
    }

    #[test]
    fn cursor_survives_compaction() {
        let mut entries = set_of((0..100).map(f64::from).map(num));
        let mut cursor = 0;
        for expected in 0..10 {
            assert_eq!(visit(&entries, &mut cursor), Some(f64::from(expected)));
        }
        // Delete enough to force compaction, including the entry the cursor
        // is about to visit.
        for i in 5..90 {
            entries.remove(&num(f64::from(i)));
        }
        entries.upsert(num(1000.0), |_, _| {});
        let mut rest = Vec::new();
        while let Some(v) = visit(&entries, &mut cursor) {
            rest.push(v);
        }
        let mut expected: Vec<f64> = (90..100).map(f64::from).collect();
        expected.push(1000.0);
        assert_eq!(rest, expected);
        assert_eq!(entries.live_len(), 5 + 10 + 1);
        assert!(entries.contains(&num(95.0)));
        assert!(!entries.contains(&num(50.0)));
    }

    #[test]
    fn cursor_before_clear_sees_entries_added_after_it() {
        let mut entries = set_of((0..20).map(f64::from).map(num));
        let mut cursor = 0;
        assert_eq!(visit(&entries, &mut cursor), Some(0.0));
        entries.clear();
        assert_eq!(entries.live_len(), 0);
        assert_eq!(visit(&entries, &mut cursor), None);
        entries.upsert(num(7.0), |_, _| {});
        assert_eq!(visit(&entries, &mut cursor), Some(7.0));
        assert_eq!(visit(&entries, &mut cursor), None);
    }

    #[test]
    fn retain_drops_entries_and_keeps_the_index_consistent() {
        let mut entries = set_of((0..64).map(f64::from).map(num));
        entries.retain(|v| v.as_number().is_some_and(|n| n % 4.0 == 0.0));
        assert_eq!(entries.live_len(), 16);
        for i in 0..64 {
            assert_eq!(entries.contains(&num(f64::from(i))), i % 4 == 0);
        }
        entries.retain(|_| false);
        assert_eq!(entries.live_len(), 0);
        assert!(drain(&entries).is_empty());
    }

    #[test]
    fn deleting_the_last_entry_keeps_spec_indices_moving_forward() {
        let mut entries = set_of([num(1.0)]);
        let mut cursor = 0;
        assert_eq!(visit(&entries, &mut cursor), Some(1.0));
        assert!(entries.remove(&num(1.0)));
        assert_eq!(entries.live_len(), 0);
        entries.upsert(num(2.0), |_, _| {});
        assert_eq!(visit(&entries, &mut cursor), Some(2.0));
        let mut fresh = 0;
        assert_eq!(visit(&entries, &mut fresh), Some(2.0));
        for round in 0..50 {
            assert!(entries.remove(&num(2.0 + f64::from(round))));
            entries.upsert(num(3.0 + f64::from(round)), |_, _| {});
        }
        assert_eq!(drain(&entries), vec![52.0]);
    }

    #[test]
    fn rolling_window_stays_bounded() {
        let mut entries = KeyedEntries::<JsValue>::default();
        for i in 0..5000u32 {
            entries.upsert(num(f64::from(i)), |_, _| {});
            if i >= 100 {
                assert!(entries.remove(&num(f64::from(i - 100))));
            }
            assert!(entries.slots.len() <= 2 * 101 + COMPACT_MIN_EMPTY + 1);
        }
        assert_eq!(entries.live_len(), 100);
        assert_eq!(drain(&entries).first(), Some(&4900.0));
    }

    fn cards(entries: &KeyedEntries<JsValue>) -> Option<Vec<usize>> {
        entries.dirty_card_bits().map(|bits| {
            (0..bits.len() * 64)
                .filter(|card| bits[card / 64] & (1 << (card % 64)) != 0)
                .collect()
        })
    }

    fn tracked(cards: usize) -> KeyedEntries<JsValue> {
        let mut entries = set_of((0..cards * CARD_ELEMENTS).map(|i| num(i as f64)));
        entries.reset_cards_after_major();
        assert_eq!(self::cards(&entries), Some(vec![]));
        entries
    }

    #[test]
    fn marking_upsert_dirties_only_the_slot_written() {
        let mut entries = tracked(4);
        entries.upsert_marking(num(f64::NAN), |_, _| {}, false);
        assert_eq!(cards(&entries), Some(vec![]));
        entries.upsert_marking(num(f64::NAN), |_, _| {}, true);
        assert_eq!(cards(&entries), Some(vec![4]));
        entries.upsert_marking(num(1.0), |_, _| {}, true);
        assert_eq!(cards(&entries), Some(vec![0, 4]));
    }

    #[test]
    fn plain_upsert_clear_compaction_and_clone_make_every_card_dirty() {
        let mut entries = tracked(4);
        entries.upsert(num(1.0), |_, _| {});
        assert_eq!(cards(&entries), None);

        let mut entries = tracked(4);
        entries.clear();
        assert_eq!(cards(&entries), None);

        let mut entries = tracked(8);
        entries.remove(&num(0.0));
        assert_eq!(cards(&entries), Some(vec![]));
        for i in 1..(6 * CARD_ELEMENTS) {
            entries.remove(&num(i as f64));
        }
        assert_eq!(cards(&entries), None);

        let entries = tracked(4);
        assert_eq!(cards(&entries.clone()), None);
    }

    #[test]
    fn small_collections_are_not_tracked_by_card() {
        let mut entries = set_of((0..CARD_ELEMENTS).map(|i| num(i as f64)));
        entries.reset_cards_after_major();
        assert_eq!(cards(&entries), None);
    }

    #[test]
    fn scan_helpers_see_live_entries_of_a_slot_range() {
        let mut entries = set_of((0..10).map(f64::from).map(num));
        entries.remove(&num(3.0));
        let seen: Vec<f64> = entries
            .live_in_slots(2, 6)
            .filter_map(JsValue::as_number)
            .collect();
        assert_eq!(seen, vec![2.0, 4.0, 5.0]);
        assert_eq!(entries.slot_count(), 10);
    }
}
