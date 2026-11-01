//! The registry of environments a collection found clean, and the dirty list
//! that lets a minor collection visit only the ones written since (#1490).
//!
//! A registered environment is either clean (`BindingMap::is_clean`: no
//! nursery object in it, nothing written since) or has its slot on the dirty
//! list. The list is filled by the one store that turns a clean mark off
//! (`BindingMap`'s write barrier takes a branch only when the mark was set), and
//! drained by the next minor collection, so that collection costs the number of
//! environments written, not the number registered.
//!
//! The slab is per thread because the barrier sits in `BindingMap`, which
//! knows neither its environment nor the interpreter. Entries carry the owning
//! interpreter so that two interpreters on one thread never scan each other's
//! environments.

use super::super::types::{EnvRef, Environment};
use std::cell::RefCell;
use std::rc::Weak;

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
    live: usize,
    owners: u32,
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
            slab.owners
        });
        EnvRegistry { owner }
    }

    /// Environments registered on this thread.
    pub(crate) fn len(&self) -> usize {
        SLAB.with_borrow(|slab| slab.live)
    }

    /// Registers `env` unless it already is.
    pub(crate) fn register(&self, env: &EnvRef) {
        if env.borrow().bindings.slot() != NO_SLOT {
            return;
        }
        let weak = std::rc::Rc::downgrade(env);
        let slot = SLAB.with_borrow_mut(|slab| {
            slab.live += 1;
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
                if slab.entries[slot]
                    .as_ref()
                    .is_some_and(|entry| entry.env.strong_count() == 0)
                {
                    slab.entries[slot] = None;
                    slab.free.push(slot as u32);
                    slab.live -= 1;
                }
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
                        || (entry.env.strong_count() > 0 && !env_is_clean(&entry.env))
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

/// Reads the clean mark of a live environment without borrowing it.
pub(crate) fn env_is_clean(env: &Weak<RefCell<Environment>>) -> bool {
    // SAFETY: callers hold the environment alive (`strong_count() > 0`) and no
    // environment is borrowed mutably at a safepoint.
    unsafe {
        (*env.as_ptr())
            .try_borrow_unguarded()
            .is_ok_and(Environment::is_clean)
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
        let Ok(mut slab) = slab.try_borrow_mut() else {
            debug_assert!(false, "a dirty-list push lost to a borrow of the slab");
            return;
        };
        slab.dirty.push(slot);
    });
}

/// An environment's last act: its slot is free for another.
pub(crate) fn release(slot: u32) {
    let _ = SLAB.try_with(|slab| {
        let Ok(mut slab) = slab.try_borrow_mut() else {
            return;
        };
        if let Some(entry) = slab.entries.get_mut(slot as usize)
            && entry.as_ref().is_some_and(|e| e.env.strong_count() == 0)
        {
            *entry = None;
            slab.free.push(slot);
            slab.live -= 1;
        }
    });
}

#[cfg(test)]
#[path = "env_registry_tests.rs"]
mod tests;
