# #1026: answer BigInt-pair operators in the VM without the generic dispatch layers

A BigInt operand pair took `binary_arm` -> `binary_slow` -> `eval_binary` -> `bigint_binary` -> `with_bigint` x2 ->
`bigint_binary_op`. `binary_slow` also re-rooted and unrooted both operands (a no-op for BigInts, which are never
rooted) and `eval_binary` re-tested for a Number pair that `binary_arm` had already ruled out.

## Change

`binary_arm` (every arithmetic, bitwise, shift and relational op except `==`/`!=`/`===`/`!==`/`>>>`) and
`CmpJumpIfFalse` (relational) try `bigint_pair_op` / `bigint_compare_pop` after the Number-pair test and before the
generic path. Both are `#[inline(never)]` helpers that return `None` with the stack untouched unless the two top
operands are BigInts, so the Number fast path of `run_chunk_inner` gains no code and the non-BigInt generic path pays
one call that returns at once. `bigint_pair_op` takes the operands off the stack without unrooting, runs the existing
`bigint_binary_op` (the same exact arithmetic, shift, sign and RangeError rules) and pushes the result;
`bigint_compare_pop` compares through `with_bigint` and drops both operands. A RangeError (zero divisor, negative
exponent) is thrown through the chunk's ordinary `throw!` route, so handlers see the same stack as for any other
throwing operator. `>>>` on BigInts and every mixed pair keep the generic path and its TypeError and ToPrimitive
ordering.

## What did not pay (measured, not shipped)

- Rewriting a uniquely owned left operand in place (`Arc::get_mut`, `*a += b`, `<<=`, ...): neutral to slightly worse
  (ops 4,976 vs 5,030 Ir/iteration, ed25519 +0.4%). Operands of the hot loops come from `LoadLocal`, so they are shared;
  temporaries such as `a * b` in `(a * b) % m` were not enough to cover the `strong_count` probes.
- `Rc` in place of `Arc` for BigInt (non-atomic refcounts): wall time 4% *slower* on the operator loop.
- Stack-buffer Knuth division and schoolbook multiplication for operands up to 8 limbs, in place of `num_bigint`:
  `num_bigint` has no `u64`-digit constructor, so building the result through `from_slice`/`from_bytes_le` costs
  200-300 Ir per result. Net: mul -340 Ir, rem and div about even, secp256k1 +0.3%, ed25519 -2.4%. Dropped.
- With the layering gone, the BigInt share of the noble profiles is the `num_bigint` kernels themselves (identical in
  Boa 0.22); the rest is VM dispatch (#1030) and by-name scope verification.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

base = `origin/main` at `c1335149`, new = base plus this change. Whole-process `I refs` from cachegrind
(`--cache-sim=no`); the two micro rows are `(N=60,000 - N=30,000) / 30,000` of the issue's loops.

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| ops_vm_add loop (4 BigInt ops), Ir/iteration | 5,354 | 4,912 | -8.3% |
| mulmod256 loop (`(a*b) % m`), Ir/iteration | 3,874 | 3,666 | -5.4% |
| bigint-noble-secp256k1 | 5,211,877,010 | 5,068,857,160 | -2.7% |
| bigint-noble-ed25519 | 6,027,738,752 | 5,883,838,599 | -2.4% |
| fib (micro) | 5,622,210,510 | 5,612,789,073 | -0.2% |
| richards | 8,685,902,241 | 8,650,362,758 | -0.4% |
| delta-blue | 6,427,171,017 | 6,408,213,387 | -0.3% |
| Box2D | 5,990,506,422 | 5,989,284,479 | 0.0% |

The last four rows are the non-BigInt controls: no regression from the extra branch in the generic path.

Wall time, buildbox3, load average 2 (interleaved runs, pinned to one CPU, median; JetStream rows are the runner's
`first_time` of one `--iterations 1` measurement, 7 repeats, Boa 0.22.0 release binary):

| workload | base | new | Boa 0.22 | new vs Boa |
|---|---:|---:|---:|---:|
| ops_vm_add, N=500,000 | 542 ms | 471 ms | 393 ms | 1.20x slower |
| mulmod256, N=500,000 | 350 ms | 328 ms | 315 ms | 1.04x slower |
| bigint-noble-secp256k1 | 1,139 ms | 1,127 ms | 1,184 ms | 4.8% faster |
| bigint-noble-ed25519 | 1,347 ms | 1,318 ms | 1,412 ms | 6.7% faster |

The two noble benchmarks were 1.24x and 1.13x behind Boa in checkpoint 15; #1126 and #1127 closed most of that and
this change adds the margin. The operator loops remain dispatch-bound (`run_chunk_inner` self time is about 45% of
`ops_vm_add`; see #1030).

## Correctness gates

`test262-extra/BigInt-compiled-pair-operators-and-jumps.js` covers every operator over BigInt pairs inside compiled
bodies, relational comparisons feeding `if`/`while` jumps (including a BigInt against a Number in the same body),
RangeErrors raised mid-expression and caught by `try`/`catch`/`finally` in the same function and in a generator,
negative and oversized shift distances, and the TypeErrors for `>>>` and mixed operands.

Full test262 default and `--no-bytecode`: 99,911/99,911 each; `test262-extra/` 1,489/1,489 normal, `--no-bytecode`, and
both under `JSSE_GC_STRESS=7`, and on the `release-checked` build (normal and `--no-bytecode`) together with a 5%
sample (7,025/7,025); `cargo test --lib` 1,145 passed; `scripts/run-custom-tests.py` 19/19; `./scripts/lint.sh` clean.
