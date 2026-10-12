# Atom-keyed function environments and verified local slots

Status: accepted (#898)

## Context

A user function call spent about 45% of its instructions in the name-keyed
`Environment.bindings: HashMap<String, Binding>`: SipHash on every lookup, a
`String` allocation per inserted name (`this`, `arguments`, each parameter),
and a `clear()` that freed every key. The bytecode VM resolved every
identifier by name. #389 showed that swapping the hasher is not acceptable
(#393 reverted it; see `docs/specs/2026-07-25-environment-binding-hashing-design.md`):
identifier names are script-controlled and must not be hashed by a
predictable function.

## Decision

1. **Atoms.** Identifiers are interned to an `Atom` (`interpreter/atom.rs`).
   The interner is a thread-local, randomised (`RandomState`) table of `Weak`
   references, purged when it doubles, so it stays bounded under `eval` /
   `new Function` churn. An atom stores its hash, computed once by the
   randomised hasher. The interner is therefore the only place script-controlled
   bytes are hashed, which keeps the #393 decision intact: nothing downstream
   hashes names with an unkeyed function. Atoms compare by pointer; index maps
   hash an atom by writing its stored `u64` through an identity hasher.
2. **`BindingMap`.** `Environment.bindings` is an insertion-ordered
   `Vec<(Atom, Binding)>`. Scopes of up to 16 entries are probed by a linear
   scan (pointer equality for atoms, length + `memcmp` for `&str`); larger
   scopes (the global environment) also keep an identity-hashed atom index. A
   `&str` probe on an indexed scope does one `Atom::lookup`; a miss means the
   name is bound nowhere in the map. Removal is `swap_remove` with index
   fix-up, so stripping thousands of builtins stays linear.
3. **`this` is a field.** `Environment.this_binding` replaces the `"this"`
   entry (`Environment::{set_this, declare_this_uninit, get_this}`), traced
   explicitly by the GC and cleared by `reset_function_scope`.
4. **Conditional `arguments`.** The binding is declared only when
   `func_uses_arguments` is true (it already counts identifier references,
   direct `eval`, `var arguments` and arrows). Legacy `f.arguments` is served
   by the deferred call-frame arguments, never by the binding.
5. **Atoms in bytecode.** `Chunk.names` holds atoms, so `LoadName`,
   `ResolveName`, `StoreResolvedName`, `UpdateName`, the var prologue and the
   scope-chain walk (`resolve_identifier` and friends, generic over
   `BindingName`) probe by pointer or precomputed hash.
6. **Local slot ops.** Function bodies are compiled in two passes: the first
   finds the names the var prologue declares, the second emits
   `LoadLocal`/`StoreLocal`/`UpdateLocal` for the frame's own names
   (`arguments`, simple parameters, vars). The slot operand is a *hint* into
   the frame's binding vector, verified on every execution by atom identity
   together with the same guards `StoreResolvedName` used (initialized, `Var`
   kind, no `with`/indirect/global-object scope). A miss falls back to by-name
   resolution, so direct `eval` injection, removal, a frame laid out in another
   order, or a Body shared by functions with different parameters degrade to
   the slower path rather than to a wrong answer. Assignment to a local no
   longer resolves the reference before evaluating the right-hand side; that is
   unobservable because a compiled body contains no `with`, closure or direct
   `eval`, so nothing can change what a frame-local name resolves to.

## Consequences

- No identifier text is hashed or allocated on the call path of a compiled
  function (`tests/perf_counters_env_hashing.rs` gates `atom_intern_calls` /
  `atom_lookup_calls` staying flat as the call count grows).
- Tree-walker lookups keep the `&str` API: a linear scan on small scopes, one
  interner probe on indexed ones, no worse than the old hash.
- Atoms are held alive only by environments, chunks and layouts, so the
  interner's memory follows live code, not every name ever seen.
- Not done here: `(hops, slot)` resolution for captured variables, slot ops in
  the tree-walker and in generator/async state machines, and AST identifiers as
  atoms.
