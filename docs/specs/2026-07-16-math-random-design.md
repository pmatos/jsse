# `Math.random` PRNG design

## Context

ECMAScript `Math.random` must return approximately uniformly distributed
Numbers in `[0, 1)`. Each realm's function must produce a distinct sequence.
JSSE currently returns the constant `0.5`, which satisfies test262's range
check but prevents randomness-dependent library tests from exercising their
real assertions.

## Requirements

- Replace the constant with an approximately uniform pseudo-random sequence.
- Keep independent state on each `Realm`, matching the specification's
  per-realm sequence requirement.
- Preserve the existing `Math.random` function shape and `[0, 1)` result range.
- Do not expose a deterministic seeding interface without a separate contract
  for its CLI/API behavior.
- Remove the lodash harness skips that exist only because of the constant stub.

## Approaches considered

1. Read OS entropy for every call. This is the smallest state model, but it
   makes a frequently called non-cryptographic API perform a system entropy
   request each time.
2. Add a general-purpose RNG crate. This supplies a maintained algorithm, but
   adds a dependency for a small operation that JSSE can implement directly.
3. Seed a small per-realm PRNG once from the existing `getrandom` dependency.
   This keeps calls fast, introduces no dependency, and maps the state directly
   to the specification's realm boundary.

JSSE will use approach 3.

## Design

`Realm` owns a 64-bit SplitMix64 state. A process seed is initialized once from
OS entropy, and each realm combines it with a process-local monotonic realm
counter so distinct realms start from distinct states. A `math_random` method
advances and mixes the state, discards the low 11 bits, and divides the
remaining 53-bit integer by `2^53`. The result is always a positive-sign Number
greater than or equal to zero and strictly less than one.

If the OS entropy source is unavailable, process seeding falls back to a fixed
non-zero base while the realm counter still gives each realm a distinct initial
state. `Math.random` is not a cryptographic API, so entropy failure must not add
a new observable exception to the built-in.

The native `Math.random` function calls the current function realm's
`math_random` method. JSSE already switches `current_realm_id` to a native
function's creation realm during calls, so cross-realm calls update the correct
state.

## Validation

- Add a `test262-extra` regression that samples successive calls, verifies the
  range, and requires more than one observed result. This fails on the `0.5`
  stub.
- Run the official `test262/test/built-ins/Math/random/` tests.
- Remove the three lodash random/shuffle skip fragments and run the lodash
  library harness to exercise its distribution-dependent assertions.
- Run the repository quality gate and full test262 suite to catch regressions.
