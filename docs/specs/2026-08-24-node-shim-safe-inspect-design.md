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

The shim captures this function with its other host primordials, then
immediately deletes the configurable global binding so subsequently evaluated
library code cannot observe or call the Proxy-target seam. The closure retains
the only reference needed by the formatter. At the start of object/function
rendering it repeatedly unwraps active Proxies, returns
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
`[Error: message]` fallback. A raw `stack` value is tested for truthiness before
safe primitive stringification, preserving the fallback for `0`, `-0`, `0n`,
`false`, and `NaN`. `null` remains a safely stringifiable primitive for `name`
and `message`, despite JavaScript's historical `typeof null === "object"` result.
Error presentation is gated by the same trap-free prototype classification as
the other built-ins: reparenting a genuine Error to an ordinary object selects
generic rendering, while a direct null prototype receives the explicit
`[Error: null prototype]` marker.

Constructor and function names are read from data descriptors. Prototype
metadata traversal unwraps a Proxy at every hop before inspecting it. Primitive
Symbol formatting uses the captured intrinsic rather than a mutable prototype
method.

Built-in presentation combines an internal-slot probe with a trap-free
prototype-chain classifier. Reparenting a genuine Date, RegExp, or boxed
primitive to an ordinary object selects the generic descriptor path; a direct
null prototype receives Node's family-specific marker. Slot-bearing prototype
objects retain cross-realm recognition where the built-in exposes such a slot.
RegExps are formatted from the captured `source` and individual flag accessors;
using the captured `RegExp.prototype.toString` would still perform ordinary
`source` and `flags` gets and could invoke replacement accessors. Normal boxed
BigInt/Symbol chains retain their existing `@@hasInstance`-sensitive shape after
the trap-free presentation classification succeeds.

`util.format`'s `%s` classification walk (`hasBuiltInToString`) decides whether
a value carries a user-defined coercion hook or routes to `inspect`. It unwraps
the value and then each prototype it visits, so a Proxy anywhere in the chain
runs no `getPrototypeOf`/`getOwnPropertyDescriptor` trap. This is a deliberate
divergence: Node *would* fire that trap, so a `--node` cross-check difference on
a prototype-chain Proxy is expected rather than a regression. The initial
`toString`/`@@toPrimitive` presence checks on that path are still ordinary
property reads, so a proxied prototype can observe those two gets; closing that
remainder requires changing the classification to descriptor lookups, which
would alter which values Node-compatibly route to `inspect`, and is left out.

## Failure behavior and fallback

The production library harness always uses `--node`, so Proxy metadata is
available on the path covered by the acceptance criterion. The documented
plain-jsse fallback has no native capability to identify a Proxy; reflection
there remains best-effort and catches exotic failures where possible.

## Deliberate divergences from Node

Node reaches a few of its outputs by invoking exactly the user code this design
exists to avoid, so refusing to call it necessarily changes the rendering. These
are accepted, not bugs:

- A function whose `name` is an accessor renders `[Function (anonymous)]`;
  Node reports `[Function: Dyn]` because it calls the getter. `functionName`
  reads an own *data* descriptor only.
- An Error whose `message` has been replaced by an object renders `[Error]`,
  dropping the message. Node invokes the object's `toString` and folds the
  result into the rendering (`Error: [object Object]`, followed by the stack);
  jsse on `main` produced `[Error: [object Object]]`. A `null` or falsy
  `message` still renders as Node does (`[Error: null]`).
- The `%s` path only classifies trap-free. Once classification answers
  "user-defined", `convS` coerces the original value, so a `toString`/
  `@@toPrimitive` hook — including a Proxy `get` trap — does run, as it does on
  Node.

The no-`--node` fallback cannot read `[[ProxyTarget]]`, so reflection there
still dispatches handler traps *and* cannot see through a Proxy to the internal
slot behind it. Its renderings therefore diverge from the `--node` path
wherever a Proxy is involved: a Proxy-wrapped built-in falls back to the
generic object shape (`Date {}`, `RegExp {}`, `Number {}` rather than the ISO
string, `/x/g`, `[Number: 5]`), and a revoked Proxy throws out of `inspect`
instead of rendering `<Revoked Proxy>`. Non-Proxy values render identically on
both paths.

## Validation

The cross-engine node-shim self-test covers array-target Proxies, throwing
`getPrototypeOf`/`ownKeys`/`getOwnPropertyDescriptor`/`get` traps, revoked and
nested Proxies, a Proxy in the prototype chain rather than as the inspected
value, Error accessors, sparse arrays with inherited accessors, reparented
RegExps with throwing `source`/`flags` getters, and replacement accessors on
`RegExp.prototype`. Ordinary- and null-prototype cases cover Error, Date,
RegExp, and every boxed primitive; a proxied replacement prototype verifies the
classifier itself dispatches no handler trap.
Trap/getter counters prove the JSSE shim invokes none of them **for those
cases**; the counters are evidence for the enumerated shapes, not a blanket
proof, and the `%s` `toString`/`@@toPrimitive` presence reads noted above remain
a known uncovered remainder. The nested-Proxy rendering assertion runs only on
JSSE because Node 24 and Node 26 differ in how many Proxy layers their native
inspectors unwrap; the Node branch emits the same successful assertion line so
the cross-check count and stdout stay identical. Node runs the other structural
cases as the reference implementation. Rust host-floor tests
verify the hook is gated, non-enumerable, unwraps without traps, and identifies
revoked Proxies. The JavaScript self-test additionally verifies the captured
hook is no longer globally reachable once the shim has initialized.

String escaping continues to use captured `String.prototype.split` with
primitive string separators. ECMAScript only consults `%Symbol.split%` when the
separator is an Object, and JSSE implements the same object-only guard, so a
library assignment to `String.prototype[Symbol.split]` cannot run on this path.

Because this is a flag-gated harness feature rather than ECMAScript behavior,
there is no targeted test262 area and no pass-count change expected. The full
test262 suite remains the regression gate.
