perf(gc): nursery collections repeatedly trace large environment/object graphs in Three.js and deep promise queues

Three.js is 1.170x Boa and the deep queue is 1.100x, despite other Promise cases favoring JSSE. The Three.js default-binary sampling profile has 22.65% self in gc_safepoint, 14.62% in trace_object_fields_in and 13.42% in collect_env_roots. The deep-queue profile has 28.84% in trace_object_fields_in and 25.44% in collect_env_roots, plus 11.84% in gc_safepoint.

Separate counter runs report Three.js **850 minor / 33 major collections**, 14,699 ms collected GC time of which 13,614 ms is minor; the queue reports **29 minor / 5 major**, 661 ms GC of which 609 ms is minor. These timings belong to the separate instrumented runs and are not divided by the default-binary wall times. Splay reports 141 minor / 7 major, with 157 of 246 GC ms spent in minors. The sampling profiles independently establish the hot paths.

[collect_env_roots](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/gc.rs#L1550) walks bindings/this/disposal roots and parents, deduplicating environment pointers through a fresh HashSet. [Object tracing](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/gc.rs#L1254) reaches captured environments and other old graph edges. Array and Map/Set card tracking (#912/#988) does not eliminate this environment traversal. Potential root cause: nursery work still scales with large live captured-environment/remembered-object graphs rather than just changed young edges.

Potential direction: measure environment visits and duplicate-parent probes per minor collection; evaluate epoch-based pointer visitation and generation-aware environment/remembered-field tracking. Any young-edge cache requires barriers on every environment mutation path, including VM slot stores, generator bindings, this/new.target and pooled-environment reset. Do not skip old environments without proving those barriers.

## Measurements

JSSE HEAD `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`; Boa HEAD `f0789a20d87ad52eef6460953929c2b3cdbf8716`; AMD Ryzen AI 9 HX 370, Rust 1.98.1, Node v26.8.2. Both engines use their default fat-LTO release profiles; Boa includes its default allocator/Intl features. Binary snapshots are fixed. Three alternating fresh CLI processes per successful case, sequential on CPU 0; 180 s / 8 GiB limits. Background desktop load was present: diagnostic measurements, with every sample and its range retained. Ratios above 1 mean JSSE is slower. V8 ratios use throughput; other rows use process wall time.

| Workload | Metric | JSSE median | Boa median | JSSE slowdown |
|---|---|---:|---:|---:|
| jetstream-extra/threejs | process wall seconds | 30.167191 | 25.789649 | 1.170x |
| diagnostic-1010/promises/bench_promises_queue_deep | process wall seconds | 0.808132 | 0.732927 | 1.103x |
| jetstream/splay | process wall seconds | 0.943974 | 0.785845 | 1.201x |
| diagnostic-1010/alloc-gc/bench_alloc-gc_splay_mini | process wall seconds | 0.191899 | 0.161936 | 1.185x |

JetStream rows are one complete workload iteration, including initialization/preparation and validation, per fresh process. They are not official JetStream scores or the default multi-iteration schedules. Timing uses the uninstrumented release binary; the separate release-nolto counter build is never used for speed ratios.

## Sampling and counter evidence

`jetstream-extra/threejs` self samples:

```text
22.65% jsse [.] <jsse::interpreter::Interpreter>::gc_safepoint
14.62% jsse [.] <jsse::interpreter::Interpreter>::trace_object_fields_in
13.42% jsse [.] <jsse::interpreter::Interpreter>::collect_env_roots
7.97% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
3.15% jsse [.] <jsse::interpreter::Interpreter>::get_object_property_with_proxy_depth::<str>
2.53% jsse [.] <hashbrown::map::HashMap<jsse::types::JsPropertyKey, u32, foldhash::fast::RandomState>>::get::<[u8]>
1.75% jsse [.] <jsse::interpreter::Interpreter>::enqueue_young::<alloc::vec::drain::Drain<u64>>
```

Counters: vm_ops=356222153, ast_work_units=37416143, gc_minor=850, gc_major=33, gc_ms=14699, local_slot_misses=0, atom_intern_calls=13249056.

`diagnostic-1010/promises/bench_promises_queue_deep` self samples:

```text
28.84% jsse [.] <jsse::interpreter::Interpreter>::trace_object_fields_in
25.44% jsse [.] <jsse::interpreter::Interpreter>::collect_env_roots
11.84% jsse [.] <jsse::interpreter::Interpreter>::gc_safepoint
3.54% jsse [.] <jsse::interpreter::Interpreter>::enqueue_young::<alloc::vec::drain::Drain<u64>>
2.59% jsse [.] <std::hash::random::RandomState as core::hash::BuildHasher>::hash_one::<&usize>
2.18% jsse [.] <hashbrown::map::HashMap<usize, (), std::hash::random::RandomState>>::insert
1.77% jsse [.] <jsse::interpreter::Interpreter>::call_function_inner_impl
```

Counters: vm_ops=2100114, ast_work_units=7, gc_minor=29, gc_major=5, gc_ms=661, local_slot_misses=0, atom_intern_calls=106.

## Reproducer

The measured fixture is `diagnostic-1010/promises/bench_promises_queue_deep`. A standalone form of its core workload is:

```js
const N = 20000, K = 5;
let sum = 0;
for (let i = 0; i < N; i++) {
  let p = Promise.resolve(i);
  for (let k = 0; k < K; k++) p = p.then(v => v + 1);
  p.then(v => { sum = (sum + v) | 0; });
}
let ticks = 0;
(function wait() {
  if (++ticks < K + 3) { Promise.resolve().then(wait); return; }
  console.log("queue-deep sum=" + sum);
})();
```

Save as `/tmp/repro.js`, then run the same file with `taskset -c 0 /path/to/jsse /tmp/repro.js` and `taskset -c 0 /path/to/boa /tmp/repro.js`. Interleave at least three process runs and compare medians/checksums. For profiling use `perf record -F 499 -e cpu-clock:u --call-graph dwarf -- /path/to/jsse /tmp/repro.js`; keep instrumented counts separate from wall time.

## Acceptance / semantic constraints

Retain the deep-queue checksum, Three.js workload and splay checks. GC/root changes require the existing checked-build root assertions, GC stress in both execution modes, and the full conformance baseline. Compare minor visits/time as well as wall time; the prior forced-major pacing problem is a different finding.

Any implementation should retain the existing test262 pass baseline and run the full required repository gates. This audit changes no engine implementation.

## Related observations in this audit

These rows share the investigated path. Very small or overlapping differences remain observations; the sizeable reproducible cases and profiles above motivate this issue.

| Workload | Observed JSSE slowdown |
|---|---:|
| jetstream/splay | 1.201x |
| diagnostic-1010/alloc-gc/bench_alloc-gc_splay_mini | 1.185x |
| diagnostic-1010/promises/bench_promises_queue_deep | 1.103x |
| jetstream-extra/threejs | 1.170x |

Raw samples, immutable generated workloads, all 110 sampling profiles, counters and paired falsification probes are retained under `docs/perf/2026-10-14/boa-head-audit/` locally. The pinned source/reproducer and inline evidence above make the finding independently reproducible.
