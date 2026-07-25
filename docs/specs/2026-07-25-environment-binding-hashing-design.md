# Environment Binding Hashing

## Context

ECMAScript declarative Environment Records bind identifier names supplied by
the program. Module Environment Records additionally store import names as
indirect bindings. In JSSE, both sets of names are `String` keys in
`Environment`.

PR #389 changed these maps from randomized `std::collections::HashMap` to
deterministic `rustc_hash::FxHashMap` after profiling showed that standard
hashing accounted for about 20.8% of a cold `setupMandreel()` run. The change
improved that workload by about 13%, but it also made the bucket placement of
script-controlled declaration and import names predictable.

JSSE already uses randomized standard hash maps for other collections keyed by
script-controlled strings, following commit `c59e4db`. Deterministic Fx maps
remain appropriate for maps keyed by engine-allocated numeric identifiers.

## Decision

Use randomized `std::collections::HashMap` for `Environment::bindings` and
`Environment::indirect_bindings`.

This applies the existing project threat model consistently: source text is an
input boundary, even though the same program can consume resources in other
ways. Hosts can bound source size and execution time independently, so the
existence of loops does not make avoidable algorithmic-complexity attacks
irrelevant.

The decision knowingly gives back the measured #389 speedup. A future
optimization may replace the standard hasher with a keyed fast hasher, but only
with an explicit HashDoS-resistance argument and benchmarks. The randomly
seeded Fx variant is not selected because `rustc-hash` documents Fx as a
non-DoS-resistant algorithm.

## Implementation

Change only the two Environment map types and their constructors in
`src/interpreter/types.rs`. Keep deterministic `FxHashMap` for `template_cache`
and other maps whose keys are engine-generated IDs. Add a comment at the type
definition so future performance work sees the security boundary.

No ECMAScript-visible semantics change. The representation continues to support
the Declarative Environment Record operations in §9.1.1.1 and
CreateImportBinding in §9.1.1.5.5.

## Verification

- Run the Environment unit tests and module live-binding coverage.
- Run targeted test262 language tests for declarations and modules.
- Run the full test262 suite against the `origin/main` baseline.
- Run formatting, Clippy, release build, and release unit tests.

No custom conformance test is added: the hasher choice is a Rust representation
and threat-model property, not ECMAScript-observable behavior. A test comparing
private random seeds would be probabilistic and would rely on implementation
details that `HashMap` does not promise.
