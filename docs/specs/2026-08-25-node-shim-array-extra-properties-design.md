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

JSSE issue #516 records that `Object.keys` correctly enumerates indices and
named properties on dynamically populated Arrays even though the currently
unmerged `Object.getOwnPropertyNames`/bare `Reflect.ownKeys` fix is still in
PR #520. It is functionally reliable, but enumerating it after the existing
100-index probe would materialize every dense index key and undo the cap's
runtime benefit.

## Approaches considered

1. Add a narrow `--node` host-floor hook that returns only an Array's enumerable
   non-index own string keys. This is selected: it walks the descriptor-backed
   `property_order` without walking the dense element backing store, so the
   formatter remains O(100 + named/special descriptors). The shim still reads
   values only from ordinary own descriptor objects.
2. Retain the bounded index probe, then enumerate `Object.keys` and discard
   array indices. This is functionally correct and trap-free after Proxy
   unwrapping, but rejected on measured performance: inspecting 100,000 filled
   elements took 0.34--0.38 seconds versus about 0.01 seconds on the capped
   `origin/main` implementation (a no-inspect control took about 0.02 seconds).
3. Depend on PR #520 and enumerate `Reflect.ownKeys` or
   `Object.getOwnPropertyNames`. This offers a more general reflection seam but
   couples this shim fix to an unmerged engine change. Both APIs also
   materialize every index key before the shim can filter it, retaining the
   dense-Array performance regression from approach 2.

## Design

`renderArray` always allocates its local parts list, including when `length` is
zero. It keeps the existing descriptor probes over
`min(length, MAX_INSPECT_ARRAY_LENGTH)`, the sparse-hole coalescing, and the
truncation marker.

The Node host floor exposes
`__host_array_extra_keys(value) -> Array<string>`. For an Array target it walks
only `property_order`, returning keys whose descriptors are enumerable Strings
and whose names are not canonical array indices. For non-Arrays and primitives
it returns an empty Array. It does not read descriptor values or traverse a
prototype. Like `__host_proxy_target`, it is installed only under `--node` as a
non-enumerable configurable global; the shim captures and immediately deletes
the binding before bundled library code runs.

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

The formatter unwraps active Proxies before `renderArray`, so the hook and
descriptor reads operate on the target without invoking handler traps. The hook
borrows only the Array's internal key order and descriptor enumerability flags;
values are observed only through descriptor objects already used by the
hardened inspector. In the small-Array fallback, key-mode
`EnumerableOwnProperties` can invoke neither indexed nor named getters. A
missing descriptor degrades to the existing `undefined` rendering rather than
a property access.

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
change on both `jsse --node` and the plain-jsse fallback. A 100,000-element
dense benchmark will confirm that inspection remains close to the capped
implementation rather than the rejected `Object.keys` pass. The node-shim
self-test, all shim fixtures, the Object.keys test262 area, and the repository's
full quality gate will cover regressions. The hook is disabled for ordinary
execution and test262, so no conformance pass-count or README change is
expected.
