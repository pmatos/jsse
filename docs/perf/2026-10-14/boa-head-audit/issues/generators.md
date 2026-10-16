perf(generator): state payload execution and resume bookkeeping retain a 2x gap on lazy/manual iteration

Lazy collections is 2.012x Boa, manual generator driving 2.338x, the reduced lazy pipeline 2.050x and a composed generator pipeline 1.59x. The manual counter run has 4,800,021 VM opcodes alongside 4,400,009 AST work units; its acc state-machine body accounts for 3,600,002 AST units across 2,000,004 state steps. Function-invocation compiled counters alone would conceal this work.

Manual iteration's profile includes generator_next_state_machine_impl 7.24%, eval_expr 4.63%, string-key BindingMap lookup 5.23%, scope reconciliation 2.21% and iterator-result creation 2.41%. Lazy collections has generator_next_state_machine_impl 9.88%, sync_generator_scope_stack 3.95%, reconcile_scope_stack 2.80% and sync_generator_for_of_stack 2.47%, plus AST/name-resolution work. GC is a small share in these cases; the manual counter run reports 9 ms GC.

[The driver](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/eval/generator_runtime.rs#L1124) invokes [exec_state_machine_body](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/exec.rs#L27), whose implementation unconditionally enters exec_body_inner and the tree-walker. State terminator evaluation and scope/for-of stack synchronization also recur per resume. Previous machine caching, terminator-clone fixes and intrinsic raw iterator steps (#914/#917/#918/#955) leave this residual path.

Potential direction: compile yield-free state payloads and terminator expressions with an appropriate state execution goal, and reduce repeated scope/try/for-of synchronization when state shape has not changed. Preserve completion routing and externally parked GC roots; a normal Function chunk's default-return behavior is not a drop-in replacement for a state payload. Manual .next() still requires a visible result object, whereas intrinsic for-of may use the existing raw-step protocol.

## Measurements

JSSE HEAD `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`; Boa HEAD `f0789a20d87ad52eef6460953929c2b3cdbf8716`; AMD Ryzen AI 9 HX 370, Rust 1.98.1, Node v26.8.2. Both engines use their default fat-LTO release profiles; Boa includes its default allocator/Intl features. Binary snapshots are fixed. Three alternating fresh CLI processes per successful case, sequential on CPU 0; 180 s / 8 GiB limits. Background desktop load was present: diagnostic measurements, with every sample and its range retained. Ratios above 1 mean JSSE is slower. V8 ratios use throughput; other rows use process wall time.

| Workload | Metric | JSSE median | Boa median | JSSE slowdown |
|---|---|---:|---:|---:|
| jetstream/lazy-collections | process wall seconds | 0.629330 | 0.312756 | 2.012x |
| diagnostic-1010/generators/bench_generators_manual_next | process wall seconds | 0.334718 | 0.142985 | 2.341x |
| diagnostic-1010/generators/bench_generators_lazy_collections | process wall seconds | 0.624972 | 0.304440 | 2.053x |
| diagnostic-1010/generators/bench_generators_pipeline | process wall seconds | 0.545202 | 0.341929 | 1.594x |
| diagnostic-1010/generators/bench_generators_simple_yield | process wall seconds | 0.382595 | 0.312079 | 1.226x |
| jetstream/sync-fs | process wall seconds | 0.666752 | 0.609395 | 1.094x |

JetStream rows are one complete workload iteration, including initialization/preparation and validation, per fresh process. They are not official JetStream scores or the default multi-iteration schedules. Timing uses the uninstrumented release binary; the separate release-nolto counter build is never used for speed ratios.

## Sampling and counter evidence

`diagnostic-1010/generators/bench_generators_manual_next` self samples:

```text
8.45% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
7.24% jsse [.] <jsse::interpreter::Interpreter>::generator_next_state_machine_impl
5.23% jsse [.] <str as jsse::interpreter::binding_map::BindingKey>::find_in
5.03% jsse [.] <jsse::interpreter::atom::Atom as jsse::interpreter::binding_map::BindingKey>::find_in
4.63% jsse [.] <jsse::interpreter::Interpreter>::eval_expr
2.82% libc.so.6 [.] malloc
2.41% jsse [.] <jsse::interpreter::Interpreter>::create_iter_result_object
```

Counters: vm_ops=4800021, ast_work_units=4400009, gc_minor=24, gc_major=25, gc_ms=9, local_slot_misses=0, atom_intern_calls=89.

`jetstream/lazy-collections` self samples:

```text
9.88% jsse [.] <jsse::interpreter::Interpreter>::generator_next_state_machine_impl
6.10% jsse [.] <str as jsse::interpreter::binding_map::BindingKey>::find_in
5.11% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
4.45% jsse [.] <jsse::interpreter::Interpreter>::eval_expr
3.95% jsse [.] <jsse::interpreter::Interpreter>::sync_generator_scope_stack
3.79% jsse [.] jsse::interpreter::global_access::walk_scope_chain::<alloc::string::String>
2.80% jsse [.] <jsse::interpreter::Interpreter>::reconcile_scope_stack
```

Counters: vm_ops=5025352, ast_work_units=8608402, gc_minor=3, gc_major=4, gc_ms=18, local_slot_misses=0, atom_intern_calls=539290.

## Reproducer

The measured fixture is `diagnostic-1010/generators/bench_generators_manual_next`. A standalone form of its core workload is:

```js
function* acc() {
  let total = 0;
  while (true) {
    const v = yield total;
    total = (total + v) | 0;
  }
}
const it = acc();
it.next();
let last = 0;
for (let i = 0; i < 400000; i++) last = it.next(i).value;
console.log("manual_next", last);
```

Save as `/tmp/repro.js`, then run the same file with `taskset -c 0 /path/to/jsse /tmp/repro.js` and `taskset -c 0 /path/to/boa /tmp/repro.js`. Interleave at least three process runs and compare medians/checksums. For profiling use `perf record -F 499 -e cpu-clock:u --call-graph dwarf -- /path/to/jsse /tmp/repro.js`; keep instrumented counts separate from wall time.

## Acceptance / semantic constraints

Use manual next(sent), for-of, pipelines, yield*, early return/throw/finally, IteratorClose and async suspension ordering. Keep GC stress and both execution modes covered. Validate compiled/AST work counts per state as well as real lazy-collections wall time.

Any implementation should retain the existing test262 pass baseline and run the full required repository gates. This audit changes no engine implementation.

## Related observations in this audit

These rows share the investigated path. Very small or overlapping differences remain observations; the sizeable reproducible cases and profiles above motivate this issue.

| Workload | Observed JSSE slowdown |
|---|---:|
| jetstream/lazy-collections | 2.012x |
| jetstream/sync-fs | 1.094x |
| diagnostic-1010/generators/bench_generators_lazy_collections | 2.053x |
| diagnostic-1010/generators/bench_generators_manual_next | 2.341x |
| diagnostic-1010/generators/bench_generators_pipeline | 1.594x |
| diagnostic-1010/generators/bench_generators_simple_yield | 1.226x |
| diagnostic-1010/generators/bench_generators_tokenizer | 1.027x |

Raw samples, immutable generated workloads, all 110 sampling profiles, counters and paired falsification probes are retained under `docs/perf/2026-10-14/boa-head-audit/` locally. The pinned source/reproducer and inline evidence above make the finding independently reproducible.
