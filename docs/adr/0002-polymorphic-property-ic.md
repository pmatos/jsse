# Property inline caches are polymorphic; call inline caches stay monomorphic

The property-access inline cache (issue #71) originally had a two-state cache
lattice: a site cached exactly one object (`Mono`) or gave up entirely
(`Megamorphic`). A site that saw a second distinct object jumped straight to
`Megamorphic` and never cached again. Because `shape_id` is a per-object version
counter (every allocation draws a fresh id) rather than a shared hidden class,
"the same object at a site" is the unit that hits — so a site that reads a small
fixed set of long-lived objects (issue #71's motivating case: a handful of
globals read from one call site) lost its cache the moment the second object
appeared.

We extended the lattice to `Empty → Mono → Poly(≤ MAX_POLY_PROP) → Megamorphic`.
`Poly` caches up to `MAX_POLY_PROP` (4) distinct `(obj_id, shape_id, kind)`
entries; the probe scans them linearly. A fifth distinct object degrades the
site to `Megamorphic`. Re-seeing a cached object refreshes its entry in place
(its shape/kind may have advanced). A non-cacheable resolution (proxy /
module-namespace / typed-array / own-accessor / depth>1) still resets an
`Empty`/`Mono` site to `Empty`, but pushes a `Poly` site — which has already
proven it sees several shapes — to `Megamorphic`.

The transition is a pure function, `PropIcSlot::advance(&self, Option<PropIcEntry>)`,
so the state machine is unit-tested in isolation from the evaluator.

## Storage: out-of-line entries, non-`Copy` slot

`Poly` owns a heap `Vec<PropIcEntry>` rather than an inline fixed array. This
keeps `PropIcSlot` within its ≤40-byte budget (the `BodyIcStore` holds one slot
per property-access site, so a bloated slot would multiply across the whole
body). The trade-off is that `PropIcSlot` is no longer `Copy`. The probe never
copies the slot: it borrows it, extracts the matching entry's `Copy` fields
(`obj_shape_id`, `kind`), and releases the borrow before touching any object.
Only the slow-path record step clones, and only when it grows a `Poly` — an
allocation that lands on the miss path, never the hit path.

## Scope: property IC only

We made only the property IC polymorphic. The call IC (`CallIcSlot`) keeps its
`Mono`/`Megamorphic` shape. Call sites in the motivating workloads are
overwhelmingly monomorphic (one function per site), so polymorphic call caching
would add scan cost and a second non-`Copy` slot type for little gain. It
remains a clean follow-up if a polymorphic-call workload shows up.

## Consequences

- Sites that alternate among a small fixed set of objects now hit instead of
  going megamorphic on the second object.
- `PropIcSlot` is `Clone` but not `Copy`; the two IC slot types are no longer
  structurally identical (`CallIcSlot` is still `Copy`).
- Per-site memory is unchanged in the common case (`Empty`/`Mono` carry no
  allocation); only genuinely polymorphic sites allocate a small `Vec`.
