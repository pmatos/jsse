perf(vm): fully native arithmetic loops still trail Boa after removing fallback, hashing and GC costs

The isolated opcode-mix arithmetic case is 1.282x Boa; the original mixed benchmark's aggregate hid this because JSSE wins its typed-array and labeled-control cases. Numeric integer arithmetic is 1.38x and the Number counterpart of the BigInt operator loop is 1.368x.

The isolated arithmetic counter run has **240,024,083 VM opcodes**, zero AST work, zero fallback executions, zero collections and flat identifier hashing. Sampling puts 80.31% self in run_chunk_inner and 10.72% in its local-operation closure, with 3.70% in gc_safepoint. Some tiny helpers are identically folded by fat LTO into symbols named after unrelated closures; those symbol names must not be interpreted as generator execution in this generator-free program.

Potential root cause: residual dispatch, stack/local-slot read/write and numeric conversion work per native operation. [number_binop](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/bytecode/vm.rs#L64) and [binary_arm](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/bytecode/vm.rs#L898) use f64-backed Numbers; bitwise operations route through [ToInt32/ToUint32](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/types.rs#L1254). The current data establishes that the hot path is native VM work, not which instruction-level change will close the gap.

Potential next step: annotate run_chunk_inner's sampled assembly and compare dispatch/local-access/number-conversion blocks; test a bounded fused/register or int32-specialized operation change against this fixture. Keep the existing safepoints. Do not extrapolate a dispatcher-wide speedup from a single microbenchmark or time an instrumented build.

## Measurements

JSSE HEAD `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`; Boa HEAD `f0789a20d87ad52eef6460953929c2b3cdbf8716`; AMD Ryzen AI 9 HX 370, Rust 1.98.1, Node v26.8.2. Both engines use their default fat-LTO release profiles; Boa includes its default allocator/Intl features. Binary snapshots are fixed. Three alternating fresh CLI processes per successful case, sequential on CPU 0; 180 s / 8 GiB limits. Background desktop load was present: diagnostic measurements, with every sample and its range retained. Ratios above 1 mean JSSE is slower. V8 ratios use throughput; other rows use process wall time.

| Workload | Metric | JSSE median | Boa median | JSSE slowdown |
|---|---|---:|---:|---:|
| audit-controls/opmix-arith | process wall seconds | 1.048314 | 0.817411 | 1.282x |
| diagnostic-1010/numeric/bench_numeric_intarith | process wall seconds | 0.222893 | 0.161888 | 1.377x |
| diagnostic-1010/bigint/bench_bigint_ops_vm_numadd | process wall seconds | 0.116855 | 0.085420 | 1.368x |
| diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_ctl_scalar_fn | process wall seconds | 0.185346 | 0.152431 | 1.216x |

JetStream rows are one complete workload iteration, including initialization/preparation and validation, per fresh process. They are not official JetStream scores or the default multi-iteration schedules. Timing uses the uninstrumented release binary; the separate release-nolto counter build is never used for speed ratios.

## Sampling and counter evidence

`audit-controls/opmix-arith` self samples:

```text
80.31% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
3.70% jsse [.] <jsse::interpreter::Interpreter>::gc_safepoint
```

Counters: vm_ops=240024083, ast_work_units=0, gc_minor=0, gc_major=0, gc_ms=0, local_slot_misses=0, atom_intern_calls=121.

## Reproducer

The measured fixture is `audit-controls/opmix-arith`. A standalone form of its core workload is:

```js
function arith(n) {
  var r0 = 0, r1 = 1, r2 = 2;
  for (var i = 0; i < n; i++) {
    r0 = (r0 + 4) | 0;
    r1 = (r0 + r1) | 0;
    r1 = r1 & 65535;
    r2 = r1 >> 2;
    r0 = (r2 + r0) | 0;
    r0 = r0 & 65535;
  }
  return r0 + r1 + r2;
}

console.log(arith(10000000));
```

Save as `/tmp/repro.js`, then run the same file with `taskset -c 0 /path/to/jsse /tmp/repro.js` and `taskset -c 0 /path/to/boa /tmp/repro.js`. Interleave at least three process runs and compare medians/checksums. For profiling use `perf record -F 499 -e cpu-clock:u --call-graph dwarf -- /path/to/jsse /tmp/repro.js`; keep instrumented counts separate from wall time.

## Acceptance / semantic constraints

Retain arithmetic checksums and all Number edge cases (NaN, infinities, signed zero, large ToInt32 inputs). Measure arithmetic, typed-array, call and mixed controls separately, then real JetStream workloads; run the normal full conformance gate for any VM implementation change.

Any implementation should retain the existing test262 pass baseline and run the full required repository gates. This audit changes no engine implementation.

## Related observations in this audit

These rows share the investigated path. Very small or overlapping differences remain observations; the sizeable reproducible cases and profiles above motivate this issue.

| Workload | Observed JSSE slowdown |
|---|---:|
| diagnostic-1010/alloc-gc/bench_alloc-gc_hashmap_js | 1.296x |
| diagnostic-1010/bigint/bench_bigint_ops_vm_numadd | 1.368x |
| diagnostic-1010/numeric/bench_numeric_bail_arrayliteral | 1.026x |
| diagnostic-1010/numeric/bench_numeric_bail_letconst | 1.021x |
| diagnostic-1010/numeric/bench_numeric_bail_memberupdate | 1.030x |
| diagnostic-1010/numeric/bench_numeric_bail_none | 1.019x |
| diagnostic-1010/numeric/bench_numeric_bail_switch | 1.030x |
| diagnostic-1010/numeric/bench_numeric_bail_throw | 1.041x |
| diagnostic-1010/numeric/bench_numeric_bail_typeof | 1.036x |
| diagnostic-1010/numeric/bench_numeric_intarith | 1.377x |
| diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_arr_sort_default | 1.004x |
| diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_ctl_scalar_fn | 1.216x |
| diagnostic-1010/strings-arrays/no-gap/fn-scope/bench_strings-arrays_str_keys_control | 1.160x |
| diagnostic-1013/bench_script_handoff_loop | 1.008x |
| audit-controls/opmix-arith | 1.282x |
| audit-controls/opmix-mixed | 1.051x |

Raw samples, immutable generated workloads, all 110 sampling profiles, counters and paired falsification probes are retained under `docs/perf/2026-10-14/boa-head-audit/` locally. The pinned source/reproducer and inline evidence above make the finding independently reproducible.

## Expanded inventory evidence

Expanded registry/block-scope evidence: block-let is 1.769x Boa and switch-let 1.608x. Counters are fully native (41,333,466 / 48,500,131 VM ops), zero AST/GC/slot misses and only 102 / 97 atom interning calls. Profiles show VM dispatch 56.26% / 69.30%, plus unwind_scopes, Environment::reset_scope, BindingMap::clear and recycle_block_environment. The [PushEnv/PopEnv path](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/bytecode/vm.rs#L1932) still acquires, initializes and recycles an Environment per lexical block/switch. Potential direction: specialize uncaptured lexical block storage/lifetime while preserving TDZ and closures; verify scope work separately from the arithmetic dispatch gap. The main opmix process aggregate is slightly faster than Boa, but its arith/called/mixed inner medians are 105/360/407 ms versus 82/281/402 ms, respectively. Mixed is a tiny observation. Profiles/counters for the whole workload and longer paired controls are retained.
