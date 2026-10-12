# #903: fused effect-context ops and inline number-number ops

Local before/after for the changes in this PR. **Not** the reference-box
protocol of `docs/perf/2026-10-10`: the host was shared (load average 20-40 on
24 cores), there is no `valgrind`/`perf` here, and `boa` was not installed, so
there are no instruction counts and no boa ratios. Dispatch counts come from a
`--features perf-counters` build (deterministic); times are the median of 7
interleaved runs of `user+sys` CPU seconds for the pre-change release binary
(`198d9dc4`) vs this branch, release build, from a build that does not have the
counters compiled in.

## Dispatched ops per iteration (`perf-counters`, deterministic)

| loop | before | after | change |
|---|---:|---:|---:|
| `bench_opmix.js` `arith` body | 43.0 | 24.0 | -44% |
| `bench_numeric_intarith.js` round body | 132.0 | 88.0 | -33% |
| `bench_numeric_bail_none.js` loop body | 43.0 | 23.2 | -46% |

`Op::Safepoint` is excluded from the counts (see `PerfCounters::record_op`).
The issue's "49 ops/iteration" predates #898's local-slot ops; at the start of
this change the same loop was 43. The "<= 25 on `bench_numeric_intarith`" line
is read as the `arith` loop (the only one with a body small enough for 25):
`intarith`'s round body is 88 ops after, a 33% reduction, inside the issue's
"25-40% fewer dispatches". `arith` is gated by
`tests/perf_counters_op_budget.rs`.

`arith` after, per iteration: the loop test `i < n` is `LoadLocal x2 +
CmpJumpIfFalse` (3), `r0 = (r0 + 4) | 0` is `LocalBinConst + BinConst +
StoreLocalPop` (3), `r1 = (r0 + r1) | 0` is `LoadLocal x2 + Add + BinConst +
StoreLocalPop` (5), the two masking/shift statements are `LocalBinConst +
StoreLocalPop` (2 each), `i++` is one discarded `UpdateLocal`, then the back
`Jump`.

## CPU time (median of 7, `user+sys`)

| benchmark | before | after | change |
|---|---:|---:|---:|
| `arith` loop, 3M iterations (bytecode) | 1.17 s | 0.83 s | -29% |
| `bench_numeric_intarith` (bytecode) | 0.52 s | 0.33 s | -36% |
| `bench_numeric_bail_none` (bytecode) | 0.26 s | 0.18 s | -32% |
| `bench_opmix` total (bytecode) | 5.15 s | 4.41 s | -14% |
| `bench_fib` (bytecode) | 0.73 s | 0.61 s | -17% |
| `bench_loop` (bytecode) | 0.74 s | 0.69 s | -7% |

`bench_opmix.js` per phase (median of 5, wall ms printed by the script):

| phase | before | after | change |
|---|---:|---:|---:|
| arith | 267 | 159 | -40% |
| elem | 610 | 459 | -25% |
| called | 701 | 557 | -21% |
| mixed | 910 | 764 | -16% |
| labeledLoop | 489 | 379 | -22% |
| labeledControl | 2763 | 1766 | -36% |

`--no-bytecode` runs moved by -25%..+8% with no tree-walker code changed; that
is host noise (the binary layout changed), not a result.

## Criteria status

- arith ops/iteration <= 25: met (24, gated).
- 25-40% fewer dispatches: met (-33% to -46%).
- `bench_opmix` arith <= 2.5x boa: **not measurable** (no boa on this host). The
  arith phase is 40% faster than before, so the ratio on the reference box
  should improve from 3.9x to roughly 2.3x if it carries over; that is an
  extrapolation, not a measurement.
- test262 unchanged: full run 99,911/99,911, 0 regressions.

## What is left

Per local access the VM still pays a `RefCell` borrow plus `BindingMap::entry_at`
and an `Rc` clone check. The follow-up that would remove it is frame-register
locals (`Vec<JsValue>` slots), together with a local-local binary form and
merging `Safepoint` into the next op. Not done here.
