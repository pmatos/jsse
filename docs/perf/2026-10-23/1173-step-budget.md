# #1173 step budget: cost of the unset path

With no budget installed the safepoint is the same single `GcPacer.poll` flag
test as before (the budget bit is folded into `poll`); the tick lives in the
existing `#[cold]` slow path. No op, no op operand and no per-statement work
was added, so the `PERF`/`OP` counter blocks are unchanged by construction.

Method: `main` (origin/main at the branch point) and the branch, both built with
the shipped `cargo build --release` profile (fat LTO). The two binaries ran
alternately; the table is the minimum user CPU time of 9-15 runs, on a shared
host (noise about ±1-2%). `valgrind` and `perf` are not installed here, so
instruction counts were not taken; do not compare these numbers across hosts.

| benchmark | mode | main (s) | branch (s) | delta |
|---|---|---|---|---|
| bench_loop | bytecode | 0.121 | 0.117 | -3.2% |
| bench_loop | tree-walker | 0.268 | 0.272 | +1.3% |
| bench_fib | bytecode | 0.199 | 0.195 | -2.0% |
| bench_fib | tree-walker | 0.438 | 0.435 | -0.7% |
| bench_opmix | bytecode | 1.454 | 1.352 | -7.0% |

Delta is within noise in every row. One pitfall found on the way: leaving the
tree-walker's `safepoint_rooted` helper without `#[inline(always)]` cost
+37-45% on `bench_loop` tree-walker in a non-LTO build; it is now inlined.
