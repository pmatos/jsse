# #959 — native array/object/regexp literals and closure creation in the VM

Design: `docs/adr/2026-10-13-0755-bytecode-literals-and-closures.md`.

Measured on a shared 24-core Linux host (load average 16-26 during the runs,
so absolute seconds are noisy; the ratios between interleaved runs are the
signal), release builds, medians of 5 runs **interleaved** between the
binaries (9 for the two JetStream rows marked `*`), wall-clock including
startup. `before` is `main` at `a1d98fc7`; `after` is this branch. `boa` is
boa 0.22 (`~/.cargo/bin/boa`). Repros are `docs/perf/2026-10-13/repros/` and
`benchmarks/scripts/`; the JetStream scripts are the runner's concatenated
driver for one iteration of each benchmark.

## Wall time (s)

| benchmark | before | after | after `--no-bytecode` | boa 0.22 | after/before | after/boa |
|---|---:|---:|---:|---:|---:|---:|
| `bench_array_literal` (non-constant `[i, i+1, ...]` per call) | 1.22 | 0.92 | 1.67 | 0.57 | 0.75 | 1.61 |
| `bench_array_const_literal` (16-number table per call) | 1.32 | 0.87 | 2.17 | 1.07 | 0.66 | 0.81 |
| `bench_object_literal` (`{x, y, z, name}` per call) | 1.57 | 1.02 | 2.27 | 1.02 | 0.65 | 1.00 |
| `bench_closure_literal` (function + arrow per iteration) | 1.62 | 1.52 | 1.82 | 0.46 | 0.94 | 3.30 |
| `bench_literal_mix` (splay-shaped nodes, regexp, method closure) | 0.67 | 0.42 | 0.52 | 0.41 | 0.63 | 1.02 |
| `bench_closure_create` | 0.67 | 0.67 | 0.87 | 0.31 | 1.00 | 2.16 |
| `bench_closures` | 1.62 | 1.62 | 1.57 | 1.07 | 1.00 | 1.51 |
| script-level `var o = {x:42,y:1}; for (...) sum += o.x + o.y` (3M) | 2.47 | 0.92 | 2.42 | - | 0.37 | - |
| JetStream splay | 3.23 | 2.82 | - | 2.12 | 0.87 | 1.33 |
| JetStream raytrace | 2.42 | 2.22 | - | 2.47 | 0.92 | 0.90 |
| JetStream typescript-octane | 18.46 | 16.97 | - | 11.09 | 0.92 | 1.53 |
| JetStream pdfjs `*` | 1.77 | 1.67 | - | 1.97 | 0.94 | 0.85 |
| JetStream Box2D `*` | 1.02 | 0.97 | - | 0.97 | 0.95 | 1.00 |
| JetStream gbemu | 3.27 | 3.32 | - | 3.42 | 1.02 | 0.97 |
| JetStream UniPoker | 0.72 | 0.67 | - | 0.77 | 0.93 | 0.87 |
| JetStream Babylon | 0.67 | 0.67 | - | 0.56 | 1.00 | 1.20 |
| JetStream cdjs | 3.92 | 4.07 | - | 5.78 | 1.04 | 0.70 |
| JetStream earley-boyer | 1.07 | 1.07 | - | 1.02 | 1.00 | 1.05 |

Per-literal microbenchmarks (1M evaluations in a loop in a function, median of
5-7 rounds): `f = [i]` 0.51 -> 0.41 s (boa 0.26), `f = {a: i}` 0.52 -> 0.46
(0.26), `f = () => 0` 0.67 -> 0.36 (0.21), `f = function () {}` 1.47 -> 1.27
(0.26).

Closure creation got faster by the dispatch and name-inference share only: a
constructable `function` expression still allocates two function-object
structures per evaluation (`create_function_with`: the function and its
`.prototype`), which is now the whole cost and not addressed here (#901 notes
lazily materialised `.prototype` as the follow-up). `bench_closure_create` /
`bench_closures` are therefore unchanged.

The wall-time effect on the one-iteration JetStream drivers is small because
setup dominates them; the counters below are the deterministic evidence.

## Deterministic counters (perf-counters build, never timed; 1 iteration)

AST work = tree-walker `exec_statement`/`eval_expr` entries. "closure/literal
rows" counts `FALLBACK` rows for `expression:Array`, `expression:Object`,
`expression:Function`, `expression:ArrowFunction`, `anonymous function
assignment`, `anonymous function initializer`, `literal` and `call callee`
(before) and the same plus `array literal: *` / `object literal: *` (after).

| benchmark | AST work units before | after | VM ops before | after | fallback execs before | after | closure/literal rows |
|---|---:|---:|---:|---:|---:|---:|---|
| splay | 11,364,174 | 167 | 20,850,784 | 26,130,787 | 756,004 | 0 | 6 -> 0 |
| cdjs | 566,565 | 227 | 61,969,398 | 62,383,876 | 97,860 | 0 | 34 -> 0 |
| ai-astar | 275,347 | 79 | 40,705,077 | 40,705,595 | 24,751 | 0 | 27 -> 0 |
| pdfjs | 1,365,549 | 698,984 | 43,117,906 | 43,556,499 | 93,598 | 337 | 528 -> 0 |
| typescript-octane | 1,517,449 | 1,324,912 | 290,197,131 | 290,250,341 | 143,801 | 5,360 | 1366 -> 0 |
| stanford-crypto-sha256 | 72,461 | 36,408 | 34,435,710 | 34,462,725 | 18,016 | 4,501 | 14 -> 0 |
| Babylon | 51,786 | 46,251 | 10,111,863 | 10,114,072 | 3,977 | 627 | 61 -> 0 |
| earley-boyer | 161,175 | 154,928 | 16,635,405 | 16,635,602 | 3,075 | 2 | 32 -> 0 |
| UniPoker | 956,090 | 940,029 | 13,099,334 | 13,099,337 | 41,587 | 25,569 | 13 -> 0 |
| Box2D | 5,560 | 4,447 | 36,078,939 | 36,079,038 | 1,279 | 265 | 861 -> 0 |
| gbemu | 5,695,523 | 5,695,038 | 142,186,563 | 142,186,660 | 4,335 | 3,993 | 161 -> 0 |
| raytrace | 2,810 | 1,275 | 48,802,203 | 48,803,706 | 62 | 15 | 6 -> 1 |
| crypto / richards / delta-blue / navier-stokes | 15,002 / 281 / 473 / 124 | 14,995 / 274 / 466 / 93 | - | - | 4 / 4 / 4 / 27 | 0 | 4 / 4 / 4 / 20 -> 0 |

What is left in `ast_units_in_fallbacks` is not a literal or a closure:
pdfjs `for-in`/`try`/`delete`/nested tail calls, gbemu `try` (its remaining
`fallback-only body` rows are `try` bodies), UniPoker/Babylon `for-of`,
Babylon template literals and destructuring declarations. The single
remaining literal row is raytrace's `object literal: spread` (15 executions
in the whole run).

## Correctness gates

- Full `scripts/run-test262.py`: 99,911 / 99,911, 0 regressions, default and
  `--no-bytecode`.
- `test262-extra/` (706 files, 1,297 scenarios): 100% on the plain release
  build and `release-checked`, with and without `--no-bytecode`, and under
  `JSSE_GC_STRESS=7` with and without `--no-bytecode`; the three new files
  also pass at `JSSE_GC_STRESS=1` on `release-checked`.
- `cargo test --lib` (debug): 1,042 passed, including the new
  `bytecode::tests` shape/parity cases; `scripts/lint.sh` clean; custom tests
  18 / 18.
- Behaviour changes found on the way: a Script-goal body made of literals,
  closures and immediately invoked functions now compiles instead of
  bailing, so two `bytecode::tests` chunk counts, two `interpreter::tests`
  inline-cache tests (which assert tree-walker IC counters; they now run
  with `--no-bytecode`) and the `compile_ok` expectation of
  `perf_counters_report_paths` (2 -> 3: `make` now compiles) were adjusted to
  say what they now measure.

## Not done here

- Object literals with computed keys or spread, array literals with spread
  or (non-constant) elisions and class expressions still hand the literal to
  the tree-walker (`array literal: spread|elision`, `object literal:
  computed key|spread`, `expression:Class`).
- Function-object creation cost (two allocations per constructable closure).
