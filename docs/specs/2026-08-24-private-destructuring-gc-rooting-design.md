# Private destructuring receiver GC rooting

## Problem

Destructuring assignment evaluates a non-pattern target before it steps an
iterator, reads a source property, or evaluates an initializer. The resulting
Reference Record must remain valid until `PutValue` runs. For a private target
such as `(new C()).#x`, JSSE currently represents that reference as a
`DestructLRef::Private` containing only the receiver's arena ID.

The iterator, source getter, or initializer can run arbitrary user code and
force a collection. Because Rust locals are not traced, the temporary receiver
can be swept and its arena ID reused before `set_private_field` implements
`PrivateSet`. The write then either disappears or targets an unrelated slot and
throws a spurious `TypeError`.

## Considered approaches

1. Root each pre-evaluated private receiver for the remainder of the enclosing
   destructuring evaluation. This directly preserves the spec Reference Record
   and uses JSSE's established temporary-root API. This is the selected
   approach.
2. Validate the receiver in `set_private_field`. This cannot distinguish the
   original receiver from a different live object that reused the same arena
   ID, so no callee-side check can make the stale ID sound.
3. Change arena IDs to generation-stamped handles or stop reusing them. Either
   would be a broad representation change unrelated to destructuring semantics.
4. Root only the private receiver and defer ordinary member targets to a later
   audit. Rejected: an ordinary member target's base, its un-coerced key, and
   each value awaiting `PutValue` are held by the same untraced Rust locals and
   fail observably today (a lost setter call, a key that coerces to
   `[object Object]`, or a spurious `TypeError`), so the same frame roots all of
   them.

## Design

Each of the three target-to-write sequences saves a `gc_root_frame()` and
performs its remaining work inside a closure. Immediately after
`eval_member_lhs_ref` produces a reference, `gc_root_destruct_lref` roots
everything that reference holds: a private or ordinary base, an ordinary
reference's un-coerced key, and a super reference's receiver. Inside the
closure, each value collected by a rest element and each value about to be
written are rooted as well, because `ToPropertyKey` and the write itself run
user code. The roots therefore span every intervening iterator step, iterator
value read, source property access, initializer evaluation, rest collection,
and final write.

After each closure returns, `gc_unroot_frame()` releases that frame's roots.
Centralized cleanup covers normal completion, throws, and yields without
duplicating unroot calls. The enclosing array destructuring evaluator retains
its existing, separately scoped iterator root and iterator-close handling.

## Specification basis

ECMAScript's `IteratorDestructuringAssignmentEvaluation` for both
`AssignmentElement` and `AssignmentRestElement`, and
`KeyedDestructuringAssignmentEvaluation` for object properties, evaluate the
destructuring target into `_lRef_` before operations that can run user code and
later pass the same `_lRef_` to `PutValue`. `PutValue` dispatches a private
reference to `PrivateSet`. The implementation must therefore keep the private
receiver represented by `_lRef_` alive across that entire interval.

## Validation

Add `test262-extra` coverage for:

- an array element private target while `next()` forces collection;
- an array rest private target while repeated `next()` calls force collection;
- an object property private target while its source getter forces collection;
- observable private setters, including an abrupt setter completion, so a
  silently skipped write cannot satisfy the tests;
- a rest element whose iterator mints a fresh object per `next()`, an ordinary
  member target reachable only through the reference, and a member key object
  whose `toString` collects, so that ordinary references are covered too.

Known gap: `destructure_object_assignment` still holds the source object
returned by `ToObject` in an untraced local, so
`({ x: a, [f()]: b } = { x: 1, y: 2 })` loses `b` when `f` collects. Fixing it
requires a single-exit restructure of that function and is tracked as #502.

Run the custom suite, assignment-destructuring and private-class targeted
test262 areas, then the repository's full quality gate and full test262 suite.
