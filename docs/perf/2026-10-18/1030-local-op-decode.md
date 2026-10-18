# #1030: local ops without per-access vetting, single-check operand decode, plain-value moves

#1030 reports the native arithmetic loops at 1.28-1.58x Boa in Ir, with the gap in `run_chunk_inner` dispatch rather than in any
helper. An instruction-level callgrind profile of the `a = b;` fixture (3 ops: `Safepoint`, `LoadLocal`, `StoreLocalPop`, 251 Ir per
statement) showed three avoidable costs repeated on every local access.

## Change

- **Frame vetting once per chunk.** `LoadLocal`/`StoreLocal`/`UpdateLocal`/`LocalBinConst` re-tested `with_object`,
  `indirect_bindings` and `global_object_id` on the scope before each access. The frame environment is checked once on entry
  (`is_plain_declarative`, `frame_plain`); when it is not plain every frame-slot op uses `NO_LOCAL_SLOT` and takes the by-name path.
  Block environments (`PushEnv`) are declarative by construction.
- **`local_target`** takes the frame branch inline; the block branch (`block_target`) is a call, so the `SmallVec` deref and `cmov` pair
  no longer run on every frame access.
- **One bounds check per operand group.** `decode_u16`/`decode_i16` and the local/constant arms read their operand bytes through
  `operand_bytes::<N>` (`first_chunk`), one check where each byte had its own.
- **Plain values move without clone/drop work.** `JsValue::is_plain` (one compare: Number, `undefined`, `null`, Boolean) lets
  `LoadLocal` copy bits and push with no rooting test. `StoreLocalPop` is its own arm that moves the popped operand into the binding
  (`move_into_local`) instead of clone + store + pop + drop.

No `unsafe` beyond the existing `discard_number_operands`. Behaviour is unchanged (`test262-extra/local-slot-load-store-edge-cases.js`
passes on the base binary as well).

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

base = `origin/main` `fe568175` (binary sha256 `035a19f9a96e...`), new = this change (`8dab13db3921...`). cachegrind `--cache-sim=no`.
Loop fixtures: Ir per iteration = (60,000 minus 20,000 iterations) / 40,000. Boa column: boa 0.22 (`boa-prof`).

| fixture | base | new | delta | boa |
|---|---:|---:|---:|---:|
| empty `for` loop | 440 | 330 | -25% | 442 |
| 32 x `a = b;` (net of loop, per move) | 251 | 168 | -33% | 158 |
| `arith` (the issue's `opmix-arith` loop) | 2,822 | 2,082 | -26.2% | 2,185 |
| float arithmetic loop | 2,529 | 1,942 | -23.2% | 2,046 |

Whole-process Ir (JetStream drivers at `--iterations 1`, micro suite):

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| gaussian-blur | 26,771,305,760 | 21,694,271,932 | -18.96% |
| micro:opmix | 52,416,853,188 | 43,137,415,000 | -17.70% |
| crypto | 4,874,657,903 | 4,020,036,460 | -17.53% |
| navier-stokes | 3,788,055,506 | 3,256,279,494 | -14.04% |
| Box2D | 5,979,709,460 | 5,506,423,537 | -7.91% |
| hash-map | 13,914,913,998 | 13,111,812,036 | -5.77% |
| micro:fib | 5,622,213,866 | 5,309,874,937 | -5.56% |
| bigint-noble-ed25519 | 5,895,357,699 | 5,580,734,282 | -5.34% |
| bigint-noble-secp256k1 | 5,069,695,165 | 4,842,526,975 | -4.48% |
| ai-astar | 10,639,917,692 | 10,176,218,164 | -4.36% |
| splay | 8,103,132,770 | 7,785,076,446 | -3.93% |
| earley-boyer | 5,169,248,470 | 4,995,919,435 | -3.35% |
| raytrace | 12,002,438,023 | 11,695,624,211 | -2.56% |
| bigint-bigdenary | 4,325,085,528 | 4,216,573,858 | -2.51% |
| Babylon | 2,424,010,766 | 2,367,921,183 | -2.31% |
| proxy-vue | 1,136,721,475 | 1,110,773,425 | -2.28% |
| delta-blue | 6,431,247,543 | 6,309,697,040 | -1.89% |
| richards | 8,680,052,802 | 8,525,945,876 | -1.78% |
| bigint-noble-bls12-381 | 57,089,592,643 | 56,089,488,829 | -1.75% |
| micro:array | 754,667,528 | 743,011,989 | -1.54% |
| Air | 1,790,678,540 | 1,767,596,693 | -1.29% |
| micro:string | 338,383,356 | 336,481,503 | -0.56% |
| micro:closures | 5,816,019,422 | 5,799,118,812 | -0.29% |
| micro:object | 1,469,946,527 | 1,467,752,548 | -0.15% |
| micro:loop | 3,129,546,873 | 3,174,548,090 | +1.44% |

`micro:loop` (`sum += i` at Script top level, no local ops) moves with code layout alone.

Wall time, the issue's `arith(10000000)` repro, pinned core, 5 alternating runs, load 0.9: base 5.07 s, new 4.18 s, boa 4.80 s
(jsse 0.87x boa).

## Correctness gates

Full test262 default and `--no-bytecode`: 99,911/99,911 each. `test262-extra/` 1491/1491 on a `release-checked` build, normal and
`--no-bytecode`, and under `JSSE_GC_STRESS=7` in both modes. `cargo test --profile release-nolto`, `./scripts/lint.sh` and the 19
custom tests are clean.
