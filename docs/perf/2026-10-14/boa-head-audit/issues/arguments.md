perf(arguments): arguments-only calls still materialize property maps and re-intern the arguments binding

The same one-million three-argument call loop is 1.594x Boa when the callee sums `arguments`, versus 1.238x for explicit parameters. The arguments-only counter run has 1,000,095 Atom::intern entries, 61 minor and 61 major collections, zero AST work and zero hand-offs.

[create_arguments_object](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/mod.rs#L2385) eagerly builds length, indexed properties, callee and iterator descriptors in a PropertyMap. [The call path](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/eval.rs#L5990) also declares the literal name `arguments` through the string API for every call. The profile includes PropertyMap append 3.11%, insert 2.42%, key interning 2.07%, numeric-key creation 1.96%, plus property lookup and ownership/destruction costs. Actual `arguments` use still pays these costs even though unused arguments objects were made cheap by #899.

Potential direction: reuse the pre-interned arguments atom and investigate dense/lazy arguments storage or a guarded frame-backed length/index read path, materializing descriptors when observable operations require them. Mapped arguments must retain parameter aliasing and descriptor changes; avoid treating them as ordinary arrays.

## Measurements

JSSE HEAD `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`; Boa HEAD `f0789a20d87ad52eef6460953929c2b3cdbf8716`; AMD Ryzen AI 9 HX 370, Rust 1.98.1, Node v26.8.2. Both engines use their default fat-LTO release profiles; Boa includes its default allocator/Intl features. Binary snapshots are fixed. Three alternating fresh CLI processes per successful case, sequential on CPU 0; 180 s / 8 GiB limits. Background desktop load was present: diagnostic measurements, with every sample and its range retained. Ratios above 1 mean JSSE is slower. V8 ratios use throughput; other rows use process wall time.

| Workload | Metric | JSSE median | Boa median | JSSE slowdown |
|---|---|---:|---:|---:|
| audit-controls/args-object | process wall seconds | 0.851030 | 0.533831 | 1.594x |
| audit-controls/args-parameters | process wall seconds | 0.194627 | 0.157222 | 1.238x |
| diagnostic-1010/calls/bench_calls_args | process wall seconds | 0.362549 | 0.196379 | 1.846x |

JetStream rows are one complete workload iteration, including initialization/preparation and validation, per fresh process. They are not official JetStream scores or the default multi-iteration schedules. Timing uses the uninstrumented release binary; the separate release-nolto counter build is never used for speed ratios.

## Sampling and counter evidence

`audit-controls/args-object` self samples:

```text
27.88% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
3.46% jsse [.] <jsse::interpreter::Interpreter>::get_object_property_with_proxy_depth::<str>
3.11% jsse [.] <jsse::interpreter::Interpreter>::call_function_inner_impl
3.11% jsse [.] <jsse::interpreter::property_map::PropertyMap>::append
2.88% jsse [.] core::ptr::drop_glue::<jsse::interpreter::property_map::PropertyMapInner>
2.65% jsse [.] <alloc::sync::Arc<[u8]>>::drop_slow
2.42% jsse [.] <jsse::interpreter::property_map::PropertyMap>::insert
```

Counters: vm_ops=58000015, ast_work_units=0, gc_minor=61, gc_major=61, gc_ms=62, local_slot_misses=0, atom_intern_calls=1000095.

## Reproducer

The measured fixture is `audit-controls/args-object`. A standalone form of its core workload is:

```js
function add(){var s=0;for(var j=0;j<arguments.length;j++)s+=arguments[j];return s;}
function main(){var sum=0;for(var i=0;i<1000000;i++)sum+=add(i,1,2);return sum;}console.log(main());
```

Save as `/tmp/repro.js`, then run the same file with `taskset -c 0 /path/to/jsse /tmp/repro.js` and `taskset -c 0 /path/to/boa /tmp/repro.js`. Interleave at least three process runs and compare medians/checksums. For profiling use `perf record -F 499 -e cpu-clock:u --call-graph dwarf -- /path/to/jsse /tmp/repro.js`; keep instrumented counts separate from wall time.

## Acceptance / semantic constraints

Validate strict and sloppy mapped/unmapped arguments, duplicate parameters, delete/defineProperty, iteration, callee/caller and parameter mutation. Use the paired arguments/parameter controls and the original mixed default-parameter repro separately.

Any implementation should retain the existing test262 pass baseline and run the full required repository gates. This audit changes no engine implementation.

## Related observations in this audit

These rows share the investigated path. Very small or overlapping differences remain observations; the sizeable reproducible cases and profiles above motivate this issue.

| Workload | Observed JSSE slowdown |
|---|---:|
| diagnostic-1010/calls/bench_calls_args | 1.846x |
| audit-controls/args-object | 1.594x |

Raw samples, immutable generated workloads, all 110 sampling profiles, counters and paired falsification probes are retained under `docs/perf/2026-10-14/boa-head-audit/` locally. The pinned source/reproducer and inline evidence above make the finding independently reproducible.
