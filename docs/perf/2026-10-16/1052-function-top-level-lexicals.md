# #1052: declare function-top-level lexical names from precomputed atoms

A compiled function body with a top-level `let`/`const`/`class` ran the tree-walker's
`instantiate_body_declarations` on every call: it re-walked the statements, cloned each bound name into
`Vec<String>`s and re-hashed every name through the randomised interner (`Atom::intern`).

Two stages, both in this change:

1. `HoistAnalysis.lexical_names: Vec<(Atom, BindingKind)>`, interned once per Body in `collect_names`;
   `instantiate_body_declarations` declares them with `Environment::declare_atom` (this also serves the
   `--no-bytecode` cached function-body path).
2. `Chunk.top_level_lexicals: Option<Vec<(u16, bool)>>`, set by `compile_function_goal` when declaration
   instantiation is nothing but those lexicals (no function declaration anywhere, hence no Annex B name, and
   every `var` name the tree-walker would hoist is already in the chunk's `var_names`). `dispatch_body` then runs
   `declare_var_prologue` + `declare_top_level_lexicals` and skips `instantiate_body_declarations` and the
   `HoistCache` lookup entirely. Any other body keeps the stage-1 path.

Binding order in the environment is unchanged (params, `arguments`, var prologue, lexicals in source order).

Measured on buildbox3 (shipped profile: fat LTO, cgu=1, nightly, `--locked`), cachegrind `--cache-sim=no` total
`I refs`, base = `main` at `243d142b`, new = the same tree plus this change. The JetStream drivers are the
audit-workspace ones (`bench.py driver`); the repro is the issue's, 100,000 calls.

| benchmark | base Ir | stage 1 | stage 1+2 | delta (final) | criterion |
|---|---:|---:|---:|---:|---|
| bigint-bigdenary | 6,987.6M | 6,494.4M | 6,379.5M | -8.70% | >= 8% |
| bigint-noble-secp256k1 | 7,752.2M | 7,293.7M | 7,245.8M | -6.53% | >= 6% |
| bigint-noble-ed25519 | 8,800.0M | 8,293.9M | 8,233.6M | -6.44% | >= 6% |
| bigint-noble-bls12-381 | 84,550.6M | 78,516.1M | 77,869.6M | -7.90% | >= 5% |
| Babylon | 3,968.7M | 3,776.5M | 3,753.9M | -5.40% | >= 4.5% |
| Air | 2,960.1M | 2,880.0M | 2,869.8M | -3.05% | >= 2.5% |
| proxy-vue (median of 5) | 1,670.7M | | 1,656.4M | -0.9% | noise floor |
| fib | 8,477.0M | | 8,485.1M | +0.10% | no regression |
| richards | 15,142.5M | | 15,145.9M | +0.02% | no regression |
| delta-blue | 10,106.3M | | 10,111.9M | +0.06% | no regression |

proxy-vue's total Ir is not run-to-run stable (base 1,661M-1,681M over four runs, new 1,635M-1,712M), so only the
medians are reported.

## Reproducer (`MODE` helper bodies, Ir per call against `plain`)

| helper body | base | stage 1+2 |
|---|---:|---:|
| `var r = 0; return a + b;` (var1) | +453 | +468 |
| `const r = 0; const s = 1; ...` (const2) | +2,633 | +830 |
| `let r = 0; let s = 1; ...` (let2) | +2,633 | +815 |
| `const r = a + b; return r;` (const1read) | +1,654 | +556 |
| `const { x, y } = o; ...` (destruct, tree-walker hand-off remains) | +4,509 | +2,898 |

`plain` is unchanged (492.88M vs 493.18M); `var1` +0.3%. `atom_intern_calls` is flat across call counts
(`tests/perf_counters_env_hashing.rs::function_top_level_lexicals_intern_nothing_per_call`).

## Not done

A class declaration still interns its binding name three times per evaluation (the class scope's own
declaration, then `declare` and `initialize_binding` in `Statement::ClassDeclaration`); that is class
evaluation on the tree-walker, not declaration instantiation. The destructuring row's remaining cost is the
tree-walker hand-off for the pattern itself.
