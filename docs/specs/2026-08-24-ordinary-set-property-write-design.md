# OrdinarySet property-write consolidation design

## Goal

Consolidate ordinary property assignment in the evaluator behind one deep
module so simple, compound, logical, update, destructuring, bytecode, and
`super` writes cannot drift in descriptor, proxy, prototype-chain, receiver,
Array-length, or strict-mode behaviour.

This is a behaviour-preserving refactor except where characterization exposes
an existing contradiction with the ECMAScript algorithms. Such contradictions
are corrected to the pinned specification and covered at the JavaScript
execution seam.

## Specification constraints

`PutValue` (§6.2.5.6) first applies `ToObject` to a property Reference's base,
then calls that object's `[[Set]]` with `GetThisValue(reference)` as the
receiver. For an ordinary member Reference the receiver is the original base,
including when that base is primitive. For a Super Reference the base is the
super base while the receiver is the actual `this` value. A rejected `[[Set]]`
throws only for a strict Reference.

`OrdinarySet` and `OrdinarySetWithOwnDescriptor` (§10.1.9) own the descriptor
lookup, prototype recursion, receiver-own-descriptor checks, property creation,
and inherited accessor invocation. Each prototype is entered through its own
`[[Set]]`, so Proxy and other exotic dispatch remains observable. Array
`length` writes must reach `ArraySetLength`; the assignment expression still
returns the uncoerced right-hand value.

Simple, compound, and logical assignment (§13.15) all finish with `PutValue`
on the captured left Reference. Their read/short-circuit/arithmetic timing may
differ, but their reached write must not.

## Existing seam

`property.rs::set_object_property(base_id, key, value, receiver)` is already
the canonical `[[Set]]` module. Its implementation dispatches module namespace,
Proxy, TypedArray, Array length, accessors, receiver descriptors, and ordinary
prototype recursion and returns the specification's Boolean success result.

`eval.rs::set_object_with_key(base, key, value, strict)` is the existing
ordinary-member `PutValue` adapter. It preserves the original base as receiver,
boxes only the `[[Set]]` holder, converts a false result to a strict-mode
`TypeError`, and is already shared by update, destructuring, and bytecode
writes. The remaining assignment dispatchers and `super_set_property` bypass
this seam and reimplement parts of `[[Set]]`.

## Approaches considered

1. **Route every remaining write into the existing canonical `[[Set]]`
   module.** Selected. Add a receiver-aware evaluator adapter over
   `set_object_property`, let `set_object_with_key` use it, and reduce
   `super_set_property` to the same adapter with a distinct base and receiver.
   Replace the simple/compound and logical member write-back blocks with
   `set_object_with_key`. This maximizes locality without creating a competing
   implementation.
2. **Add a new `ordinary_set` implementation in `eval.rs`.** Rejected because
   `property.rs` already owns the internal-method implementation. A second
   module would merely move, rather than remove, the semantic duplication and
   would leave Reflect/builtin callers on a separate implementation.
3. **Introduce a first-class Reference Record and rewrite all assignment
   evaluation around it.** This could eventually reduce duplicated evaluation
   order code, but expands the change into identifier environments, private
   references, destructuring, and bytecode. It is not required to delete the
   property-write cluster and has a much larger regression surface.

## Design

Add one private receiver-aware `PutValue` adapter in `eval.rs` with the logical
interface `(base object id, key, value, receiver, strict) -> Result<bool,
JsValue>`. It calls `set_object_property` exactly once, returns the success bit
for the one caller that mirrors successful global-object writes, and translates
a false result into the existing strict assignment diagnostic. The interface
keeps strictness outside the object internal method, matching the specification:
`[[Set]]` returns a Boolean; `PutValue` decides whether false throws.

`set_object_with_key` remains the ordinary-member adapter. It captures the
original value as receiver, boxes a primitive base, and delegates to the new
receiver-aware adapter. `super_set_property` delegates with the super base as
holder and actual `this` as receiver, returning the assigned value after a
successful or sloppy rejected write.

The public member arms of `eval_assign` and `eval_logical_assign` retain their
current left-reference capture, read, right-hand evaluation, short-circuit,
and compound-operation order. Only the terminal write-back is replaced. The
dense ordinary-Array indexed-write optimization may remain as a guarded fast
path because it is an implementation of the same successful observable
outcome and explicitly bails for descriptors, prototypes, proxies, holes,
non-extensibility, and non-writable length. All slow paths cross the canonical
seam.

Private fields intentionally remain on `PrivateSet`; they are not ordinary
properties and have brand/accessor semantics unrelated to `OrdinarySet`.
`apply_compound_assign` also remains a value-computation module: its member
callers perform the unified write after it returns.

## Error handling and observable order

No caller probes descriptors before calling `[[Set]]`. Proxy traps therefore
run once and in prototype order, inherited setters receive the Reference's
receiver, and receiver `[[GetOwnProperty]]`/`[[DefineOwnProperty]]` operations
remain inside the canonical implementation. A thrown trap, setter, coercion,
or exotic operation propagates unchanged. A false result is silent for sloppy
References and becomes a `TypeError` for strict References.

When prototype recursion reaches the synthetic writable descriptor and the
receiver is a module namespace exotic object, canonical `OrdinarySet` performs
the receiver's `[[GetOwnProperty]]`-equivalent deferred-evaluation/TDZ check
before returning false. This behaviour previously lived only in the open-coded
super path.

The assigned expression result is always the computed right-hand value, not an
Array length value after coercion. Existing caller-side GC rooting remains in
place around assignment paths that already protect a transient base across
computed key evaluation, getters, right-hand evaluation, and write-back.

## Verification

Characterize behaviour through executable JavaScript, the public seam:

- a Proxy in the prototype chain, including trap receiver and false-result
  strict/sloppy handling;
- inherited non-writable data properties in strict and sloppy code;
- inherited setters and receiver identity;
- `ArraySetLength` effects while assignment returns the uncoerced RHS;
- super-property writes with a distinct holder and receiver;
- a deferred module namespace used as a super-property receiver;
- primitive member References, especially logical assignment, retaining the
  primitive receiver;
- the simple, compound, and logical assignment forms that reach `PutValue`.

Run the custom characterization tests, the relevant test262 assignment,
logical-assignment, update, super, Reflect.set, and Proxy/set areas, then the
repository's complete formatting, lint, release build/test, and full test262
quality gate. The refactor must not change the baseline pass count.
