//! Key -> slot index of a spilled [`PropertyMap`].
//!
//! A map with 9 to `TAG_MAX` entries keeps one tag byte per entry slot instead
//! of a hash table: a lookup hashes the key to a tag, compares eight tags per
//! word and verifies only the matching slots against the entry list. That is
//! one byte per entry where a `HashMap<JsPropertyKey, u32>` costs 25 or more
//! (bucket, control byte and load-factor slack), which is most of what an AST
//! node or type object with a dozen fields spent on its index. A larger map
//! moves to the hash table for good.

use super::*;

/// Most entry slots (live or deleted) a tag index covers; a scan is at most
/// eight word compares.
pub(super) const TAG_MAX: usize = 64;

/// Entry-list capacity of a map that has just spilled: nine entries, rounded
/// up to the allocator's next size class.
pub(super) const SPILL_CAPACITY: usize = 10;

/// `n` entries rounded up to a count whose byte size is a size class of
/// jemalloc (64 bytes apart up to 512, then 128, then 256), so an
/// exact-capacity list wastes nothing on top of its own slack.
pub(super) fn round_capacity(n: usize) -> usize {
    match n {
        0..=8 => n,
        9..=16 => n.next_multiple_of(2),
        17..=32 => n.next_multiple_of(4),
        _ => n.next_multiple_of(8),
    }
}

/// The entry-list capacity after `cap` is exhausted: a quarter more, where
/// doubling left a third of the list empty on a node with thirteen fields.
pub(super) fn grown_capacity(cap: usize) -> usize {
    round_capacity(cap + cap / 4 + 1)
}

const LOW_BITS: u64 = 0x0101_0101_0101_0101;
const LOW_SEVEN: u64 = 0x7F7F_7F7F_7F7F_7F7F;
const MIX: u64 = 0x9E37_79B9_7F4A_7C15;

pub(super) enum SlotIndex {
    /// Byte `slot % 8` of word `slot / 8` is `tag_of` the key `entries[slot]`
    /// had when it was pushed. The bytes of deleted slots, of slots past the
    /// end of `entries` and of padding are never trusted: a candidate is
    /// always verified against the entry itself.
    Tags(Vec<u64>),
    Hashed(Box<HashMap<JsPropertyKey, u32, IndexState>>),
}

/// Not a defence against crafted keys: a collision only costs a key
/// comparison, and at most `TAG_MAX` of them.
#[inline]
pub(super) fn tag_of(key: &[u8]) -> u8 {
    let n = key.len();
    let bits = if n > 16 {
        // Keys such as paths or qualified names often differ only in the middle.
        let mut h = u64::from_le_bytes(key[n - 8..].try_into().unwrap());
        for chunk in key.as_chunks::<8>().0 {
            h = (h ^ u64::from_le_bytes(*chunk))
                .wrapping_mul(MIX)
                .rotate_left(23);
        }
        h
    } else if n >= 8 {
        let lo = u64::from_le_bytes(key[..8].try_into().unwrap());
        let hi = u64::from_le_bytes(key[n - 8..].try_into().unwrap());
        lo ^ hi.rotate_left(29)
    } else if n >= 4 {
        let lo = u32::from_le_bytes(key[..4].try_into().unwrap());
        let hi = u32::from_le_bytes(key[n - 4..].try_into().unwrap());
        u64::from(lo) | u64::from(hi) << 32
    } else if n > 0 {
        u64::from(key[0]) | u64::from(key[n / 2]) << 8 | u64::from(key[n - 1]) << 16
    } else {
        0
    };
    ((bits ^ n as u64).wrapping_mul(MIX) >> 56) as u8
}

#[inline]
fn holds(entries: &[Option<Entry>], slot: usize, key: &[u8]) -> bool {
    matches!(entries.get(slot), Some(Some((k, _))) if key_bytes_eq(k.as_bytes(), key))
}

#[inline]
pub(super) fn set_tag(words: &mut Vec<u64>, slot: usize, tag: u8) {
    if slot.is_multiple_of(8) {
        words.push(0);
    }
    let shift = (slot % 8) * 8;
    let word = &mut words[slot / 8];
    *word = *word & !(0xFF << shift) | u64::from(tag) << shift;
}

/// The first slot whose tag byte matches `key`'s and that `holds` accepts.
#[inline]
pub(super) fn scan_tags(
    words: &[u64],
    key: &[u8],
    mut holds: impl FnMut(usize) -> bool,
) -> Option<usize> {
    let splat = LOW_BITS.wrapping_mul(u64::from(tag_of(key)));
    for (w, &word) in words.iter().enumerate() {
        let x = word ^ splat;
        // Bit 7 of a byte is set exactly when that byte of `x` is zero.
        let mut zero = !(((x & LOW_SEVEN) + LOW_SEVEN) | x | LOW_SEVEN);
        while zero != 0 {
            let slot = w * 8 + (zero.trailing_zeros() / 8) as usize;
            if holds(slot) {
                return Some(slot);
            }
            zero &= zero - 1;
        }
    }
    None
}

impl SlotIndex {
    /// An index for `entries`, with room for `capacity` of them.
    pub(super) fn build(entries: &[Option<Entry>], capacity: usize) -> Self {
        let capacity = capacity.max(entries.len());
        if capacity <= TAG_MAX {
            let mut words = Vec::with_capacity(capacity.div_ceil(8));
            for (slot, entry) in entries.iter().enumerate() {
                set_tag(
                    &mut words,
                    slot,
                    entry.as_ref().map_or(0, |(k, _)| tag_of(k.as_bytes())),
                );
            }
            Self::Tags(words)
        } else {
            let mut map = HashMap::with_capacity_and_hasher(capacity, IndexState::default());
            for (slot, entry) in entries.iter().enumerate() {
                if let Some((k, _)) = entry {
                    map.insert(k.clone(), slot as u32);
                }
            }
            Self::Hashed(Box::new(map))
        }
    }

    #[inline]
    pub(super) fn find(&self, entries: &[Option<Entry>], key: &[u8]) -> Option<usize> {
        match self {
            Self::Hashed(map) => map.get(key).map(|&slot| slot as usize),
            Self::Tags(words) => scan_tags(words, key, |slot| holds(entries, slot, key)),
        }
    }

    /// Append `key` (absent from the map) to `entries` and index it.
    pub(super) fn push(
        &mut self,
        entries: &mut Vec<Option<Entry>>,
        key: JsPropertyKey,
        value: PropertyRecord,
    ) {
        let slot = entries.len();
        if slot == entries.capacity() && slot < TAG_MAX && matches!(self, Self::Tags(_)) {
            entries.reserve_exact(grown_capacity(slot) - slot);
        }
        match self {
            Self::Tags(words) if slot < TAG_MAX => {
                set_tag(words, slot, tag_of(key.as_bytes()));
                entries.push(Some((key, value)));
            }
            Self::Tags(_) if entries.iter().any(Option::is_none) => {
                // Deleted slots alone must not push a small map onto the hash table.
                entries.retain(Option::is_some);
                *self = Self::build(entries, entries.len() + 1);
                self.push(entries, key, value);
            }
            Self::Tags(_) => {
                entries.push(Some((key, value)));
                *self = Self::build(entries, slot + 1);
            }
            Self::Hashed(map) => {
                map.insert(key.clone(), slot as u32);
                entries.push(Some((key, value)));
            }
        }
    }

    /// Slot of `key`, which is being deleted: the index stops naming it.
    #[inline]
    pub(super) fn remove(&mut self, entries: &[Option<Entry>], key: &[u8]) -> Option<usize> {
        match self {
            Self::Hashed(map) => map.remove(key).map(|slot| slot as usize),
            Self::Tags(_) => self.find(entries, key),
        }
    }

    /// `entries` was shortened to `len` slots.
    #[inline]
    pub(super) fn truncated(&mut self, len: usize) {
        if let Self::Tags(words) = self {
            words.truncate(len.div_ceil(8));
        }
    }

    /// Room for `additional` more entries in `entries`: exactly that many
    /// while the map is small, amortised once it hashes.
    pub(super) fn reserve_entries(&self, entries: &mut Vec<Option<Entry>>, additional: usize) {
        match self {
            Self::Tags(_) => entries.reserve_exact(additional),
            Self::Hashed(_) => entries.reserve(additional),
        }
    }

    /// Room for `additional` more slots in `entries`; a bulk copy that will
    /// outgrow the tag index starts on a right-sized hash table instead.
    pub(super) fn reserve(&mut self, entries: &[Option<Entry>], additional: usize) {
        match self {
            Self::Tags(_) if entries.len() + additional > TAG_MAX => {
                *self = Self::build(entries, entries.len() + additional);
            }
            Self::Tags(words) => words.reserve(additional.div_ceil(8)),
            Self::Hashed(map) => map.reserve(additional),
        }
    }

    /// Drop the deleted slots of `entries` and renumber what the index names.
    pub(super) fn compact(&mut self, entries: &mut Vec<Option<Entry>>) {
        match self {
            Self::Tags(words) => {
                entries.retain(Option::is_some);
                words.clear();
                for (slot, (k, _)) in entries.iter().flatten().enumerate() {
                    set_tag(words, slot, tag_of(k.as_bytes()));
                }
            }
            Self::Hashed(map) => {
                let mut remap = Vec::with_capacity(entries.len());
                let mut next = 0u32;
                for entry in entries.iter() {
                    remap.push(next);
                    next += u32::from(entry.is_some());
                }
                entries.retain(Option::is_some);
                for slot in map.values_mut() {
                    *slot = remap[*slot as usize];
                }
            }
        }
    }
}
