# Global Data Fast Path: re-validated own-property access with verified slot hints

Status: accepted (#905)

## Context

A free name that resolves to a global-object data property — every
script-level `var`, every builtin such as `Math` or `Int32Array`, every
implicit global — paid the full generic resolution in both the bytecode VM and
the tree-walker. A read walked the scope chain (an `Rc` clone per hop) and then
called `get_own_property` on the global object, which hashes the key and clones
a `PropertyDescriptor`. A write ran `find_binding_env`, `env_check_set_binding`,
`proxy_has_property`, a full `proxy_set` [[Set]] and `sync_global_object_binding`
(a realm scan): five or six property-map hashes per store, more for `x++`.
Benchmarks whose hot loop touches script-level variables (the Mandreel/
Emscripten output shape) ran 1.8-2x slower than the same loop over locals.

The issue proposed a `(shape, slot)` inline cache keyed on the global object's
`shape_id`. That is not available: `shape_id` is bumped by only *some*
structural mutators (`JsObjectData::remove_property`, `insert_property` and
several direct `properties.insert` calls do not bump it), and `PropertyMap` had
no stable slots.

## Decision

1. **Re-validate instead of invalidate.** `global_access.rs` holds the only
   code that decides "ordinary, own, data, (writable)". Each access probes the
   live property map once and answers `None` / `false` — leaving the object
   untouched — for a Proxy or exotic global object, a missing or inherited
   property, an accessor, or a non-writable property. Callers then run the
   unchanged generic path. Nothing observable can be stale because nothing
   observable is cached.
2. **Clone-free scope walk.** `walk_scope_chain` borrows each scope in turn.
   It never runs user code. Anything it does not model exactly (`with`, import
   bindings, TDZ, an over-deep chain) is `Slow` and the generic loop redoes the
   lookup from the start. The declarative record still wins over the object
   record: a name present in any scope, including the global environment's own
   `bindings`, is never read from the global object by this path.
3. **Verified slot hints.** `PropertyMap`'s spilled storage is a dense
   `Vec<(key, descriptor)>` plus a randomised key→slot index (the same shape as
   `BindingMap`). `Chunk.global_hints` holds one `GlobalHint` per name-pool
   entry: a remembered slot and the interned key of that name. A hit is "entry
   at slot has the same key allocation (or equal bytes)"; a miss re-probes by
   key and retrains. `remove` swap-moves the last entry into the hole and fixes
   the index, so a remembered slot can go stale — which is why it is only ever a
   hint, verified on every use, exactly like the `LoadLocal` slot hint
   (ADR-2026-10-12-0900). A hint must only be used with its own name.
4. **Name filter.** `BindingMap` keeps a 64-bit filter of the atoms ever
   pushed (top six hash bits). An atom lookup that misses a scope is one mask
   test instead of a scan; removal leaves bits set, so the filter only errs
   towards "maybe".
5. **Write barrier.** `global_object_store_own_data` performs
   `gc_write_barrier_value` before the in-place store, as `env_set` and the
   `proxy_set` own-data branch do.

## Consequences

- Same observable behaviour (the characterisation tests under
  `test262-extra/global-*.js` were written against the old path first) with one
  deliberate correction: a strict-mode write to a non-writable global property
  now throws `TypeError` (§9.1.1.2.5 SetMutableBinding / §10.1.9.2
  OrdinarySetWithOwnDescriptor with `S` true); `put_value_by_ref` used to drop
  `proxy_set`'s `false` result.
- No `shape_id` audit is needed and none is attempted; the general property
  inline cache (#71) is unaffected.
- A name found through a prototype of the global object, an accessor, or a
  non-writable property costs the old path plus one failed probe. Multiple
  realms sharing one chunk retrain the hint on each switch of global object;
  correctness is unaffected.
- Not done, deliberately: skipping the intermediate scope walk by a static hop
  count. Closures of a shared `Body`, sloppy direct-`eval` var injection and
  `with` would need a permanent "dynamic scope seen" poison flag plus a static
  structure argument. Left as follow-up work.
