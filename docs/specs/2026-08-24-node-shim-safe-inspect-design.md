# Proxy- and Accessor-Safe Node Shim Inspection

## Context

`scripts/node-shim.js` implements the best-effort `util.inspect` used by the
Node-compatible library harness. Descriptor-based member rendering already
avoids ordinary getters, but reflection on a Proxy still invokes its traps.
Array length, sparse holes, and Error formatting also retain direct property
reads that can invoke user code.

ECMAScript requires `Object.getPrototypeOf`, `Object.keys`, and
`Object.getOwnPropertyDescriptor` to dispatch through a Proxy's corresponding
internal methods. `Array.isArray` is different: `IsArray` unwraps Proxy targets
through internal slots without calling a handler trap. Consequently, a
pure-JavaScript formatter cannot both recognize and inspect arbitrary Proxies
without invoking traps.

## Approaches considered

1. Catch reflection failures and render an opaque object. This prevents a
   throwing trap from escaping, but the trap still runs and can mutate state,
   so it does not satisfy the acceptance criterion.
2. Move the complete formatter into Rust. This provides safe access to all
   metadata, but duplicates the existing JavaScript formatting policy and
   substantially expands the native host surface.
3. Add a single, `--node`-gated Proxy-target metadata hook, then retain the
   formatter in JavaScript. The shim captures the hook before library code can
   replace it, recursively unwraps Proxies before reflection, and handles
   revoked Proxies as an opaque terminal value. This is the selected approach.

## Design

The Node host floor exposes `__host_proxy_target(value)` only when `--node` is
enabled. It returns:

- `undefined` when `value` is not a Proxy;
- the active Proxy's target object without consulting the handler; or
- `null` for a revoked Proxy.

The shim captures this function with its other host primordials. At the start
of object/function rendering it repeatedly unwraps active Proxies, returns
`<Revoked Proxy>` for revoked ones, and only then performs type classification,
descriptor lookup, enumeration, or prototype traversal. This mirrors the
metadata access Node's native inspector has while keeping the hook too narrow
to encode presentation policy.

Array rendering obtains `length` from the unwrapped array's own data descriptor
and renders missing descriptors as collapsed `<N empty item(s)>` entries. It
never falls back to indexed property access, so inherited accessors are not
observed.

Error rendering uses data descriptors for `stack`, `name`, and `message`.
Accessor descriptors shadow inherited values but are never called. The normal
JSSE Error prototype stack accessor is therefore skipped, while its data-valued
name and the instance's data-valued message still produce a readable
`[Error: message]` fallback.

Constructor and function names are read from data descriptors. Prototype
metadata traversal unwraps a Proxy before inspecting it. Primitive Symbol
formatting uses the captured intrinsic rather than a mutable prototype method.

## Failure behavior and fallback

The production library harness always uses `--node`, so Proxy metadata is
available on the path covered by the acceptance criterion. The documented
plain-jsse fallback has no native capability to identify a Proxy; reflection
there remains best-effort and catches exotic failures where possible.

## Validation

The cross-engine node-shim self-test covers array-target Proxies, throwing
`getPrototypeOf`/`ownKeys`/`getOwnPropertyDescriptor`/`get` traps, revoked and
nested Proxies, Error accessors, and sparse arrays with inherited accessors.
Trap/getter counters prove the JSSE shim invokes none of them; Node runs the
same structural cases as the reference implementation. Rust host-floor tests
verify the hook is gated, non-enumerable, unwraps without traps, and identifies
revoked Proxies.

Because this is a flag-gated harness feature rather than ECMAScript behavior,
there is no targeted test262 area and no pass-count change expected. The full
test262 suite remains the regression gate.
