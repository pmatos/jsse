# Node-Shim Array Extra-Property Inspection

## Context

The library-harness `util.inspect` shim renders an Array by reading its own
`length` descriptor and probing index descriptors from zero up to the existing
100-index safety cap. It never enumerates other own properties, so an enumerable
property such as `z` is absent from `[ 1, z: 2 ]`. The early return for a
zero-length Array also prevents any named properties on `[]` from being shown.

ECMAScript does not specify Node's `util.inspect` presentation. It does specify
the reflection seam used here: `Object.keys` calls `EnumerableOwnProperties` in
key mode, which invokes `[[OwnPropertyKeys]]` and `[[GetOwnProperty]]` to test
enumerability but does not perform `Get` on a property value. Array exotic
objects inherit the ordinary `[[OwnPropertyKeys]]` algorithm; their special
internal method is `[[DefineOwnProperty]]`. The resulting string keys place
array indices first in numeric order and other strings in property-creation
order.

`Object.keys` correctly enumerates indices and named properties on dynamically
populated Arrays; JSSE issue #516 recorded that `Object.getOwnPropertyNames` and
bare `Reflect.ownKeys` did not, and PR #520 has since fixed them. Availability is
therefore not the constraint — cost is. Enumerating any of the three after the
existing 100-index probe would materialize every dense index key and undo the
cap's runtime benefit.

## Approaches considered

1. Add a narrow `--node` host-floor hook backed by Array-only non-index String
   property metadata. This is selected. `ArrayData` maintains creation order for
   just those keys alongside the ordinary `property_order`; numeric indices and
   Symbols never enter the dedicated list, and deletion removes a key so a
   later re-creation appends it in the correct chronological position. The hook
   therefore reads no values and does work proportional to named String
   descriptors, independent of the Array's element count.
2. Walk the existing descriptor-backed `property_order` and discard indices.
   This was the initial implementation, but it is unbounded for literals and
   the many built-ins that route through `create_array`: those constructors put
   every present index in `property_order`. A deterministic probe over a
   20,000-entry `Array.from` result took 186--200 ms for 2,000 hook calls solely
   to discard index keys. This approach is rejected because it restores linear
   work per inspection.
3. Retain the bounded index probe, then enumerate `Object.keys` and discard
   array indices. This is functionally correct and trap-free after Proxy
   unwrapping, but rejected on measured performance: inspecting 100,000 filled
   elements took 0.34--0.38 seconds versus about 0.01 seconds on the capped
   `origin/main` implementation (a no-inspect control took about 0.02 seconds).
4. Enumerate `Reflect.ownKeys` or `Object.getOwnPropertyNames` (fixed by
   PR #520). This offers a more general reflection seam, but both APIs
   materialize every index key before the shim can filter it, retaining the
   dense-Array performance regression from approach 2. Now that #516 is closed
   this is the approach most likely to be retried; the cost, not availability,
   is what rules it out.

## Design

`renderArray` always allocates its local parts list, including when `length` is
zero. It keeps the existing descriptor probes over
`min(length, MAX_INSPECT_ARRAY_LENGTH)`, the sparse-hole coalescing, and the
truncation marker.

The Node host floor exposes
`__host_array_extra_keys(value) -> Array<string>`. For an Array target it walks
only `ArrayData::extra_string_property_order`, returning keys whose
descriptors are enumerable. For non-Arrays and primitives it returns an empty
Array. It does not read descriptor values or traverse a prototype. Like
`__host_proxy_target`, it is installed only under `--node` as a non-enumerable
configurable global; the shim captures and immediately deletes the binding
before bundled library code runs.

All Array construction paths install `ObjectKind::Array` before creating their
descriptors. The shared property-creation helper records non-index String keys
in the dedicated order while excluding canonical indices, Symbols, and the
mandatory `length` property. Since `length` is permanently non-enumerable, it
can never be returned by the hook; excluding it also keeps the metadata Vec
allocation-free for Arrays without extra properties. The shared removal helper
deletes from both orders. This does not alter ECMAScript `[[OwnPropertyKeys]]`:
the complete `property_order` and dense-element machinery remain authoritative
for ordinary reflection.

After the index window and marker, `renderArray` walks the captured hook's
result. Every key is labelled with the same identifier-or-quoted-string policy
as ordinary objects, then its own descriptor is passed to `renderDescriptor`.
A named getter or setter is therefore displayed as metadata and never called.

The plain-jsse fallback has no host metadata hook. For Arrays whose length is at
most the 100-entry inspection cap, it uses captured `Object.keys` and filters
array indices, preserving the reported behavior for normal small Arrays. For
longer Arrays it omits extra properties rather than performing an unbounded key
enumeration; the documented fallback is degraded, while the library harness
always runs with `--node` and receives the full behavior.

The array-index predicate implements the spec boundary precisely: the key's
numeric value must be an unsigned 32-bit integer below 2^32 - 1 and convert back
to the identical String. Thus `"0"` through `"4294967294"` are indices, while
`"4294967295"`, `"-0"`, `"01"`, fractional forms, and exponential spellings
remain named properties.

Enumerable Symbol properties remain outside this change, consistently with
the shim's existing generic-object path, which also uses `Object.keys`. Node's
full inspector supports a wider Symbol/hidden-key surface, but the shim is
explicitly best-effort and issue #517's reported divergence concerns named
string properties.

## Safety and failure behavior

Under `--node` the formatter unwraps active Proxies before `renderArray`, so the
hook and descriptor reads operate on the target without invoking handler traps.
The hook borrows only the Array's internal key order and descriptor
enumerability flags; values are observed only through descriptor objects already
used by the hardened inspector. A missing descriptor degrades to the existing
`undefined` rendering rather than a property access.

The plain-jsse fallback has no unwrapping seam, so `v` there may still be a
Proxy: `Object.keys` dispatches its `ownKeys` trap, and each named key it
yields is then read back through the `getOwnPropertyDescriptor` trap — both
user code, and both newly reachable, since the pre-existing probes only ever
touched `length` and index keys. Key-mode `EnumerableOwnProperties` can invoke
neither indexed nor named getters, but a hostile or merely throwing trap must
not turn a diagnostic print into a throw, so the enumeration *and* the
per-key descriptor reads are guarded together and degrade to no extra
properties. Parts are accumulated locally and returned only on success, so a
throw part-way through cannot leave a half-rendered tail.

## Validation

Rust host-floor tests will assert that the hook is absent without `--node`, is
non-enumerable/configurable with the floor enabled, returns only enumerable
non-index string keys from a densely populated Array, and invokes no getter.

The node-shim self-test will assert exact Node-compatible output for:

- the reported `[1]` with an enumerable `z` property;
- a zero-length Array with an enumerable named property;
- an enumerable named getter that is displayed without being called;
- `"4294967295"` and `"-0"` named properties; and
- a 101-element Array whose extra property follows the truncation marker.

The 116-shape differential corpus from PR #497 will be run before and after the
change on both `jsse --node` and the plain-jsse fallback. A deterministic
high-repeat hook probe over an `Array.from` result will catch work proportional
to descriptor-backed indices, while literal, `Array.from`, and `fill` fixtures
must return the same named keys. The node-shim self-test, all shim fixtures, the
Object.keys test262 area, and the repository's full quality gate will cover
regressions. The host hook is disabled for ordinary execution and test262; the
Array metadata itself is exercised by the full conformance run, so no pass-count
or README change is expected.
