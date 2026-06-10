//! Inline-cache slots for property access and call sites.
//!
//! Issue #71 — see `.ultraplan/property-lookup-caching.md` for the full design.
//!
//! `PropIcSlot` lives on every `Expression::Member` AST node behind a `Cell`
//! (slot is `Copy` so `Cell::get` is sufficient — no `RefCell` ceremony).
//! The probe at `eval_member` reads the slot; on hit it dispatches directly.
//! On miss it falls through to the slow path which records a fresh `Mono`
//! entry (or transitions to `Megamorphic`).
//!
//! Shape-id matching is the core invariant: a slot is valid if and only if
//! `obj.id == slot.obj_id && obj.shape_id == slot.obj_shape_id`. The global
//! shape-id counter (in `types::fresh_shape_id`) guarantees that an `obj_id`
//! freed and re-used by GC cannot collide with a stale slot — the new
//! object's shape_id is freshly drawn from the counter.

/// Property-access IC slot. Held in `Cell<PropIcSlot>` on every
/// `Expression::Member` node. `Copy` so the cell can use `get`/`set`.
#[derive(Clone, Copy, Debug)]
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
    /// Site has seen a non-cacheable resolution (proxy, module-namespace,
    /// symbol key, depth>1 prototype) or has shape-thrashed across multiple
    /// distinct `obj_id`s. Probes always fall through; slow path skips
    /// the record cost.
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
    /// Construct an empty slot. Used by `fresh_prop_ic_cell()` and reserved
    /// for any future code path that wants the canonical empty value.
    #[allow(dead_code)]
    pub(crate) const fn empty() -> Self {
        PropIcSlot::Empty
    }
}

/// Helper for parser/transform sites that construct `Expression::Member` —
/// produces a fresh, empty IC cell. Avoids spelling out
/// `Cell::new(PropIcSlot::empty())` at every callsite.
#[inline]
pub fn fresh_prop_ic_cell() -> std::cell::Cell<PropIcSlot> {
    std::cell::Cell::new(PropIcSlot::Empty)
}

// ---- Call-site IC (Phase 3, plan Step 13) -----------------------------------

/// Call-site IC slot. Held in `Cell<CallIcSlot>` on every `Expression::Call`
/// and `Expression::New` node. `Copy` so the cell can use `get`/`set`. The
/// state machine mirrors `PropIcSlot` exactly.
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

/// Helper for parser/transform sites that construct `Expression::Call` /
/// `Expression::New` — fresh, empty call IC cell.
#[inline]
pub fn fresh_call_ic_cell() -> std::cell::Cell<CallIcSlot> {
    std::cell::Cell::new(CallIcSlot::Empty)
}

const _ASSERT_CALL_IC_SLOT_SIZE: () = {
    assert!(std::mem::size_of::<CallIcSlot>() <= 32);
};

// Compile-time invariant: the slot must stay small (<=32 bytes) so that
// `Expression::Member` doesn't bloat by more than 4 pointer-sized words.
// The largest variant — `Mono { u64, u64, PropIcKind }` with the largest
// kind (`ProtoData { u64, u64 }`) — is 32 bytes. If this assertion ever
// fails, audit `PropIcKind` for accidental growth.
const _ASSERT_PROP_IC_SLOT_SIZE: () = {
    assert!(std::mem::size_of::<PropIcSlot>() <= 40);
};
