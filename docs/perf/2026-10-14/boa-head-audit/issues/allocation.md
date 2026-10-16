perf(runtime): closure and small-array construction still eagerly allocate metadata and property storage

Closure creation is 1.870x Boa in Boa's own closure benchmark; the JSSE factory benchmark is 1.386x, closure literals 1.526x, and small array literals 1.200x. Compilation and literal metadata are shared: the Boa closure counter run has only three successful compiles for 100,001 compiled invocations, almost no AST work, and flat identifier hashing.

The closure-literal profile has create_function_with_name 10.04% self, PropertyMap destruction 7.03%, PropertyMap append 5.42%, alloc_object 3.61%, plus frame and allocator costs. [Current create_function_with_name](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/mod.rs#L1914) still eagerly inserts length/name and, for applicable functions, caller/arguments/prototype; constructable functions allocate a separate prototype and constructor property. The current PropertyMap inline capacity is three, so the larger function layout also requires heap entry storage. The arrow benchmark avoids a prototype but still allocates the function, captured environment and metadata.

A five-repeat allocator falsification probe did **not** narrow the gap: default JSSE 0.066146 s, JSSE with LD_PRELOAD=libjemalloc.so.2 0.066581 s, Boa 0.035369 s. Constructor churn likewise changed 0.382905 → 0.384834 s. An allocator swap alone is not supported as the fix.

Potential direction: reduce construction work/allocation count through compact function metadata, shared immutable descriptor templates or lazy own-property/prototype materialization, and examine captured-environment volume. Keep observable property order/descriptors and prototype identity intact. The earlier #901 targets were looser; this report concerns the residual work visible on current HEAD, rather than per-closure recompilation.

## Measurements

JSSE HEAD `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`; Boa HEAD `f0789a20d87ad52eef6460953929c2b3cdbf8716`; AMD Ryzen AI 9 HX 370, Rust 1.98.1, Node v26.8.2. Both engines use their default fat-LTO release profiles; Boa includes its default allocator/Intl features. Binary snapshots are fixed. Three alternating fresh CLI processes per successful case, sequential on CPU 0; 180 s / 8 GiB limits. Background desktop load was present: diagnostic measurements, with every sample and its range retained. Ratios above 1 mean JSSE is slower. V8 ratios use throughput; other rows use process wall time.

| Workload | Metric | JSSE median | Boa median | JSSE slowdown |
|---|---|---:|---:|---:|
| boa-scripts/closures/create | process wall seconds | 0.070093 | 0.037429 | 1.873x |
| jsse/bench_closure_create | process wall seconds | 0.189865 | 0.136955 | 1.386x |
| diagnostic-1013/bench_closure_literal | process wall seconds | 0.334763 | 0.219326 | 1.526x |
| diagnostic-1013/bench_array_literal | process wall seconds | 0.321162 | 0.267709 | 1.200x |
| diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_arr_literals | process wall seconds | 0.235562 | 0.190370 | 1.237x |

JetStream rows are one complete workload iteration, including initialization/preparation and validation, per fresh process. They are not official JetStream scores or the default multi-iteration schedules. Timing uses the uninstrumented release binary; the separate release-nolto counter build is never used for speed ratios.

## Sampling and counter evidence

`boa-scripts/closures/create` self samples:

```text
22.15% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
3.95% jsse [.] <jsse::interpreter::Interpreter>::create_function_with_name
3.51% jsse [.] jsse::interpreter::global_access::walk_scope_chain::<jsse::interpreter::atom::Atom>
2.85% jsse [.] <jsse::interpreter::Interpreter>::call_function_inner_impl
2.85% jsse [.] <jsse::interpreter::Interpreter>::dispatch_body
2.85% libc.so.6 [.] malloc
2.41% jsse [.] core::ptr::drop_glue::<jsse::interpreter::property_map::PropertyMapInner>
```

Counters: vm_ops=2700298, ast_work_units=2, gc_minor=6, gc_major=6, gc_ms=10, local_slot_misses=0, atom_intern_calls=112.

`diagnostic-1013/bench_closure_literal` self samples:

```text
17.27% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
10.04% jsse [.] <jsse::interpreter::Interpreter>::create_function_with_name
7.03% jsse [.] core::ptr::drop_glue::<jsse::interpreter::property_map::PropertyMapInner>
6.02% jsse [.] <jsse::interpreter::Interpreter>::call_function_inner_impl
5.42% jsse [.] <jsse::interpreter::property_map::PropertyMap>::append
3.61% jsse [.] <jsse::interpreter::Interpreter>::alloc_object
3.01% jsse [.] <jsse::interpreter::Interpreter>::bind_simple_parameters
```

Counters: vm_ops=12000015, ast_work_units=0, gc_minor=91, gc_major=92, gc_ms=62, local_slot_misses=0, atom_intern_calls=96.

## Reproducer

The measured fixture is `jsse/bench_closure_create`. A standalone form of its core workload is:

```js
function makeAdder(x) { return function (y) { return x + y; }; }
function main() {
  var sum = 0;
  for (var i = 0; i < 300000; i++) { var add5 = makeAdder(i & 15); sum += add5(i & 255); }
  return sum;
}
console.log(main());
```

Save as `/tmp/repro.js`, then run the same file with `taskset -c 0 /path/to/jsse /tmp/repro.js` and `taskset -c 0 /path/to/boa /tmp/repro.js`. Interleave at least three process runs and compare medians/checksums. For profiling use `perf record -F 499 -e cpu-clock:u --call-graph dwarf -- /path/to/jsse /tmp/repro.js`; keep instrumented counts separate from wall time.

## Acceptance / semantic constraints

Preserve Reflect.ownKeys/getOwnPropertyNames ordering, delete/define behavior, function/class/generator prototype attributes and GC roots. Compare arrow and constructable closures, small dense arrays and real workloads; increasing generic object inline capacity needs a whole-suite size/performance tradeoff measurement.

Any implementation should retain the existing test262 pass baseline and run the full required repository gates. This audit changes no engine implementation.

## Related observations in this audit

These rows share the investigated path. Very small or overlapping differences remain observations; the sizeable reproducible cases and profiles above motivate this issue.

| Workload | Observed JSSE slowdown |
|---|---:|
| jsse/bench_closure_create | 1.386x |
| boa-scripts/closures/create | 1.873x |
| diagnostic-1010/calls/bench_calls_closure_bigbody | 1.373x |
| diagnostic-1010/calls/bench_calls_closure_create | 1.399x |
| diagnostic-1010/proxy-classes/bench_proxy-classes_field_arrow_fn | 1.299x |
| diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_arr_literals | 1.237x |
| diagnostic-1013/bench_array_literal | 1.200x |
| diagnostic-1013/bench_closure_literal | 1.526x |

Raw samples, immutable generated workloads, all 110 sampling profiles, counters and paired falsification probes are retained under `docs/perf/2026-10-14/boa-head-audit/` locally. The pinned source/reproducer and inline evidence above make the finding independently reproducible.

## Expanded inventory evidence

Expanded registry evidence: date-format-tofte-SP is 1.154x Boa. The profile includes PropertyMap destruction 4.68%, create_function_with_name 3.00%, PropertyMap append 1.87%, alloc_object 1.31% plus substantial allocator work. Counters show 2,964,419 VM ops, 208,006 AST units, 224,218 atom interning calls and 31 ms GC in the separate instrumented run. Its nested per-call closures and small object/string work exercise the construction paths here; lexical/AST work is also present, so metadata allocation is not claimed as the sole cause.
