//! Key lists shared by objects that were built the same way (#1401).
//!
//! tsc allocates millions of AST nodes, symbols and types that each get the
//! same dozen properties in the same order, and a per-object entry list paid a
//! 16-byte key for every one of them plus an index. A [`Shape`] is the ordered
//! key list of such a family, with its lookup tags, and an object keeps only an
//! `Rc` to it and one [`PropertyRecord`](super::PropertyRecord) per property.
//! Attributes stay in the record, so a shape says nothing about them and a
//! redefinition never leaves it.
//!
//! Adding a key moves the object to a child shape found in the parent's
//! transition table. A shape is created the first time a sequence is seen and
//! lives while an object or a child shape uses it; the table holds it weakly.
//! Anything unusual (a deletion, more than [`SHAPED_MAX`] keys, a full
//! transition table, too many live shapes) moves the object to the per-object
//! representation instead.

use super::slot_index::{scan_tags, set_tag, tag_of};
use super::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// Most keys a shape holds; the tag index of a shape is at most six words.
pub(super) const SHAPED_MAX: usize = 48;

/// Most children one shape keeps. A node base shape of the TypeScript AST has
/// a child per node kind (about a hundred and thirty).
const MAX_FAN_OUT: usize = 512;

/// Bound on the live shapes of one thread, so objects used as dictionaries of
/// ever-new key sequences cannot build shape trees without limit.
const MAX_LIVE_SHAPES: usize = 50_000;

/// A shape up to this size is searched by comparing keys one by one.
const LINEAR_SCAN_MAX: usize = 8;

/// Up to this many children are kept in a list, more in a map.
const FEW_CHILDREN: usize = 8;

/// The children of one key, by attribute bits.
type ByAttrs = Box<[Weak<Shape>; 8]>;

type ChildMap = HashMap<JsPropertyKey, ByAttrs, foldhash::fast::FixedState>;

enum Children {
    Few(Vec<(JsPropertyKey, u8, Weak<Shape>)>),
    Many(ChildMap),
}

fn dead_slots() -> ByAttrs {
    Box::new(std::array::from_fn(|_| Weak::new()))
}

pub(super) struct Shape {
    keys: Box<[JsPropertyKey]>,
    /// Writable, enumerable and configurable bits of each key (`record::ATTR_*`).
    attrs: Box<[u8]>,
    /// `tag_of` of each key, eight to a word, as in `SlotIndex::Tags`.
    tags: Box<[u64]>,
    /// Keeps the chain of ancestors alive: a transition is only found from a
    /// parent that exists.
    parent: Option<Rc<Shape>>,
    children: RefCell<Children>,
}

thread_local! {
    static ROOT: Rc<Shape> = Rc::new(Shape {
        keys: Box::new([]),
        attrs: Box::new([]),
        tags: Box::new([]),
        parent: None,
        children: RefCell::new(Children::Few(Vec::new())),
    });
    static ENABLED: Cell<bool> = const { Cell::new(true) };
    static LIVE: Cell<usize> = const { Cell::new(0) };
}

/// While alive, objects built on this thread get per-object entry lists. The
/// builtin objects of a realm are created like this: each is a family of one,
/// and a shape per property would only slow the start of every process.
pub(crate) struct ShapesPaused(bool);

impl ShapesPaused {
    pub(crate) fn new() -> Self {
        Self(ENABLED.replace(false))
    }
}

impl Drop for ShapesPaused {
    fn drop(&mut self) {
        ENABLED.set(self.0);
    }
}

#[inline]
pub(super) fn shapes_enabled() -> bool {
    ENABLED.get()
}

impl Drop for Shape {
    fn drop(&mut self) {
        if self.parent.is_some() {
            LIVE.set(LIVE.get().saturating_sub(1));
        }
    }
}

impl Shape {
    pub(super) fn root() -> Rc<Shape> {
        ROOT.with(Rc::clone)
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.keys.len()
    }

    #[inline]
    pub(super) fn keys(&self) -> &[JsPropertyKey] {
        &self.keys
    }

    #[inline]
    pub(super) fn attrs(&self) -> &[u8] {
        &self.attrs
    }

    #[inline]
    pub(super) fn parent(&self) -> Option<&Rc<Shape>> {
        self.parent.as_ref()
    }

    #[inline]
    pub(super) fn find(&self, key: &[u8]) -> Option<usize> {
        if self.keys.len() <= LINEAR_SCAN_MAX {
            return self
                .keys
                .iter()
                .position(|k| key_bytes_eq(k.as_bytes(), key));
        }
        scan_tags(&self.tags, key, |slot| {
            self.keys
                .get(slot)
                .is_some_and(|k| key_bytes_eq(k.as_bytes(), key))
        })
    }

    /// The shape of this one's objects after they gain `key`, which they do
    /// not have yet, with attribute bits `attrs`. `None` when the object
    /// should leave shapes.
    pub(super) fn with_key(self: &Rc<Self>, key: &JsPropertyKey, attrs: u8) -> Option<Rc<Shape>> {
        if self.keys.len() >= SHAPED_MAX {
            return None;
        }
        let mut children = self.children.borrow_mut();
        let known = match &*children {
            Children::Few(list) => list
                .iter()
                .find(|(k, a, _)| *a == attrs && key_bytes_eq(k.as_bytes(), key.as_bytes()))
                .map(|(_, _, shape)| shape),
            Children::Many(map) => map
                .get(key.as_bytes())
                .map(|by_attrs| &by_attrs[usize::from(attrs)]),
        };
        if let Some(shape) = known.and_then(Weak::upgrade) {
            return Some(shape);
        }
        if LIVE.get() >= MAX_LIVE_SHAPES {
            return None;
        }
        if let Children::Many(map) = &mut *children
            && map.len() >= MAX_FAN_OUT
            && !map.contains_key(key.as_bytes())
        {
            map.retain(|_, slots| slots.iter().any(|shape| shape.strong_count() > 0));
            if map.len() >= MAX_FAN_OUT {
                // The child would be rebuilt by every object that follows it.
                return None;
            }
        }
        let child = Rc::new(self.extended(key, attrs));
        LIVE.set(LIVE.get() + 1);
        let weak = Rc::downgrade(&child);
        match &mut *children {
            Children::Few(list) => {
                if let Some(slot) = list
                    .iter_mut()
                    .find(|(k, a, _)| *a == attrs && key_bytes_eq(k.as_bytes(), key.as_bytes()))
                {
                    slot.2 = weak;
                } else {
                    if list.len() >= FEW_CHILDREN {
                        list.retain(|(_, _, shape)| shape.strong_count() > 0);
                    }
                    if list.len() < FEW_CHILDREN {
                        list.push((key.clone(), attrs, weak));
                    } else {
                        let mut map = ChildMap::default();
                        for (k, a, shape) in list.drain(..) {
                            map.entry(k).or_insert_with(dead_slots)[usize::from(a)] = shape;
                        }
                        map.entry(key.clone()).or_insert_with(dead_slots)[usize::from(attrs)] =
                            weak;
                        *children = Children::Many(map);
                    }
                }
            }
            Children::Many(map) => {
                map.entry(key.clone()).or_insert_with(dead_slots)[usize::from(attrs)] = weak;
            }
        }
        Some(child)
    }

    fn extended(self: &Rc<Self>, key: &JsPropertyKey, attr_bits: u8) -> Shape {
        let slot = self.keys.len();
        let mut keys = Vec::with_capacity(slot + 1);
        keys.extend(self.keys.iter().cloned());
        keys.push(key.clone());
        let mut attrs = Vec::with_capacity(slot + 1);
        attrs.extend_from_slice(&self.attrs);
        attrs.push(attr_bits);
        let mut tags = Vec::with_capacity((slot + 1).div_ceil(8));
        tags.extend_from_slice(&self.tags);
        set_tag(&mut tags, slot, tag_of(key.as_bytes()));
        Shape {
            keys: keys.into_boxed_slice(),
            attrs: attrs.into_boxed_slice(),
            tags: tags.into_boxed_slice(),
            parent: Some(Rc::clone(self)),
            children: RefCell::new(Children::Few(Vec::new())),
        }
    }
}
