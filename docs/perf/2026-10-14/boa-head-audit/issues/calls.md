perf(function): compiled leaf calls still spend substantial time building and tearing down frames

The [compiled user-call path](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/eval.rs#L5486) still constructs a CallTarget, acquires/resets an environment, binds parameters/this, records CallFrame/CallFrameArguments, switches IC/body state, enters the VM, disposes resources and tears down the frame. Fibonacci's counter run has 2,692,537 compiled function invocations, zero AST work, zero collections, zero slot misses and only 85 atom interning calls. Recompilation, GC and per-call identifier hashing therefore do not explain that case.

Its uninstrumented profile has 14.67% self samples in call_function_inner_impl, 5.19% in CallFrame Vec push, 4.83% in DeferredCallArguments::new, 3.58% in bind_simple_parameters, and further frame/reset/dispatch costs. The comparator-sort profile has 18.95% in call_function_inner_impl; sorting's callback machinery itself is only 6.64% self. Both strict and sloppy leaf controls remain slower than Boa, so legacy sloppy arguments retention is only part of the cost.

Potential direction: cache a compiled invocation/binding plan per immutable literal, avoid revalidating immutable parameter metadata on every call, and investigate a guarded fast call path for small compiled functions. Preserve observable callers/arguments, realm switching, this, new.target and user-code reentry. This is a measured residual after #898/#899/#900, whose earlier larger-gap targets were met.

## Measurements

JSSE HEAD `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`; Boa HEAD `f0789a20d87ad52eef6460953929c2b3cdbf8716`; AMD Ryzen AI 9 HX 370, Rust 1.98.1, Node v26.8.2. Both engines use their default fat-LTO release profiles; Boa includes its default allocator/Intl features. Binary snapshots are fixed. Three alternating fresh CLI processes per successful case, sequential on CPU 0; 180 s / 8 GiB limits. Background desktop load was present: diagnostic measurements, with every sample and its range retained. Ratios above 1 mean JSSE is slower. V8 ratios use throughput; other rows use process wall time.

| Workload | Metric | JSSE median | Boa median | JSSE slowdown |
|---|---|---:|---:|---:|
| jsse/bench_fib | process wall seconds | 0.373312 | 0.301767 | 1.237x |
| diagnostic-1010/calls/bench_calls_leaf | process wall seconds | 0.339913 | 0.260312 | 1.306x |
| audit-controls/call-sloppy-local | process wall seconds | 0.451864 | 0.374231 | 1.207x |
| audit-controls/call-strict-local | process wall seconds | 0.408157 | 0.349320 | 1.168x |
| audit-controls/opmix-called | process wall seconds | 1.750740 | 1.426298 | 1.227x |
| diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_arr_sort_cmp | process wall seconds | 0.511916 | 0.453868 | 1.128x |

JetStream rows are one complete workload iteration, including initialization/preparation and validation, per fresh process. They are not official JetStream scores or the default multi-iteration schedules. Timing uses the uninstrumented release binary; the separate release-nolto counter build is never used for speed ratios.

## Sampling and counter evidence

`jsse/bench_fib` self samples:

```text
29.16% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
14.67% jsse [.] <jsse::interpreter::Interpreter>::call_function_inner_impl
5.19% jsse [.] <alloc::vec::Vec<jsse::interpreter::CallFrame>>::push_mut
4.83% jsse [.] <jsse::interpreter::DeferredCallArguments>::new
4.47% jsse [.] <jsse::interpreter::atom::Atom as jsse::interpreter::binding_map::BindingKey>::find_in
3.58% jsse [.] <jsse::interpreter::Interpreter>::bind_simple_parameters
3.40% jsse [.] jsse::interpreter::global_access::walk_scope_chain::<jsse::interpreter::atom::Atom>
```

Counters: vm_ops=18847764, ast_work_units=0, gc_minor=0, gc_major=0, gc_ms=0, local_slot_misses=0, atom_intern_calls=85.

`diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_arr_sort_cmp` self samples:

```text
19.73% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
18.95% jsse [.] <jsse::interpreter::Interpreter>::call_function_inner_impl
4.88% jsse [.] <jsse::interpreter::DeferredCallArguments>::new
4.10% jsse [.] <jsse::interpreter::Interpreter>::bind_simple_parameters
3.52% jsse [.] <alloc::vec::Vec<jsse::interpreter::CallFrame>>::push_mut
3.52% jsse [.] <jsse::interpreter::types::Environment>::reset_scope
3.12% jsse [.] <jsse::interpreter::Interpreter>::dispatch_body
```

Counters: vm_ops=19823806, ast_work_units=0, gc_minor=0, gc_major=0, gc_ms=0, local_slot_misses=0, atom_intern_calls=106.

## Reproducer

The measured fixture is `diagnostic-1010/calls/bench_calls_leaf`. A standalone form of its core workload is:

```js
function add(a, b) { return a + b; }
function main() {
  var s = 0;
  for (var i = 0; i < 2000000; i++) {
    s = add(s, i & 7);
  }
  return s;
}
console.log(main());
```

Save as `/tmp/repro.js`, then run the same file with `taskset -c 0 /path/to/jsse /tmp/repro.js` and `taskset -c 0 /path/to/boa /tmp/repro.js`. Interleave at least three process runs and compare medians/checksums. For profiling use `perf record -F 499 -e cpu-clock:u --call-graph dwarf -- /path/to/jsse /tmp/repro.js`; keep instrumented counts separate from wall time.

## Acceptance / semantic constraints

Use leaf, recursive and sort-comparator controls, and functions with observable caller/arguments/this/new.target. Preserve all existing call and realm tests; remeasure real object-heavy workloads as well as the leaf microbenchmark.

Any implementation should retain the existing test262 pass baseline and run the full required repository gates. This audit changes no engine implementation.

## Related observations in this audit

These rows share the investigated path. Very small or overlapping differences remain observations; the sizeable reproducible cases and profiles above motivate this issue.

| Workload | Observed JSSE slowdown |
|---|---:|
| jsse/bench_fib | 1.237x |
| boa-scripts/basic/call-loop | 1.117x |
| boa-scripts/closures/invoke | 1.207x |
| jetstream/bigint-bigdenary | 1.199x |
| diagnostic-1010/bigint/bench_bigint_bigdenary_numeric_control | 1.206x |
| diagnostic-1010/bigint/bench_bigint_bigdenary_real | 1.208x |
| diagnostic-1010/calls/bench_calls_closure_call | 1.238x |
| diagnostic-1010/calls/bench_calls_fib | 1.267x |
| diagnostic-1010/calls/bench_calls_leaf | 1.306x |
| diagnostic-1010/calls/bench_calls_method | 1.040x |
| diagnostic-1010/calls/bench_calls_nested | 1.093x |
| diagnostic-1010/strings-arrays/bench_strings-arrays_arr_sort_cmp | 1.030x |
| diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_arr_sort_cmp | 1.128x |
| audit-controls/args-parameters | 1.238x |
| audit-controls/call-sloppy-captured | 1.232x |
| audit-controls/call-sloppy-local | 1.207x |
| audit-controls/call-strict-captured | 1.190x |
| audit-controls/call-strict-local | 1.168x |
| audit-controls/opmix-called | 1.227x |
| audit-controls/params-simple | 1.309x |

Raw samples, immutable generated workloads, all 110 sampling profiles, counters and paired falsification probes are retained under `docs/perf/2026-10-14/boa-head-audit/` locally. The pinned source/reproducer and inline evidence above make the finding independently reproducible.

## Expanded inventory evidence

Expanded registry evidence: SunSpider crypto-md5 is 1.216x Boa, crypto-sha1 1.246x. They have 36,402,764 / 50,664,850 VM ops, only five AST work units, zero GC/slot misses and 369 / 275 atom calls. Self samples include VM dispatch 37.55% / 42.68%, call_function_inner_impl 8.68% / 9.60%, DeferredCallArguments::new 3.58% / 3.05%, bind_simple_parameters 3.02% / 2.59% and CallFrame push 1.51% / 2.29%. These support the same call/frame hypothesis alongside #1030's dispatch work; they do not indicate slow cryptographic native primitives.
