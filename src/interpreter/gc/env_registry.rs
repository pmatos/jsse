//! The registry of environments a collection found clean, and the dirty list
//! that lets a minor collection visit only the ones written since (#1490).
//!
//! A registered environment is either clean (`BindingMap::is_clean`: no
//! nursery object in it, nothing written since) or has its slot on the dirty
//! list. The list is filled when the clean mark is turned off (`BindingMap`'s
//! write barrier pushes only on that flip) and drained by the next minor
//! collection, so that collection costs the number of environments written, not
//! the number registered.
//!
//! The slab is per thread because the barrier sits in `BindingMap`, which
//! knows neither its environment nor the interpreter. Entries carry the owning
//! interpreter so that two interpreters on one thread never scan each other's
//! environments.

use super::super::types::{EnvRef, Environment};
use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub(crate) const NO_SLOT: u32 = u32::MAX;

struct Entry {
    owner: u32,
    env: Weak<RefCell<Environment>>,
}

#[derive(Default)]
struct Slab {
    entries: Vec<Option<Entry>>,
    free: Vec<u32>,
    dirty: Vec<u32>,
    /// Registered environments per owner, indexed by owner.
    live: Vec<usize>,
    owners: u32,
}

impl Slab {
    /// Frees `slot` if its environment is dead.
    fn free_if_dead(&mut self, slot: usize) {
        if self.entries[slot]
            .as_ref()
            .is_some_and(|entry| entry.env.strong_count() == 0)
        {
            let owner = self.entries[slot].take().map_or(0, |entry| entry.owner);
            self.free.push(slot as u32);
            self.live[owner as usize] -= 1;
        }
    }
}

thread_local! {
    static SLAB: RefCell<Slab> = RefCell::new(Slab::default());
}

/// An interpreter's handle on the thread's registry.
pub(crate) struct EnvRegistry {
    owner: u32,
}

impl EnvRegistry {
    pub(crate) fn new() -> Self {
        let owner = SLAB.with_borrow_mut(|slab| {
            slab.owners += 1;
            slab.live.resize(slab.owners as usize + 1, 0);
            slab.owners
        });
        EnvRegistry { owner }
    }

    /// Environments registered by this interpreter.
    #[cfg(any(test, feature = "perf-counters"))]
    pub(crate) fn len(&self) -> usize {
        SLAB.with_borrow(|slab| slab.live[self.owner as usize])
    }

    /// Registers `env` unless it already is.
    pub(crate) fn register(&self, env: &EnvRef) {
        if env.borrow().bindings.slot() != NO_SLOT {
            return;
        }
        let weak = Rc::downgrade(env);
        let slot = SLAB.with_borrow_mut(|slab| {
            slab.live[self.owner as usize] += 1;
            let entry = Some(Entry {
                owner: self.owner,
                env: weak,
            });
            if let Some(slot) = slab.free.pop() {
                slab.entries[slot as usize] = entry;
                slot
            } else {
                slab.entries.push(entry);
                (slab.entries.len() - 1) as u32
            }
        });
        env.borrow().bindings.set_slot(slot);
    }

    /// Takes the environments written since the last drain.
    pub(crate) fn take_dirty(&self) -> Vec<EnvRef> {
        SLAB.with_borrow_mut(|slab| {
            let mut out = Vec::new();
            let mut foreign = Vec::new();
            for slot in std::mem::take(&mut slab.dirty) {
                match &slab.entries[slot as usize] {
                    Some(entry) if entry.owner == self.owner => {
                        out.extend(entry.env.upgrade());
                    }
                    Some(_) => foreign.push(slot),
                    None => {}
                }
            }
            slab.dirty = foreign;
            out
        })
    }

    /// A major collection's start: frees the slots of environments that died
    /// without releasing theirs.
    pub(crate) fn begin_major(&self) {
        SLAB.with_borrow_mut(|slab| {
            for slot in 0..slab.entries.len() {
                slab.free_if_dead(slot);
            }
        });
    }

    /// A major collection's end: it settled every environment it reached, so
    /// this interpreter's queue keeps only the entries of environments that are
    /// alive and still not clean (one the walk missed), which bounds the queue
    /// when minor collections are suppressed.
    pub(crate) fn end_major(&self) {
        SLAB.with_borrow_mut(|slab| {
            let Slab { entries, dirty, .. } = &mut *slab;
            dirty.retain(|&slot| {
                entries[slot as usize].as_ref().is_some_and(|entry| {
                    entry.owner != self.owner
                        || entry
                            .env
                            .upgrade()
                            .is_some_and(|env| env.try_borrow().is_ok_and(|env| !env.is_clean()))
                })
            });
        });
    }

    #[cfg(test)]
    pub(crate) fn registered(&self) -> Vec<EnvRef> {
        SLAB.with_borrow(|slab| {
            slab.entries
                .iter()
                .flatten()
                .filter(|entry| entry.owner == self.owner)
                .filter_map(|entry| entry.env.upgrade())
                .collect()
        })
    }
}

/// The write barrier's slow path: the environment at `slot` was clean and is
/// not any more.
#[cold]
#[inline(never)]
pub(crate) fn note_dirty(slot: u32) {
    debug_assert_ne!(slot, NO_SLOT, "a clean environment is registered");
    if slot == NO_SLOT {
        return;
    }
    let _ = SLAB.try_with(|slab| {
        // A lost push would hide a written environment from every later minor
        // collection; no slab code runs a write barrier, so this cannot happen.
        slab.try_borrow_mut()
            .expect("the slab is not borrowed across a write barrier")
            .dirty
            .push(slot);
    });
}

/// An environment's last act: its slot is free for another.
pub(crate) fn release(slot: u32) {
    let _ = SLAB.try_with(|slab| {
        let Ok(mut slab) = slab.try_borrow_mut() else {
            return;
        };
        if (slot as usize) < slab.entries.len() {
            slab.free_if_dead(slot as usize);
        }
    });
}

#[cfg(test)]
#[path = "env_registry_tests.rs"]
mod tests;
