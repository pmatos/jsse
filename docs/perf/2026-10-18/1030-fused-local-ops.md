# #1030: fused local-pair ops (`LocalLocalBin`, `MoveLocal`, `LocalLocalCmpJump`, `LocalConstCmpJump`)

After #1129 the instruction-level profile of `a = b;` and the `arith` loop is dominated by dispatch and operand-stack traffic: a
statement such as `r1 = (r0 + r1) | 0;` is `Safepoint`, `LoadLocal`, `LoadLocal`, `Add`, `BinConst`, `StoreLocalPop`, and each pushed local
is written to and read back from the operand stack. Fusing the operand loads into the consuming op removes both.

## Change

Four ops, emitted by the compiler when every operand is a function-local name (frame or block slot) or a number literal:

- `LocalLocalBin a b op`: `a op b` for any of the 20 binary ops (`LoadLocal a; LoadLocal b; op`). A Number pair is computed straight from the
  two bindings; anything else (TDZ, non-Number, a hint miss) pushes the two operands in order through `push_local!` and runs the existing
  `binary_arm!` tail, so ToPrimitive order, BigInt, string and error behaviour are the generic code's.
- `MoveLocal src dst`: `dst = src` in effect context (`LoadLocal; StoreLocalPop`). The value is copied binding to binding (`read_local` then
  `write_local`, bit copy for plain values); a miss on either side falls back to `push_local!` + the `StoreLocalPop` body.
- `LocalLocalCmpJump a b cmp off` / `LocalConstCmpJump a cmp k off`: the `if`/loop condition forms of `CmpJumpIfFalse`
  (`a < b`, `i < 10`). `LocalConstCmpJump` replaces `LocalBinConst` + `JumpIfFalse` (a pushed Boolean and a pop). The generic tail is
  `cmp_jump_taken!`, the body `CmpJumpIfFalse` already had, now a macro shared by all three.

The jump offset is the last operand of both jump ops, so `patch_jump` is unchanged. Handler ranges, safepoints and the stack-height
accounting (`max_stack` counts the two temporaries of the slow path) need no change. `Instanceof`/`In` never fuse.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

base = `origin/main` `0474e481` (#1129; binary sha256 `16a23f83ab86...`), new = this change (`018be8b59174...`). cachegrind `--cache-sim=no`; loop
fixtures are Ir per iteration = (60,000 minus 20,000 iterations) / 40,000. Boa is 0.22 (`boa-prof`).

| fixture | base | new | delta | boa |
|---|---:|---:|---:|---:|
| empty `for` loop | 328 | 257 | -21.6% | 442 |
| 32 x `a = b;` (net of loop, per move) | 162 | 127 | -21.6% | 158 |
| `arith` (the issue's `opmix-arith` loop) | 2,041 | 1,912 | -6.3% | 2,185 |
| float arithmetic loop | 1,908 | 1,735 | -9.1% | 2,046 |
| `for (let i...) { let t = i + 1; s = s + t; }` | 1,329 | 1,234 | -7.1% | 603 |

Cumulative with #1129 against the pre-#1129 base: `a = b;` 251 -> 127 net per move, `arith` 2,822 -> 1,912, float loop 2,529 -> 1,735.

Whole-process Ir (JetStream drivers at `--iterations 1`, micro suite):

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| micro:opmix | 42,674,601,227 | 38,860,066,503 | -8.94% |
| gaussian-blur | 21,268,482,903 | 19,879,765,660 | -6.53% |
| proxy-vue | 1,148,512,343 | 1,103,423,464 | -3.93% |
| micro:fib | 5,286,991,466 | 5,111,978,697 | -3.31% |
| crypto | 3,945,515,919 | 3,852,689,319 | -2.35% |
| splay | 7,750,982,981 | 7,678,921,899 | -0.93% |
| Babylon | 2,365,861,752 | 2,352,315,689 | -0.57% |
| Box2D | 5,448,982,726 | 5,417,750,898 | -0.57% |
| navier-stokes | 3,162,885,082 | 3,146,934,747 | -0.50% |
| hash-map | 13,067,315,802 | 13,029,165,747 | -0.29% |
| ai-astar | 10,095,804,149 | 10,074,606,488 | -0.21% |
| micro:array | 740,665,715 | 739,213,728 | -0.20% |
| bigint-bigdenary | 4,208,401,245 | 4,203,500,370 | -0.12% |
| bigint-noble-secp256k1 | 4,812,719,632 | 4,807,487,749 | -0.11% |
| Air | 1,768,862,553 | 1,768,621,355 | -0.01% |
| micro:string | 335,984,778 | 335,984,278 | -0.00% |
| raytrace | 11,652,108,759 | 11,657,569,714 | +0.05% |
| micro:loop | 3,084,546,745 | 3,087,549,697 | +0.10% |
| richards | 8,519,230,901 | 8,528,985,249 | +0.11% |
| micro:closures | 5,779,954,020 | 5,787,349,902 | +0.13% |
| bigint-noble-ed25519 | 5,583,520,771 | 5,593,729,431 | +0.18% |
| delta-blue | 6,321,903,138 | 6,334,889,221 | +0.21% |
| micro:object | 1,465,157,280 | 1,469,720,750 | +0.31% |
| bigint-noble-bls12-381 | 56,074,249,922 | 56,361,976,051 | +0.51% |
| earley-boyer | 4,958,572,394 | 5,005,929,972 | +0.96% |

`proxy-vue` moves by +-3% between runs of one binary (randomised interner). The small bigint-bls12-381 and earley-boyer increases are code layout:
neither executes a fused op on a hot path. The block-let loop is still 2x Boa; that gap is `PushEnv`/`PopEnv` environment churn, not
local access, and stays with #1030's expanded inventory.

Wall time, the issue's `arith(10000000)` repro, pinned core, 5 alternating runs, load 1.4: base (pre-#1129) 5.05 s, #1129 4.71 s, this
change 4.19 s, boa 4.75 s (jsse 0.88x boa).

## Correctness gates

`test262-extra/local-pair-operations-semantics.js` (Number/BigInt/string/Symbol operand kinds, NaN and signed zero, ToPrimitive order, a
`valueOf` that rebinds the right operand, TDZ on either side of an operator, compare-and-branch and move, const and TDZ move targets,
captured and block-scoped locals) passes on the base binary and under Node. Counts and gates are in the PR.
