# #1030: local ops without per-access vetting, single-check operand decode, plain-value moves

#1030 reports the native arithmetic loops at 1.28-1.58x Boa in Ir, with the gap in `run_chunk_inner` dispatch rather than in any
helper. An instruction-level callgrind profile of the `a = b;` fixture (3 ops: `Safepoint`, `LoadLocal`, `StoreLocalPop`, 251 Ir per
statement) showed three avoidable costs repeated on every local access.

## Change

- **Frame vetting once per chunk.** `LoadLocal`/`StoreLocal`/`UpdateLocal`/`LocalBinConst` re-tested `with_object`,
  `indirect_bindings` and `global_object_id` on the scope before each access. The frame environment is checked once on entry
  (`is_plain_declarative`, `frame_plain`); when it is not plain every frame-slot op uses `NO_LOCAL_SLOT` and takes the by-name path.
  Block environments (`PushEnv`) are declarative by construction.
- **Shared hint verification.** `hinted_binding`/`hinted_writable` are the one place a slot hint is verified (atom identity,
  initialized, `Var`/`Let` for writes); `decode_local` reads the name/slot operand pair.
- **`local_target`** takes the frame branch inline; the block branch (`block_target`) is a call, so the `SmallVec` deref and `cmov` pair
  no longer run on every frame access.
- **One bounds check per operand group.** `decode_u16`/`decode_i16` and the local/constant arms read their operand bytes through
  `operand_bytes::<N>` (`first_chunk`), one check where each byte had its own.
- **Plain values move without clone/drop work.** `JsValue::is_plain` (one compare: Number, `undefined`, `null`, Boolean) lets
  `LoadLocal` copy bits and push with no rooting test. `StoreLocalPop` is its own arm that moves the popped operand into the binding
  (`move_into_local`) instead of clone + store + pop + drop. `root_stack_value`/`unroot_stack_value` test `is_plain` first, so every
  push and pop of a plain value skips the object-tag test.

No `unsafe` beyond the existing `discard_number_operands`. Behaviour is unchanged (`test262-extra/local-slot-load-store-edge-cases.js`
passes on the base binary as well).

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

base = `origin/main` `fe568175` (binary sha256 `035a19f9a96e...`), new = this change (`16a23f83ab86...`). cachegrind `--cache-sim=no`.
Loop fixtures: Ir per iteration = (60,000 minus 20,000 iterations) / 40,000. Boa column: boa 0.22 (`boa-prof`).

| fixture | base | new | delta | boa |
|---|---:|---:|---:|---:|
| empty `for` loop | 440 | 328 | -25.5% | 442 |
| 32 x `a = b;` (net of loop, per move) | 251 | 162 | -35% | 158 |
| `arith` (the issue's `opmix-arith` loop) | 2,822 | 2,041 | -27.7% | 2,185 |
| float arithmetic loop | 2,529 | 1,908 | -24.6% | 2,046 |

Whole-process Ir (JetStream drivers at `--iterations 1`, micro suite):

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| gaussian-blur | 26,771,305,760 | 21,268,482,903 | -20.55% |
| crypto | 4,874,657,903 | 3,945,515,919 | -19.06% |
| micro:opmix | 52,416,853,188 | 42,674,601,227 | -18.59% |
| navier-stokes | 3,788,055,506 | 3,162,885,082 | -16.50% |
| Box2D | 5,979,709,460 | 5,448,982,726 | -8.88% |
| hash-map | 13,914,913,998 | 13,067,315,802 | -6.09% |
| micro:fib | 5,622,213,866 | 5,286,991,466 | -5.96% |
| bigint-noble-ed25519 | 5,895,357,699 | 5,583,520,771 | -5.29% |
| ai-astar | 10,639,917,692 | 10,095,804,149 | -5.11% |
| bigint-noble-secp256k1 | 5,069,695,165 | 4,812,719,632 | -5.07% |
| splay | 8,103,132,770 | 7,750,982,981 | -4.35% |
| earley-boyer | 5,169,248,470 | 4,958,572,394 | -4.08% |
| raytrace | 12,002,438,023 | 11,652,108,759 | -2.92% |
| bigint-bigdenary | 4,325,085,528 | 4,208,401,245 | -2.70% |
| Babylon | 2,424,010,766 | 2,365,861,752 | -2.40% |
| micro:array | 754,667,528 | 740,665,715 | -1.86% |
| richards | 8,680,052,802 | 8,519,230,901 | -1.85% |
| bigint-noble-bls12-381 | 57,089,592,643 | 56,074,249,922 | -1.78% |
| delta-blue | 6,431,247,543 | 6,321,903,138 | -1.70% |
| micro:loop | 3,129,546,873 | 3,084,546,745 | -1.44% |
| Air | 1,790,678,540 | 1,768,862,553 | -1.22% |
| micro:string | 338,383,356 | 335,984,778 | -0.71% |
| micro:closures | 5,816,019,422 | 5,779,954,020 | -0.62% |
| micro:object | 1,469,946,527 | 1,465,157,280 | -0.33% |
| proxy-vue | 1,136,721,475 | 1,148,512,343 | +1.04% |

`micro:loop` (`sum += i` at Script top level, no local ops) and proxy-vue (+1.0%; its total moves by +-3% between runs of one binary with the randomised interner: 1,108M-1,156M for the earlier build, 1,091M-1,100M here against 1,128M-1,137M base) are code-layout and noise.

Wall time, the issue's `arith(10000000)` repro, pinned core, 5 alternating runs. Before the review cleanups, at load 0.9: base 5.07 s,
new 4.18 s, boa 4.80 s. The final build at load 1.2-2.6 (another job started on the host): base 4.96 s, new 4.55 s, boa 4.60 s.

## Correctness gates

Full test262 default and `--no-bytecode`: 99,911/99,911 each. `test262-extra/` 1491/1491 on a `release-checked` build, normal and
`--no-bytecode`, and under `JSSE_GC_STRESS=7` in both modes. `cargo test --profile release-nolto`, `./scripts/lint.sh` and the 19
custom tests are clean.
