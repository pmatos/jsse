# #1132: a loop body's `let`/`const` share the loop's environment

A body block that declares `let`/`const` ran `PushEnv`/`PopEnv` on every iteration (pooled environment acquire, declare, release),
which was about 40% of the instructions of `for (let i...) { let t = i + 1; s = s + t; }`. When nothing can capture the
environment and no read can precede a declaration, the body's bindings now live in one environment opened before the loop (merged
with the head's own scope for `for (let ...)` and `for (const x of ...)`), and each iteration re-initialises them with
`InitLexical`. No op is added or changed; `vm.rs` is untouched. Rules and rationale:
`docs/adr/2026-10-20-0345-loop-body-environment-sharing.md`.

## Change

- `compiler/loop_scope.rs`: `open_loop_scope` decides per loop (`for`, `while`, `do`-`while`, `for-in`, `for-of`) whether the body
  block can share the loop environment and opens one scope with the head's and the body's names. `compile_loop_body` compiles the
  block's statements in it. The analysis is a compile-time scan, once per loop, over the body's top-level statements.
- `ast/capture_walk.rs`: `CaptureWalk` (the closure/`eval`/`with`/class scan behind `may_capture_iteration_scope`) moved out of `ast.rs`
  and can collect the identifiers it reaches. `compiler/block_scope.rs` takes the existing scope helpers out of `compiler.rs`.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

base = `origin/main` `7a7151e7`, new = this branch. cachegrind `--cache-sim=no`. Fixtures are Ir per iteration = (60,000 minus
20,000 iterations) / 40,000, each loop in a function. Boa is 0.22 (`boa-release`).

| fixture | base | new | delta | boa |
|---|---:|---:|---:|---:|
| `for (let i...) { let t = i + 1; s = s + t; }` (the issue's loop) | 1,254 | 759 | -39.5% | 622 |
| `for (let i...) { const a = i + 1; const b = a * 2; s = s + b; }` | 1,593 | 980 | -38.5% | 799 |
| `while (i < n) { let t = i + 1; s = s + t; i++; }` | 1,237 | 741 | -40.1% | |
| same loop with `var i`, `var t` (floor for these ops) | 708 | 707 | -0.1% | 861 |
| empty `for (let i...)` | 280 | 280 | 0.0% | 289 |
| the issue's loop with a closure capturing `t` once | 1,401 | 1,401 | 0.0% | |

The issue's acceptance bar (at or below boa's Ir per iteration) is **not met**: 759 against 622 on this fixture (603 in the #1030
measurement). Per iteration the loop executes seven ops (`LocalLocalCmpJump`, `LocalBinConst`, `InitLexical`, `LocalLocalBin`,
`StoreLocalPop`, `UpdateLocal`, `Jump`), about 105 Ir each in `run_chunk_inner`. What is left is the dispatch cost of those ops: the
equivalent `var` loop is 707 (and boa's own `var` loop is 861), and `InitLexical` costs 52 Ir more than `StoreLocalPop`.
Environment churn is gone; closing the remaining 22% needs fused ops or cheaper dispatch in `vm.rs` (#1030's inventory).

`PushEnv` count, `perf-counters`: 1 per loop entry instead of one per iteration (`tests/perf_counters_loop_body_environment.rs`).

## Control set, whole-process Ir

cachegrind `I refs`, JetStream drivers at `--iterations 1`, micro suite; one run per binary except the noisy rows below.

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| UniPoker | 3,797,808,642 | 3,699,986,310 | -2.58% |
| bigint-noble-secp256k1 | 4,673,056,294 | 4,561,462,801 | -2.39% |
| micro:fib | 4,967,911,865 | 4,868,289,870 | -2.01% |
| Babylon | 2,309,938,756 | 2,283,909,128 | -1.13% |
| raytrace | 11,432,914,273 | 11,333,999,214 | -0.87% |
| Air | 1,659,828,860 | 1,646,648,922 | -0.79% |
| richards | 8,395,672,948 | 8,394,167,428 | -0.02% |
| delta-blue | 6,175,982,090 | 6,176,287,929 | 0.00% |
| Box2D | 5,310,669,484 | 5,309,163,035 | -0.03% |
| lazy-collections | 3,682,337,914 | 3,681,916,212 | -0.01% |
| micro:opmix | 38,707,347,364 | 38,707,351,747 | 0.00% |
| gaussian-blur | 19,773,891,442 | 19,773,908,294 | 0.00% |
| splay | 7,543,194,493 | 7,546,481,715 | +0.04% |

The other 19 workloads measured (earley-boyer, crypto, cdjs, navier-stokes, hash-map, typescript-octane, pdfjs, regexp-octane,
validatorjs, ai-astar, and the string/array/object/json/loop/regex/closures micros) move by less than 0.4%.

`js-tokens` and `mandreel` read +1.5% and +2.3% in the first pass. Both are randomised-interner noisy (a single binary spans about
+-1%): ten runs per binary of js-tokens give medians 4.693 G (base) and 4.700 G (new) Ir; four runs per binary of mandreel give
182.4 G and 182.3 G.

## Wall time and memory

Pinned core, runs alternating base/new, load average at most 1.5 at the start of every run, shipped binaries, medians of 9
(seconds):

| benchmark | base | new | delta |
|---|---:|---:|---:|
| bigint-noble-secp256k1 | 1.110 | 1.070 | -3.6% |
| Box2D | 1.250 | 1.210 | -3.2% |
| lazy-collections | 0.880 | 0.850 | -3.4% |
| richards | 1.930 | 1.890 | -2.1% |
| delta-blue | 1.390 | 1.370 | -1.4% |
| fib | 0.920 | 0.910 | -1.1% |
| splay | 2.100 | 2.090 | -0.5% |
| gaussian-blur | 3.980 | 3.970 | -0.3% |
| micro:opmix | 7.010 | 7.060 | +0.7% |

`micro:opmix` is the one regression: +0.7% median wall (+0.7% in each of three separate batches; gaussian-blur reads +0.8%
to -0.3% across batches) at an Ir count that differs by 4 kIr of 38.7 G, so no executed instruction changed: `opmix` never meets a
loop body that this change touches. Diagnostics:

- `run_chunk_inner::<false>` moves from `0x11fa100` to `0x11fa220` (64-byte to 32-byte alignment). Building both sides with
  `-C llvm-args=-align-all-functions=6` puts it at 64-byte alignment in both (`0x120e240`, `0x120e300`) and the gap stays (+0.56%
  opmix, +0.76% gaussian-blur, medians of 9), so function alignment alone does not explain it.
- Null control: two byte-identical copies of the base binary measured the same way (11 alternating runs): medians 6.99 s and 6.98 s.
  The harness resolves roughly +-0.15% on medians; single runs span +-1%.
- No `vm.rs` code changed; the delta is code placement elsewhere in the fat-LTO binary (`compiler.rs` lost 3.2 KB to two new modules
  and gained loop scope code). Same class as the +0.4% Ir layout effects recorded in #1030.

Peak RSS (`/usr/bin/time %M`): Box2D, 15 interleaved runs of each binary plus a second copy of the base binary: medians
38,648 KB base, 38,548 KB new, 38,612 KB base copy. `jsse empty.js`: 16.3 MB for both.

## Correctness gates

- full test262, default and `--no-bytecode`: 99,911 / 99,911 scenarios, 0 regressions (release, non-LTO profile).
- `test262-extra/` (1,563 scenarios) default, `--no-bytecode`, `JSSE_GC_STRESS=7` default and `--no-bytecode`, release-checked build
  default and `JSSE_GC_STRESS=7`; the new `loop-body-block-environment-reuse.js` additionally at `JSSE_GC_STRESS=1` and `3`.
- `cargo test --profile release-nolto`, `cargo test --profile release-nolto --features perf-counters --test 'perf_counters_*'`,
  `./scripts/lint.sh`, `uv run python scripts/run-custom-tests.py`.
