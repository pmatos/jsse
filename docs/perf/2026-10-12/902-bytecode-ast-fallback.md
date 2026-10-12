# #902 — per-node tree-walker fallback and native switch/throw/typeof/update/…

Measured on a shared 24-core Linux host at load average 25-35 (no idle host was
available), release builds, medians of 5 runs **interleaved** between the two
binaries, wall-clock including startup. `base` is the parent commit
(`b7fd4470`); `after` is this branch. Absolute seconds are not comparable with
buildbox3 numbers in the issue; the ratios between interleaved runs are the
signal. Repros are `docs/perf/2026-10-10/repros/numeric/`.

## `bench_numeric_bail_*` (default, bytecode)

| repro | base (s) | after (s) | after/base | after / none | `--no-bytecode` (s) |
|---|---:|---:|---:|---:|---:|
| `bench_numeric_bail_none` | 0.28 | 0.27 | 0.98 | 1.00 | 1.13 |
| `bench_numeric_bail_switch` | 1.08 | 0.27 | 0.25 | 1.01 | 1.10 |
| `bench_numeric_bail_throw` | 1.30 | 0.28 | 0.21 | 1.03 | 1.06 |
| `bench_numeric_bail_typeof` | 1.03 | 0.27 | 0.26 | 1.00 | 1.08 |
| `bench_numeric_bail_memberupdate` | 1.18 | 0.27 | 0.23 | 0.99 | 1.04 |
| `bench_numeric_bail_letconst` | 1.03 | 0.27 | 0.26 | 1.00 | 1.01 |
| `bench_numeric_bail_arrayliteral` | 1.10 | 0.27 | 0.25 | 1.00 | 1.06 |

Every variant is within 1.03x of the control (criterion: 1.1x); before, each
variant was 3.7-4.6x the control because one never-executed statement moved the
whole function to the tree-walker.

## JetStream (jsse score, higher is better; N=3, host unstable)

| benchmark | base | after |
|---|---:|---:|
| stanford-crypto-aes | 3.1 | 6.2 |
| richards | 2.0 | 4.2 |
| hash-map | 1.6 | 2.9 |
| earley-boyer | 3.5 | 5.7 |
| Box2D | 4.8 | 6.7 |
| crypto | 8.4 | 12.5 |
| gbemu | 1.2 | 1.9 |
| UniPoker | 3.9 | 5.3 |
| raytrace | 1.6 | 1.9 |
| navier-stokes | 12.0 | 14.9 |
| delta-blue | 6.7 | 6.6 |
| stanford-crypto-sha256 | 10.6 | 10.7 |
| **geomean (12)** | **3.8** | **5.5** |

## Deterministic counters (perf-counters build, never timed; 1 iteration)

AST work = tree-walker `exec_statement`/`eval_expr` entries; VM ops from the
same run.

| benchmark | VM ops | AST units base | AST units after | fallback execs | note |
|---|---:|---:|---:|---:|---|
| stanford-crypto-aes | 67.6M | 61M (issue) | 0.44M (0.65%) | 18.9k | `throw` guard compiled; acceptance < 1% |
| richards | 47.7M | 6.6M (issue) | 682 | 9 | member `++` native |
| crypto | 54.6M | 1.9M (issue) | 15k | 9 | `montReduce` member `++/--` native |
| Box2D | 40.1M | 2.9M (issue) | 5.6k | 1.3k | `switch` native (82% of base AST work) |
| gbemu | 171.7M | 77M (issue) | 5.7M (3.3%) | 4.3k | `switch` native; remainder is `try` around a one-time `base64_decode` loop and the harness |
| UniPoker | 12.2M | — | 4.7M (28%) | 36k | `for-of` and block-scoped `const` still run as tree-walker nodes |
| Babylon | 8.4M | — | 1.9M (19%) | 87k | block-scoped lexicals, `for-of`, array literals |

`BAIL` rows no longer name `switch`, `throw`, `typeof`, `update target`,
`lexical declaration`, `literal`, `binary op` (instanceof/in) or
`expression:*` for function bodies in any benchmark above. What remains in
`BAIL`: the benchmark's own `<script body>` (Script goal still bails whole-body),
`fallback-only body` (a body whose only content would be hand-offs), and
`construct needs tree-walked frame` (class constructors calling `super(...)`,
`with`, direct `eval`).

## What still runs on the tree-walker

Read `FALLBACK` and `PERF ast_units_in_fallbacks`, not `BAIL`, to find hot
tree-walker work. In these benchmarks the remaining cost is: blocks and loop
bodies that declare `let`/`const` (the whole block is one `ExecAst` unit —
no VM block environments yet), `for-of`/`for-in`, `try`, array/object
literals and closures. These are follow-ups; see the PR description.

## Method notes

- Timings: plain `cargo build --release`, `jsse` default mode, interleaved
  base/after/`--no-bytecode` per repro, 5 rounds, median.
- JetStream: `scripts/run-jetstream.py --iterations 1 --repeats 3
  --no-idle-gate`, same host, sequential per engine (not interleaved), so the
  absolute scores carry the load noise the runner flags as UNSTABLE.
- Counters: `cargo build --release --features perf-counters --target-dir
  target/perf-counters`, JetStream driver output captured from stderr.
- Not measurable here: `boa` and `valgrind` are not installed, so the issue's
  "x boa" ratios and callgrind figures are not reproduced; the after/base
  ratios above applied to the issue's buildbox3 baselines project the
  numeric-bail family to ~1.0x the control (3.3x boa).
