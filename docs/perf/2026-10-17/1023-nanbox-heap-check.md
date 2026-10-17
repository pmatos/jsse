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
only on a hit. No `unsafe` is added or removed; the existing `Arc` calls are unchanged.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`, JetStream drivers at `--iterations 1`. base = `origin/main`
`252429a7` (binary sha256 `1652d41399ff...`), new = base plus this change (`da2a1f4c16fb...`).

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| crypto | 5,698,396,070 | 4,973,546,802 | -12.72% |
| gaussian-blur | 30,804,734,816 | 27,032,040,599 | -12.25% |
| opmix (micro) | 60,359,352,394 | 53,856,029,867 | -10.77% |
| navier-stokes | 4,367,898,141 | 3,898,219,831 | -10.75% |
| stanford-crypto-sha256 | 5,080,181,377 | 4,537,580,696 | -10.68% |
| js-tokens | 5,333,391,929 | 4,803,719,829 | -9.93% |
| hash-map | 18,552,857,420 | 16,779,175,555 | -9.56% |
| Box2D | 8,004,961,605 | 7,244,733,367 | -9.50% |
| earley-boyer | 6,356,144,057 | 5,812,338,807 | -8.56% |
| raytrace | 17,083,544,661 | 15,698,668,936 | -8.11% |
| delta-blue | 8,805,079,490 | 8,098,213,609 | -8.03% |
| richards | 13,430,900,769 | 12,353,191,909 | -8.02% |
| ai-astar | 14,159,767,504 | 13,085,250,978 | -7.59% |
| fib (micro) | 7,451,779,369 | 6,894,428,671 | -7.48% |
| closures (micro) | 6,543,309,715 | 6,076,851,463 | -7.13% |
| lazy-collections | 5,044,197,084 | 4,711,770,378 | -6.59% |
| Babylon | 3,290,245,353 | 3,075,285,863 | -6.53% |
| promise_new (micro) | 6,268,611,363 | 5,878,664,167 | -6.22% |
| splay | 9,852,255,473 | 9,258,391,003 | -6.03% |
| Basic | 3,872,815,019 | 3,655,608,842 | -5.61% |
| bigint-bigdenary | 5,583,542,126 | 5,270,386,082 | -5.61% |
| string (micro) | 363,976,871 | 346,982,837 | -4.67% |
| proxy-mobx | 726,841,150 | 693,592,423 | -4.57% |
| object (micro) | 1,520,374,281 | 1,471,991,606 | -3.18% |
| regex (micro) | 120,982,278 | 117,241,136 | -3.09% |
| Air | 2,137,678,751 | 2,078,592,604 | -2.76% |
| bigint-noble-ed25519 | 7,776,512,935 | 7,563,738,041 | -2.74% |
| doxbee-promise | 4,194,535,123 | 4,080,766,521 | -2.71% |
| json (micro) | 320,651,307 | 313,764,778 | -2.15% |
| json-parse-inspector | 9,170,502,805 | 9,162,239,588 | -0.09% |

Geometric mean over the 31 workloads: -6.94%; nothing regresses. The leaf-call reproducer from #1023 (Ir per iteration
= (N=250,000 minus N=50,000) / 200,000) goes from 3,158 to 2,859 for a sloppy call with one argument (-299), 3,725 to
3,300 with four, and the call-free control loop from 835 to 729, so the saving is not specific to calls.

## Correctness gates

Behaviour is unchanged (the representation and every observable value are the same); the unit tests in `types.rs` that
pin the NaN-box range still pass. Full test262 default and `--no-bytecode`: 99,911/99,911 each; `test262-extra/`
normal, `--no-bytecode`, and both under `JSSE_GC_STRESS=7`, and on the `release-checked` binary in both modes, all
pass; `cargo test --lib` 1,139 passed; `tests/recursion-limit-*.js` and `scripts/run-custom-tests.py` (19/19) pass on the
plain release build; `./scripts/lint.sh` clean.
