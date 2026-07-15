//! Inline-cache slots for property access and call sites.
//!
//! Issue #71 — see `.ultraplan/property-lookup-caching.md` for the full design.
//!
//! The slot values are stored in a per-`Body` `BodyIcStore` in the interpreter,
//! keyed by the identity of the executing `Body`. The AST carries only dense
//! `CallSiteId` / `PropSiteId` identifiers (see `src/interpreter/ic_store.rs`).
//! The probe at `eval_member` reads the slot via `Interpreter::prop_slot`; on
//! hit it dispatches directly. On miss it falls through to the slow path which
//! feeds the freshly classified resolution into `PropIcSlot::advance`, driving
//! the `Empty → Mono → Poly(≤MAX_POLY_PROP) → Megamorphic` state machine.
//!
//! Shape-id matching is the core invariant: a slot is valid if and only if
//! `obj.id == slot.obj_id && obj.shape_id == slot.obj_shape_id`. The global
//! shape-id counter (in `types::fresh_shape_id`) guarantees that an `obj_id`
//! freed and re-used by GC cannot collide with a stale slot — the new
//! object's shape_id is freshly drawn from the counter.

/// Maximum number of distinct `(obj_id, shape_id)` pairs cached at a single
/// property-access site before it degrades to `Megamorphic`. Four mirrors the
/// conventional inline-cache arity: enough to cover a site that genuinely sees
/// a small fixed set of objects (issue #71's motivating case is a handful of
/// long-lived globals read from the same site), while keeping the linear probe
/// scan short.
pub(crate) const MAX_POLY_PROP: usize = 4;

/// One cached resolution at a property-access site: the object identity and
/// shape version it was captured against, plus the resolution `kind` the probe
/// dispatches on. A `Mono` slot holds one of these inline; a `Poly` slot holds
/// up to `MAX_POLY_PROP` of them out-of-line.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PropIcEntry {
    pub obj_id: u64,
    pub obj_shape_id: u64,
    pub kind: PropIcKind,
}

/// Property-access IC slot. Stored in a `BodyIcStore` slot and looked up by
/// `PropSiteId`.
///
/// Not `Copy`: the `Poly` variant owns a heap `Vec` of entries so the slot
/// stays small (the `Empty`/`Mono`/`Megamorphic` variants carry no allocation
/// and the whole slot fits within the size budget below). The probe never
/// copies the slot — it borrows it, extracts the matching entry's Copy fields,
/// and releases the borrow before touching the heap. Only the slow-path record
/// step clones, and only when it grows a `Poly`.
#[derive(Clone, Debug)]
pub(crate) enum PropIcSlot {
    /// First execution at this site, or just transitioned out of `Mono` due
    /// to a non-cacheable resolution. Probe falls through to slow path; slow
    /// path may write a fresh `Mono` entry.
    Empty,
    /// One object/shape pair has been seen at this site. Probe checks the
    /// current `(obj_id, shape_id)` against the cached pair and dispatches
    /// directly on `kind` if they match.
    Mono {
        obj_id: u64,
        obj_shape_id: u64,
        kind: PropIcKind,
    },
    /// Two-to-`MAX_POLY_PROP` distinct objects have been seen at this site.
    /// Probe scans the entries for one whose `(obj_id, shape_id)` matches the
    /// current object and dispatches on that entry's `kind`. Invariants:
    /// `2 <= entries.len() <= MAX_POLY_PROP` and every entry has a distinct
    /// `obj_id`.
    Poly(Vec<PropIcEntry>),
    /// Site has seen a non-cacheable resolution (proxy, module-namespace,
    /// symbol key, depth>1 prototype) or has cached the maximum number of
    /// distinct `obj_id`s and then seen yet another. Probes always fall
    /// through; slow path skips the record cost.
    Megamorphic,
}

/// Resolution category captured in a `Mono` slot. Carries enough information
/// for the probe to dispatch without re-walking proxy/module-ns/typed-array
/// detection or the prototype chain.
///
/// Constructs `OwnData`, `Missing`, and depth-1 `ProtoData`. `OwnAccessor`
/// and `TypedArrayElement` are reserved for follow-up cycles (the probe path
/// already handles them defensively, so adding the recording hook later is a
/// small change).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum PropIcKind {
    /// Property resolved as a data descriptor on the target object.
    /// Probe re-fetches via `PropertyMap.get(name)` (skipping proxy/ns/ta
    /// detection) — re-fetch is required because pure value reassignment
    /// does NOT bump shape_id (plan Step 5).
    OwnData,
    /// Property resolved as an accessor descriptor on the target object.
    /// Probe re-fetches the descriptor and invokes the getter.
    OwnAccessor,
    /// Property resolved on the immediate prototype (depth 1) as a data
    /// descriptor. Probe verifies the receiver shape, the receiver's current
    /// `prototype_id` (a proto swap does NOT bump the receiver shape), AND the
    /// prototype's shape before re-fetching the value from the prototype's own
    /// data property.
    ProtoData { proto_id: u64, proto_shape_id: u64 },
    /// Property is absent up to and including the immediate prototype.
    /// Probe verifies the prototype shape (or `proto_id == None`) and
    /// returns `undefined` directly.
    Missing {
        /// `None` means the receiver had `prototype_id == None` at capture;
        /// `proto_shape_id` is unused in that case (always 0).
        proto_id: Option<u64>,
        proto_shape_id: u64,
    },
    /// Numeric index access on a typed-array. Probe takes the existing
    /// `is_valid_integer_index` + `typed_array_get_index` fast path
    /// without re-checking proxy/ns flags.
    #[allow(dead_code)] // wired up in Phase 2 Step 10
    TypedArrayElement,
}

impl PropIcSlot {
    /// Construct an empty slot. Reserved for any future code path that wants
    /// the canonical empty value.
    #[allow(dead_code)]
    pub(crate) const fn empty() -> Self {
        PropIcSlot::Empty
    }

    /// Apply the IC state-machine transition after a slow-path resolution.
    ///
    /// `observed` is the freshly classified entry for the current access
    /// (`Some(entry)` if cacheable, `None` if not — proxy / module-namespace /
    /// typed-array / own-accessor / depth>1). `self` is the slot as it stood
    /// before this access.
    ///
    /// - `Megamorphic` is terminal.
    /// - A non-cacheable resolution resets `Empty`/`Mono` to `Empty` (giving
    ///   the site another chance) but pushes a `Poly` site — which has already
    ///   proven it sees several shapes — to `Megamorphic`.
    /// - `Empty` records the first `Mono`. A second distinct object promotes
    ///   `Mono` to a two-entry `Poly`. Further distinct objects extend the
    ///   `Poly` up to `MAX_POLY_PROP`; one more distinct object degrades it to
    ///   `Megamorphic`. Re-seeing a cached `obj_id` refreshes that entry in
    ///   place (its shape/kind may have advanced).
    pub(crate) fn advance(&self, observed: Option<PropIcEntry>) -> PropIcSlot {
        match (self, observed) {
            (PropIcSlot::Megamorphic, _) => PropIcSlot::Megamorphic,
            // Diverse site meets a non-cacheable shape → give up caching it.
            (PropIcSlot::Poly(_), None) => PropIcSlot::Megamorphic,
            // Empty/Mono meet a non-cacheable shape → stay resettable.
            (_, None) => PropIcSlot::Empty,
            (PropIcSlot::Empty, Some(e)) => PropIcSlot::Mono {
                obj_id: e.obj_id,
                obj_shape_id: e.obj_shape_id,
                kind: e.kind,
            },
            (
                PropIcSlot::Mono {
                    obj_id,
                    obj_shape_id,
                    kind,
                },
                Some(e),
            ) => {
                if *obj_id == e.obj_id {
                    // Same object — refresh (shape/kind may have advanced).
                    PropIcSlot::Mono {
                        obj_id: e.obj_id,
                        obj_shape_id: e.obj_shape_id,
                        kind: e.kind,
                    }
                } else {
                    PropIcSlot::Poly(vec![
                        PropIcEntry {
                            obj_id: *obj_id,
                            obj_shape_id: *obj_shape_id,
                            kind: *kind,
                        },
                        e,
                    ])
                }
            }
            (PropIcSlot::Poly(entries), Some(e)) => {
                let mut entries = entries.clone();
                if let Some(slot) = entries.iter_mut().find(|x| x.obj_id == e.obj_id) {
                    *slot = e; // refresh existing entry
                    PropIcSlot::Poly(entries)
                } else if entries.len() < MAX_POLY_PROP {
                    entries.push(e);
                    PropIcSlot::Poly(entries)
                } else {
                    PropIcSlot::Megamorphic
                }
            }
        }
    }
}

// ---- Call-site IC (Phase 3, plan Step 13) -----------------------------------

/// Call-site IC slot. Stored in a `BodyIcStore` slot and looked up by
/// `CallSiteId`. `Copy` so reads and writes can be done with short-lived
/// mutable borrows. The state machine mirrors `PropIcSlot` exactly.
///
/// Phase-3 v1 reads `callee_obj_id` and `callee_shape_id` for the hit check
/// but does not yet branch on `kind` — the fast-dispatch entry that uses it
/// to skip proxy/wrapped/class-ctor checks is the next perf-only cycle. The
/// field is captured at record time so the future cycle is purely additive.
#[derive(Clone, Copy, Debug)]
pub(crate) enum CallIcSlot {
    Empty,
    Mono {
        /// Object id of the callable (the function object itself, not its
        /// receiver). Probe checks `callee.id == this`.
        callee_obj_id: u64,
        /// Shape of the callable at capture time. If the callable's shape
        /// has advanced (e.g. a property was added that changed proxy/bound
        /// status), the cached `kind` is stale.
        callee_shape_id: u64,
        #[allow(dead_code)] // wired for the call_function_fast follow-up
        kind: CallIcKind,
    },
    Megamorphic,
}

/// Resolution category captured in a `Mono` call slot. Carries enough
/// information for the probe to skip the proxy/wrapped/class-ctor entry
/// checks in `call_function_inner` and dispatch to the appropriate
/// JsFunction variant directly.
///
/// Phase-3 v1 only constructs `NativeFn` and `UserFn`; bound/wrapped/proxy
/// callables stay slow. The probe path verifies `callable` is still the
/// expected variant before dispatching, so a shape-stable mutation that
/// somehow swapped variants would degrade to a single mis-prediction
/// rather than miscompile.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum CallIcKind {
    /// `obj.callable == Some(JsFunction::Native(_))`. No proxy, no wrapped,
    /// not a class constructor without `new`.
    NativeFn,
    /// `obj.callable == Some(JsFunction::User { .. })`. Same exclusions.
    UserFn,
}

impl CallIcSlot {
    #[allow(dead_code)]
    pub(crate) const fn empty() -> Self {
        CallIcSlot::Empty
    }
}

const _ASSERT_CALL_IC_SLOT_SIZE: () = {
    assert!(std::mem::size_of::<CallIcSlot>() <= 32);
};

// Compile-time invariant: the slot must stay small (<=40 bytes). Slots live in
// the per-body `BodyIcStore` (see `docs/adr/0001-inline-cache-ast-seam.md`),
// sized one-per-site, so a bloated slot would inflate that `Vec` across every
// property-access site in a body. The `Poly` variant keeps its entries
// out-of-line in a `Vec` (a 3-word header) precisely so the slot stays small;
// the largest inline variant is `Mono { u64, u64, PropIcKind }`. If this
// assertion ever fails, audit `PropIcKind` for accidental growth.
const _ASSERT_PROP_IC_SLOT_SIZE: () = {
    assert!(std::mem::size_of::<PropIcSlot>() <= 40);
};

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(obj_id: u64, shape: u64) -> PropIcEntry {
        PropIcEntry {
            obj_id,
            obj_shape_id: shape,
            kind: PropIcKind::OwnData,
        }
    }

    /// Collect `(obj_id, obj_shape_id)` from a `Poly` slot for assertions.
    fn poly_ids(slot: &PropIcSlot) -> Vec<(u64, u64)> {
        match slot {
            PropIcSlot::Poly(entries) => {
                entries.iter().map(|e| (e.obj_id, e.obj_shape_id)).collect()
            }
            other => panic!("expected Poly, got {other:?}"),
        }
    }

    #[test]
    fn empty_records_first_mono() {
        let next = PropIcSlot::Empty.advance(Some(entry(1, 10)));
        assert!(matches!(
            next,
            PropIcSlot::Mono {
                obj_id: 1,
                obj_shape_id: 10,
                ..
            }
        ));
    }

    #[test]
    fn mono_same_object_refreshes_shape() {
        let mono = PropIcSlot::Mono {
            obj_id: 1,
            obj_shape_id: 10,
            kind: PropIcKind::OwnData,
        };
        // Same obj_id, advanced shape — must refresh in place, not promote.
        let next = mono.advance(Some(entry(1, 11)));
        assert!(matches!(
            next,
            PropIcSlot::Mono {
                obj_id: 1,
                obj_shape_id: 11,
                ..
            }
        ));
    }

    #[test]
    fn mono_second_object_promotes_to_poly() {
        let mono = PropIcSlot::Mono {
            obj_id: 1,
            obj_shape_id: 10,
            kind: PropIcKind::OwnData,
        };
        let next = mono.advance(Some(entry(2, 20)));
        assert_eq!(poly_ids(&next), vec![(1, 10), (2, 20)]);
    }

    #[test]
    fn poly_extends_up_to_max_then_megamorphic() {
        let mut slot = PropIcSlot::Empty.advance(Some(entry(1, 10)));
        // Feed distinct objects 2..=MAX_POLY_PROP: each extends the Poly.
        for id in 2..=MAX_POLY_PROP as u64 {
            slot = slot.advance(Some(entry(id, id * 10)));
        }
        assert_eq!(poly_ids(&slot).len(), MAX_POLY_PROP);
        // One more distinct object overflows to Megamorphic.
        let next = slot.advance(Some(entry(MAX_POLY_PROP as u64 + 1, 999)));
        assert!(matches!(next, PropIcSlot::Megamorphic));
    }

    #[test]
    fn poly_refreshes_existing_entry_in_place() {
        let slot = PropIcSlot::Poly(vec![entry(1, 10), entry(2, 20)]);
        // Re-see obj 1 with an advanced shape: refresh, do not grow.
        let next = slot.advance(Some(entry(1, 11)));
        assert_eq!(poly_ids(&next), vec![(1, 11), (2, 20)]);
    }

    #[test]
    fn mono_non_cacheable_resets_to_empty() {
        let mono = PropIcSlot::Mono {
            obj_id: 1,
            obj_shape_id: 10,
            kind: PropIcKind::OwnData,
        };
        assert!(matches!(mono.advance(None), PropIcSlot::Empty));
        assert!(matches!(PropIcSlot::Empty.advance(None), PropIcSlot::Empty));
    }

    #[test]
    fn poly_non_cacheable_goes_megamorphic() {
        let slot = PropIcSlot::Poly(vec![entry(1, 10), entry(2, 20)]);
        assert!(matches!(slot.advance(None), PropIcSlot::Megamorphic));
    }

    #[test]
    fn megamorphic_is_terminal() {
        assert!(matches!(
            PropIcSlot::Megamorphic.advance(Some(entry(1, 10))),
            PropIcSlot::Megamorphic
        ));
        assert!(matches!(
            PropIcSlot::Megamorphic.advance(None),
            PropIcSlot::Megamorphic
        ));
    }
}
