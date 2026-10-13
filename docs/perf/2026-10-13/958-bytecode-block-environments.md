# #958 — VM block environments for block-scoped `let`/`const` and `for (let ...)`

Design: `docs/adr/2026-10-13-0632-bytecode-block-environments.md`.

Measured on a shared 24-core Linux host (load average 17-45 over the runs, so
absolute seconds are noisy; the ratios between interleaved runs are the
signal), release builds, medians of 5 runs **interleaved** between the
binaries, wall-clock including startup. `before` is `main` at `45a0ec8a`; `after` is this branch.
`boa` is boa 0.22 (`~/.cargo/bin/boa`).
Repros are `docs/perf/2026-10-13/repros/`; the JetStream scripts are the
runner's concatenated driver for one iteration of each benchmark.

## Wall time (s)

| benchmark | before | after | after `--no-bytecode` | boa 0.22 | after/before | after/boa |
|---|---:|---:|---:|---:|---:|---:|
| `bench_block_let` (block `const`/`let` in a `for (let ...)`) | 1.65 | 0.42 | 1.55 | 0.24 | 0.25 | 1.79 |
| `bench_for_let_closure` (capturing `for (let ...)`) | 1.03 | 0.30 | 0.99 | 1.25 | 0.29 | 0.24 |
| `bench_switch_let` (case-block `const`/`let`) | 1.11 | 0.37 | 1.62 | 0.26 | 0.33 | 1.45 |
| JetStream UniPoker | 0.72 | 0.68 | 0.80 | 0.60 | 0.94 | 1.14 |
| JetStream raytrace | 2.63 | 2.18 | 3.49 | 2.21 | 0.83 | 0.99 |
| JetStream Babylon | 0.76 | 0.66 | 0.72 | 0.54 | 0.86 | 1.21 |

`bench_for_let_closure` creates a closure every 100th iteration, so most
iterations take the non-capturing path of a loop the analysis must treat as
capturing: it keeps a fresh environment per iteration, which `CopyIterEnv`
does without hashing any name.

## Deterministic counters (perf-counters build, never timed; 1 iteration)

AST work = tree-walker `exec_statement`/`eval_expr` entries.

| benchmark | AST work units before | after | VM ops before | after | fallback execs before | after |
|---|---:|---:|---:|---:|---:|---:|
| UniPoker | 4,689,070 | 956,090 | 9,158,264 | 13,099,334 | 35,971 | 41,587 |
| raytrace | 8,970,957 | 2,810 | 38,977,141 | 48,802,203 | 70,218 | 62 |
| Babylon | 1,941,349 | 51,786 | 7,693,583 | 10,111,863 | 87,508 | 3,977 |

`FALLBACK` rows, before -> after:

| reason | UniPoker | raytrace | Babylon |
|---|---|---|---|
| `block-scoped declaration` | 1 -> 0 | 1 -> 0 | 46 -> 0 |
| `lexical declaration` | 5 -> 0 | 4 -> 0 | 3 -> 0 |

`local_slot_misses` stays 0 in all three: frame locals read from inside block
bodies hit the frame environment's slot, block bindings hit their own
environment's slot.

What remains on the tree-walker in these benchmarks: `for-of` / `for-in`
(UniPoker keeps 956k units: its hot loops are `for (const x of ...)`, #960),
array/object literals and closures (#959), `try`, `lexical binding pattern`
(destructuring `const {a} = o` runs as one `ExecAst` in the block's
environment) and template literals.

## Correctness gates

- `cargo test --lib` (debug): 1027 passed, including the new
  `bytecode::tests` parity/op-shape cases; `cargo test --release` and the
  `perf-counters` integration tests pass.
- Full `scripts/run-test262.py`: 99,911 / 99,911, 0 regressions, default and
  `--no-bytecode`.
- `test262-extra/` (703 files, 1,294 scenarios): 100% on the plain release
  build and `release-checked`, with and without `--no-bytecode`, and under
  `JSSE_GC_STRESS=7` with and without `--no-bytecode`.
- Tree-walker fixes found on the way: `switch` case-block TDZ and
  discriminant/environment rooting (`bytecode-block-environments{,-gc-rooting}.js`),
  and `for (let ...)` head TDZ while the initializers run
  (`for-let-head-tdz-in-initializer.js`), all in both modes.

## Not done here

- Environment elision for blocks no closure/`eval`/hand-off can observe: the
  per-iteration `PushEnv` still acquires (from the pool) and resets an
  environment. `bench_block_let` stays ~1.8x boa on this; a static
  no-capture analysis for blocks would let such blocks keep their bindings in
  the frame environment.
