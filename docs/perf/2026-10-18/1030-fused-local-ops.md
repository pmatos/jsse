# #1030: fused local-pair ops (`LocalLocalBin`, `MoveLocal`, `LocalLocalCmpJump`, `LocalConstCmpJump`)

After #1129 the instruction-level profile of `a = b;` and the `arith` loop is dominated by dispatch and operand-stack traffic: a
statement such as `r1 = (r0 + r1) | 0;` is `Safepoint`, `LoadLocal`, `LoadLocal`, `Add`, `BinConst`, `StoreLocalPop`, and each pushed local
is written to and read back from the operand stack. Fusing the operand loads into the consuming op removes both.

## Change

Four ops, emitted by the compiler when every operand is a function-local name (frame or block slot) or a number literal:

- `LocalLocalBin a b op`: `a op b` for any of the 20 binary ops (`LoadLocal a; LoadLocal b; op`). A Number pair is computed straight from the
  two bindings; anything else (TDZ, non-Number, a hint miss) pushes the two operands in order through `push_local!` and runs the existing
  `binary_arm!` tail, so ToPrimitive order, BigInt, string and error behaviour are the generic code's.
- Compound assignment `x op= y` between two locals compiles to `LocalLocalBin` + `StoreLocalPop`.
- `MoveLocal src dst`: `dst = src` in effect context (`LoadLocal; StoreLocalPop`). The value is copied binding to binding (`read_local` then
  `write_local`, bit copy for plain values); a miss on either side falls back to `push_local!` + the `StoreLocalPop` body.
- `LocalLocalCmpJump a b cmp off` / `LocalConstCmpJump a cmp k off`: the `if`/loop condition forms of `CmpJumpIfFalse`
  (`a < b`, `i < 10`). `LocalConstCmpJump` replaces `LocalBinConst` + `JumpIfFalse` (a pushed Boolean and a pop). The generic tail is
  `cmp_jump_taken!`, the body `CmpJumpIfFalse` already had, now a macro shared by all three.

The jump offset is the last operand of both jump ops, so `patch_jump` is unchanged. Handler ranges, safepoints and the stack-height
accounting (`max_stack` counts the two temporaries of the slow path) need no change. `Instanceof`/`In` never fuse.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

base = `origin/main` `0474e481` (#1129; binary sha256 `16a23f83ab86...`), new = this change (`2a5370738d70...`). cachegrind `--cache-sim=no`; loop
fixtures are Ir per iteration = (60,000 minus 20,000 iterations) / 40,000. Boa is 0.22 (`boa-prof`).

| fixture | base | new | delta | boa |
|---|---:|---:|---:|---:|
| empty `for` loop | 328 | 257 | -21.6% | 442 |
| 32 x `a = b;` (net of loop, per move) | 162 | 127 | -21.6% | 158 |
| `arith` (the issue's `opmix-arith` loop) | 2,041 | 1,912 | -6.3% | 2,185 |
| float arithmetic loop | 1,908 | 1,735 | -9.1% | 2,046 |
| `for (let i...) { let t = i + 1; s = s + t; }` | 1,329 | 1,234 | -7.1% | 603 |

Cumulative with #1129 against the pre-#1129 base: `a = b;` 251 -> 127 net per move, `arith` 2,822 -> 1,912, float loop 2,529 -> 1,735.

Whole-process Ir (JetStream drivers at `--iterations 1`, micro suite); the 20 workloads that move by less than 0.9% in either direction are
omitted:

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| micro:opmix | 42,674,601,227 | 38,860,068,953 | -8.94% |
| gaussian-blur | 21,268,482,903 | 19,860,340,248 | -6.62% |
| micro:fib | 5,286,991,466 | 5,111,979,237 | -3.31% |
| crypto | 3,945,515,919 | 3,843,479,192 | -2.59% |
| splay | 7,750,982,981 | 7,678,916,843 | -0.93% |

`proxy-vue` moves by +-3% between runs of one binary (randomised interner). The bigint-bls12-381 (+0.5%) and earley-boyer (+0.4%) increases are code layout: neither executes a fused op on a hot path. The block-let loop is still 2x Boa; that gap is `PushEnv`/`PopEnv` environment churn, not
local access, and stays with #1030's expanded inventory.

Wall time, the issue's `arith(10000000)` repro, pinned core, 5 alternating runs, load 1.4: base (pre-#1129) 5.05 s, #1129 4.71 s, this
change 4.19 s, boa 4.75 s (jsse 0.88x boa).

Wall time of the three #1030 table rows (`docs/perf/2026-10-10/repros`), pinned core, 9 alternating runs, load 1.3-2.0, medians:

| workload | pre-#1129 | #1129 | this change | boa 0.22 | this change / boa |
|---|---:|---:|---:|---:|---:|
| `bench_numeric_intarith` | 0.878 s | 0.758 s | 0.737 s | 0.904 s | 0.82x |
| `bench_bigint_ops_vm_numadd` | 0.556 s | 0.410 s | 0.373 s | 0.458 s | 0.81x |
| `bench_strings-arrays_ctl_scalar_fn` | 0.951 s | 0.772 s | 0.647 s | 0.747 s | 0.87x |

`run_chunk_inner` self Ir per counted VM op (cachegrind self Ir of the function over the perf-counters `vm_ops`): richards 70.3
(3,069M / 43.7M; 119 in the #1030 audit), gaussian-blur 102.9 (16,212M / 157.5M ops; 116 in the audit, which counted 223M ops before
fusion). `gc_safepoint` is the inline `poll` test since #1058; no out-of-line call on a no-op safepoint.

## Correctness gates

`test262-extra/local-pair-operations-semantics.js` (Number/BigInt/string/Symbol operand kinds, NaN and signed zero, ToPrimitive order, a
`valueOf` that rebinds the right operand, TDZ on either side of an operator, compare-and-branch and move, const and TDZ move targets,
captured and block-scoped locals) passes on the base binary and under Node. Counts and gates are in the PR.
