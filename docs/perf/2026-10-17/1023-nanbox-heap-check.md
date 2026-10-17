# #1023: one comparison decides whether a `JsValue` owns a heap pointer

Re-profiling `fib` on `origin/main` (`252429a7`) for #1023 put `NanBoxedValue::tag()` at the top of the source-line table:
`is_boxed` (`types.rs:1019`) 3.8%, the `match self.tag()` in `Clone` and `Drop` another 3.9%, and the slice drop glue
that every call-operand pop, environment release and `ThisBinding` drop runs. `Clone` and `Drop` rebuilt an
`Option<NanTag>` (boxed test, shift, mask, eight-way match) to find out that almost every value is a Number, `undefined`
or an object id and owns nothing.

## Change

Only String, Symbol and BigInt hold an `Arc` strong count. Their tags (5, 6, 7) are the three largest boxed tags, and
`NanBoxedValue::number` canonicalises every NaN away from the boxed range, so a value owns a heap pointer exactly when its
bits are `>= BOX_SIGNATURE | (NanTag::String << TAG_SHIFT)`. `Clone` and `Drop` test that with one unsigned comparison
and call an out-of-line `retain_heap_pointer`/`release_heap_pointer` (the old match, now with an `unreachable!` default)
only on a hit. No `unsafe` is added or removed; the existing `Arc` calls are unchanged. A const assertion pins the tag
order the comparison relies on, and a unit test checks the owning/non-owning partition (including an Object with the
largest payload and every Infinity/NaN/maximum Number) and the strong counts across clone and drop.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`, JetStream drivers at `--iterations 1`. base = `origin/main`
`252429a7` (binary sha256 `1652d41399ff...`), new = base plus this change (`f9a3ed3b9fcb...`).

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| crypto | 5,698,396,070 | 5,004,666,914 | -12.17% |
| gaussian-blur | 30,804,734,816 | 27,245,352,216 | -11.55% |
| navier-stokes | 4,367,898,141 | 3,866,731,912 | -11.47% |
| stanford-crypto-sha256 | 5,080,181,377 | 4,571,880,976 | -10.01% |
| array (micro) | 943,786,905 | 850,414,202 | -9.89% |
| opmix (micro) | 60,359,352,394 | 54,608,587,675 | -9.53% |
| hash-map | 18,552,857,420 | 16,798,936,288 | -9.45% |
| Box2D | 8,004,961,605 | 7,285,677,737 | -8.99% |
| earley-boyer | 6,356,144,057 | 5,808,138,766 | -8.62% |
| raytrace | 17,083,544,661 | 15,741,030,523 | -7.86% |
| delta-blue | 8,805,079,490 | 8,114,627,482 | -7.84% |
| richards | 13,430,900,769 | 12,392,248,910 | -7.73% |
| ai-astar | 14,159,767,504 | 13,121,083,308 | -7.34% |
| fib (micro) | 7,451,779,369 | 6,907,891,348 | -7.30% |
| closures (micro) | 6,543,309,715 | 6,086,762,738 | -6.98% |
| js-tokens | 5,333,391,929 | 4,976,938,242 | -6.68% |
| Babylon | 3,290,245,353 | 3,079,767,723 | -6.40% |
| lazy-collections | 5,044,197,084 | 4,722,051,551 | -6.39% |
| splay | 9,852,255,473 | 9,254,256,177 | -6.07% |
| proxy-mobx | 726,841,150 | 682,788,440 | -6.06% |
| promise_new (micro) | 6,268,611,363 | 5,893,008,881 | -5.99% |
| bigint-bigdenary | 5,583,542,126 | 5,261,000,364 | -5.78% |
| string (micro) | 363,976,871 | 343,181,878 | -5.71% |
| Basic | 3,872,815,019 | 3,654,793,744 | -5.63% |
| object (micro) | 1,520,374,281 | 1,466,690,802 | -3.53% |
| Air | 2,137,678,751 | 2,065,931,367 | -3.36% |
| regex (micro) | 120,982,278 | 117,005,572 | -3.29% |
| bigint-noble-ed25519 | 7,776,512,935 | 7,533,245,762 | -3.13% |
| doxbee-promise | 4,194,535,123 | 4,081,121,225 | -2.70% |
| json (micro) | 320,651,307 | 313,581,344 | -2.20% |
| json-parse-inspector | 9,170,502,805 | 9,159,093,945 | -0.12% |

Geometric mean over the 31 workloads: -6.81%; nothing regresses. The leaf-call reproducer from #1023 (Ir per iteration
= (N=250,000 minus N=50,000) / 200,000) goes from 3,158 to 2,868 for a sloppy call with one argument (-290), 3,725 to
3,312 with four, and the call-free control loop from 835 to 735, so the saving is not specific to calls.

Wall time (hyperfine, 9 runs, pinned to one CPU, load average 1.3, `jsse-base` vs the final build; medians): fib
1.392 s -> 1.330 s (-4.5%), richards 3.325 s -> 3.091 s (-7.0%), stanford-crypto-sha256 1.031 s -> 0.968 s (-6.1%),
the `string` micro-benchmark (the one most dominated by String clone/drop) 0.079 s -> 0.077 s.

## Correctness gates

Behaviour is unchanged (the representation and every observable value are the same); the unit tests in `types.rs` that
pin the NaN-box range still pass. Full test262 default and `--no-bytecode`: 99,911/99,911 each; `test262-extra/`
normal, `--no-bytecode`, and both under `JSSE_GC_STRESS=7`, and on the `release-checked` binary in both modes, all
pass; `cargo test --lib` 1,139 passed; `tests/recursion-limit-*.js` and `scripts/run-custom-tests.py` (19/19) pass on the
plain release build; `./scripts/lint.sh` clean.
