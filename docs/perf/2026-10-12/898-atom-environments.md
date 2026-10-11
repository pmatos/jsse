# #898 — atom-keyed environments and verified local slots

Measured on a shared 24-core Linux host at load average 35-50 (no idle host was
available), release builds, medians of 5 runs **interleaved** between the two
binaries, wall-clock including startup. `base` is the parent commit
(`bea020b3`); `after` is this branch. Absolute seconds are not comparable with
buildbox3 numbers in the issue; the ratio between interleaved runs is the
signal. Repros are `docs/perf/2026-10-10/repros/`.

## Micro-benchmarks (default, bytecode)

| benchmark | base (s) | after (s) | after/base |
|---|---:|---:|---:|
| `bench_calls_fib` | 1.65 | 0.77 | 0.47 |
| `bench_calls_leaf` | 1.37 | 0.86 | 0.63 |
| `bench_calls_nested` | 1.22 | 0.60 | 0.49 |
| `bench_calls_closure_call` | 1.16 | 0.69 | 0.59 |
| `bench_numeric_intarith` | 1.19 | 0.48 | 0.40 |
| `bench_numeric_bail_none` | 0.54 | 0.27 | 0.50 |
| `bench_numeric_gaussblur` | 0.95 | 0.50 | 0.53 |

With `--no-bytecode` (tree-walker; touched only through the representation
change) the call benchmarks improve ~20-30% (`fib` 1.40 → 0.97 s, `leaf`
1.93 → 1.52 s) and the local-variable-heavy numeric loops are within noise of
the base (`intarith` and `gaussblur` have 17+ locals, so their scopes use the
hash index; CPU-time min of 5: 1.70 → 1.45 s and 1.37 → 1.51 s). Startup
(`jsse -e 1`) is unchanged within noise (10.3 vs 10.6 ms).

## Deterministic counters (perf-counters build, never timed)

`fib(22)` — 57,313 compiled calls:

- 630,438 VM ops (11.0 per call), 0 `LoadName`/`ResolveName`/`StoreResolvedName`;
  143,282 `LoadLocal` slot hits, 0 misses.
- `atom_intern_calls` 82 and `atom_lookup_calls` 326 for the whole run, all
  from interpreter startup; both stay flat when the call count doubles
  (`tests/perf_counters_env_hashing.rs`). Before this change every call hashed
  `this`, `arguments`, each parameter and each looked-up name.

`bench_numeric_gaussblur` OP histogram: `LoadLocal` 35.9%, `StoreLocal` 12.4%,
`UpdateLocal` 1.1%; `LoadName` 3 executions in the whole run (the module's own
setup), none in the hot loop.

## Acceptance criteria not measurable here

- callgrind `fib(22)` ≤ 4,200 instr/call: `valgrind` is not installed on this
  host. The deterministic counters above (no string hashing, no per-call
  `String` allocation, 11 dispatches per call) are the proxy; run the recipe in
  `docs/perf/2026-10-10/repros/calls/findings.md` on buildbox3 to confirm.
- ratio vs boa ≤ 1.9 / absolute seconds on buildbox3: `boa` is not installed
  here. The after/base ratios above (0.40-0.63) applied to the issue's
  buildbox3 baselines (3.0-3.3x boa) project to ~1.3-2.0x for the call and
  numeric kernels; `bench_calls_leaf` (~2.0x projected) is marginally above the
  1.9 target and is the remaining per-call cost in `call_function_inner_impl`
  (frame/`JsFunction` handling that #899 owns).
