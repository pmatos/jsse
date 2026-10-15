# Engine benchmarks

`engine.rs` contains 35 CodSpeed/Divan benchmarks: two parser cases and 33
evaluation cases, including the empty-program setup control. Evaluation measures
parsing, a fresh realm, execution, job draining, validation, and teardown. Source
template construction happens outside the measured closure. The benchmark
profile inherits the shipped release configuration (fat LTO, one codegen unit).

The targeted cases complement the original broad workloads:

| Cases | Purpose | Source workload |
| --- | --- | --- |
| `regexp::sticky_*` | Failed anchored matches on a 14,001-code-unit subject, including lookahead and interleaved short subjects | `docs/perf/2026-10-10/repros/generators/bench_generators_sticky_{regex,lookahead}.js` |
| `async_jobs::{then_chain,await_loop}` | 2,000 chained reactions; 1,000 iterations awaiting a primitive, a resolved promise, and an async call | `docs/perf/2026-10-10/repros/promises/bench_promises_{then_chain,await_loop}.js` |
| `generators::{manual_next,pipeline}` | Sent values and suspension; composed generators with early iterator closing | `docs/perf/2026-10-10/repros/generators/bench_generators_{manual_next,pipeline}.js` |
| `fields::{private,public}_{fields,getters}` | Matched private/public controls, including trivial private getters | `docs/perf/2026-10-10/repros/proxy-classes/bench_proxy-classes_{private,public}_access.js` and the #931 getter report |
| `gc::retained_{array,map}` | Retain 16,384 records, then overwrite 1,024 slots with 24,576 fresh records; exercise promotion, remembered containers, and dirty cards | `docs/perf/2026-10-10/repros/alloc-gc/bench_alloc-gc_retain_grow.js` and the #988 Map/Set report |
| `map_set_rolling` | 5,000 insert/delete rounds with a 256-entry live window, including tombstone compaction | `docs/perf/2026-10-10/repros/alloc-gc/bench_alloc-gc_map_rolling.js` |
| `opmix::*` | Separate arithmetic, element traffic, leaf calls, labeled loops, and labeled control flow | `benchmarks/scripts/bench_opmix.js` |
| `bigint_mulmod256` | 1,000 rounds of 256-bit multiply/remainder arithmetic | `docs/perf/2026-10-10/repros/bigint/bench_bigint_mulmod256.js` |
| `closures_local` | Function-local slots alongside the existing script-scope closure case | `benchmarks/scripts/bench_closure_create.js` |

All nonempty evaluation workloads check their result. Expected checksums were
cross-checked with Node. Async workloads store a fulfillment result, then
`bench_eval_with_check` runs a synchronous assertion in the same interpreter
after jobs drain. A missing result fails too, covering rejection or a callback
that never ran. Throwing only inside `.then` would merely reject its promise.

Run every case once for correctness without fat-LTO test relinking:

```sh
cargo bench --locked --bench engine --profile release-nolto -- --test
cargo test --profile release-nolto --lib benchmark_validation
```

Run local wall-clock measurements or the CI simulation configuration:

```sh
cargo bench --locked --bench engine
cargo codspeed build -m simulation
codspeed run -m simulation -- cargo codspeed run
```

Keep fixed iteration counts; do not add `Date.now()` loops or console output to
measured scripts. Before shrinking the GC cases, use a separate `perf-counters`
CLI build to confirm that they still perform minor collections after the
container survives promotion. Never time that instrumented build, and do not
compare `release-nolto` timings with shipped-profile measurements. These cases
track end-to-end work; they do not isolate collector time from allocation or
provide a steady-state execution-only measurement.

At the current sizes, a separate `perf-counters` check recorded three minor and
three major collections for each retained-container case, with each `run` body
dispatched through bytecode. These counts verify workload sizing; they are not
timing results or fixed expectations for future collector implementations.
