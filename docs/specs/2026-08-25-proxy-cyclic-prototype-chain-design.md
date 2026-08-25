# Proxy-mediated cyclic prototype-chain recovery

## Goal

Make prototype-chain operations on a cycle routed through a Proxy fail with a
catchable `RangeError: Maximum call stack size exceeded` instead of exhausting
the Rust thread stack or looping forever. Preserve the specification-required
ability to construct such a cycle.

## Specification constraints

`OrdinarySetPrototypeOf` (ECMA-262 §10.1.2.1) deliberately stops its cycle
check when it reaches an object whose `[[GetPrototypeOf]]` is not the ordinary
internal method. The accompanying note guarantees acyclicity only for chains
made entirely of ordinary objects. The test262
`Object/prototype/__proto__/set-cycle-shadowed.js` test requires a Proxy to
remain an escape hatch, so rejecting the assignment is not a valid fix.

`OrdinaryGet`, `OrdinarySetWithOwnDescriptor`, and `OrdinaryHasProperty`
(§§10.1.8–10.1.9) forward to the next object's corresponding internal method
when an own property is absent. Proxy `[[Get]]`, `[[Set]]`, and
`[[HasProperty]]` (§10.5) forward to their target when the handler method is
missing. In combination, those rules can repeatedly enter the same cycle.
The specification does not prescribe a particular resource limit, but a host
resource failure must remain catchable; Node reports a stack-exhaustion
`RangeError` for these operations.

## Approaches considered

1. Reject a prototype assignment when a Proxy target closes the cycle. This
   contradicts `OrdinarySetPrototypeOf` and the existing test262 requirement.
2. Track visited object identities and fail on repetition. This detects the
   small repro immediately, but is observably wrong when a Proxy handler getter
   mutates the chain before returning `undefined`, or when a `getPrototypeOf`
   trap returns a repeated object temporarily and later terminates.
3. Bound Proxy forwarding within each prototype-chain operation. Selected.
   Ordinary-only chains keep their existing behaviour and depth capacity. A
   chain can cycle only through a non-ordinary `[[GetPrototypeOf]]`, and every
   JSSE cycle in scope therefore repeatedly crosses a Proxy forwarding seam.
   A generous soft bound permits dynamic chains to terminate while preventing
   native recursion or an unbounded iterative walk.

Rewriting every internal method and consumer as one iterative state machine
would also avoid native recursion, but it would be a much larger semantic
change and would still need a resource policy for dynamic Proxy traps.

## Design

Define one interpreter-wide Proxy-chain forwarding limit. The canonical
`[[Get]]`, `[[Set]]`, and `[[HasProperty]]` entry points start a forwarding
counter at zero. Their private recursive implementations preserve the counter
across ordinary prototype edges and increment it only when a Proxy has no
applicable trap and directly forwards to its target. Crossing the limit creates
the same `RangeError` and message as the existing call/evaluation stack guards.

Calls into user code retain normal operation boundaries. A getter, setter, or
Proxy trap that starts a new MOP operation receives a fresh counter; ordinary
handler lookup also remains independently guarded. This prevents a caught
inner exhaustion from poisoning later operations and permits handlers to
mutate a chain before a direct forwarding step continues.

Proxy-aware iterative prototype consumers apply the same policy. Ordinary
`instanceof` counts Proxy objects encountered while walking
`[[GetPrototypeOf]]`; Proxy-started `for-in` enumeration does likewise. These
paths currently loop without growing the Rust stack, so the shared bound turns
their unrecoverable hang into the same catchable resource error.

Own properties continue to short-circuit before any forwarding. Metadata-only
helpers that intentionally inspect stored descriptors/prototype slots remain
unchanged.

## Error handling and verification

The thrown value is a realm-appropriate `RangeError` with message
`Maximum call stack size exceeded`. Regression coverage constructs the
spec-permitted `ordinary -> ordinary -> Proxy(target ordinary)` cycle and
checks missing-property `[[Get]]`, `[[Set]]`, and `[[HasProperty]]`, plus the
two iterative consumers. It also verifies own properties still short-circuit,
the error is catchable, and an ordinary operation succeeds after recovery.

Targeted test262 validation covers Object prototype mutation and Proxy get,
set, has, and prototype traps. The complete custom and test262 suites remain
the regression gates; the existing pass count is not expected to change.
