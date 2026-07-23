# Node shim `%s` object dispatch

## Goal

Make `scripts/node-shim.js` choose between `String(value)` and the shim's
best-effort `inspect(value, { depth: 0 })` the same way Node does for object
arguments to `util.format("%s", value)`.

This is Node-host compatibility behavior, not an ECMAScript engine change.
ECMAScript remains authoritative for what `String(value)` does after the shim
chooses that path: `ToString` calls `ToPrimitive` with a string hint, which
checks `Symbol.toPrimitive` before ordinary `toString`/`valueOf` coercion.

## Decision

At shim startup, snapshot the capitalized names on `globalThis`, matching
Node's built-in-constructor name set before user bundle code executes.

For each object:

1. Treat a callable own `toString` or `Symbol.toPrimitive` as user-defined and
   use `String(value)`.
2. Treat an object with neither callable hook as built-in and inspect it.
3. Otherwise walk the prototype chain to the object that owns the inherited
   hook.
4. Inspect when that owner has an own function-valued `constructor` whose name
   is in the startup built-in set; use `String(value)` otherwise.

Functions keep the shim's existing handling because issue #250 is limited to
object dispatch. Node's internal proxy unwrapping cannot be reproduced by a
pure-JavaScript shim and is outside this change.

## Alternatives rejected

- Checking only own properties breaks user classes whose `toString` lives on
  the class prototype.
- Comparing against a static list of intrinsic method identities diverges for
  patched built-in prototypes, subclasses/cross-realm-style prototype chains,
  and Node's constructor-name collision behavior.

## Verification

Extend the byte-compared node-shim self-test with arrays, plain objects,
user-defined own and inherited hooks, `Symbol.toPrimitive`, `Date`, `RegExp`,
non-callable hooks, patched built-in prototypes, and a user class named after a
built-in. Run the shim self-test on jsse and Node, the shared shim fixtures, the
relevant ECMAScript coercion test262 directories, and the repository's complete
quality gate.
