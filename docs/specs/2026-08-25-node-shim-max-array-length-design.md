# Bounded Node-Shim Array Inspection

## Context

The library-harness `util.inspect` shim reads an Array's own `length` data
descriptor, then probes every index with `Object.getOwnPropertyDescriptor`.
This avoids inherited getters but makes inspection O(`length`): logging
`new Array(1_000_000)` performs one million index-descriptor reads.

ECMAScript specifies Array `length` and indexed own properties, but not Node's
host-provided `util.inspect`. Node 26.5.0 defaults `maxArrayLength` to 100. It
truncates 101 dense elements after 100, but coalesces a sparse hole run into one
rendered entry and can therefore find distant elements without scanning every
index. JSSE cannot efficiently reproduce that sparse behavior until issue #516
fixes own-index enumeration for arrays populated after creation.

## Approaches considered

1. Keep scanning until 100 rendered values or hole runs have been found. This
   follows Node's sparse presentation more closely, but an all-hole array still
   scans its complete length and leaves the reported hang intact.
2. Enumerate own index keys and render at most 100 entries. This is both bounded
   and closest to Node, but `Object.getOwnPropertyNames` and `Reflect.ownKeys`
   currently omit indices added by `push`, assignment, or `fill` (issue #516).
3. Probe only the first 100 indices and represent the remaining indices with a
   truncation suffix. This is selected because it provides a hard complexity
   bound with the existing reliable descriptor seam. Dense arrays match Node;
   sparse arrays may truncate holes or omit distant elements and are explicitly
   best-effort until issue #516 enables approach 2.

## Design

`renderArray` limits its descriptor loop to `min(length, 100)`. Existing hole
coalescing and accessor-safe descriptor rendering remain unchanged inside that
window. If `length` exceeds the window, it appends `... N more item` or
`... N more items`, where `N` is the number of uninspected indices.

The cap is deliberately fixed rather than exposing the full Node
`maxArrayLength` option surface: issue #515 asks for the default safety bound,
and the shim documents `util.inspect` as a best-effort formatter rather than a
complete Node implementation.

## Validation

The node-shim self-test will assert the exact dense 101-element rendering on
both engines. A one-million-hole case will assert Node's native coalesced output
on Node and the shim's bounded prefix-plus-suffix output on JSSE, ensuring the
performance-triggering shape cannot regress to a full-length scan. This is a
host-harness change, not ECMAScript behavior, so there is no targeted test262
area and no test262 pass-count change expected; the normal full-suite gate is
still run for regressions.
