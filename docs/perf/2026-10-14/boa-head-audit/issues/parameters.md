perf(function): default parameters re-intern names and allocate a body environment on every call

The paired controls call `add(a,b)` or `add(a,b=1)` one million times. Even when `b` is always supplied, JSSE takes 0.2910 s versus Boa's 0.1432 s (2.033x); the simple-parameter form is 0.1858 versus 0.1420 s (1.309x). An omitted default is 2.179x Boa. Supplying the value rules out evaluation of the default initializer as the main additional cost.

The supplied-default counter run has **2,000,089 Atom::intern entries**, zero AST work, zero GC and zero fallback executions. Profiling gives Atom::intern 5.88% self and keyed string hashing another 3.03%, plus call and scope-walk overhead. [bind_function_parameters](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/eval.rs#L5206) walks patterns through bind_pattern; the [non-simple call path](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/eval.rs#L6030) additionally creates a fresh body Environment and consults the hoist analysis on every call. That body environment is dropped rather than recycled through the function-environment pool.

Potential direction: cache an atom-based parameter/binding plan for non-simple formals and reuse an uncaptured body environment. Keep distinct parameter/body environments where default-initializer closures, eval, var shadowing or arguments semantics make the separation observable. The noble-style field helper additionally combines this overhead with BigInt operators and unhinted lexical names; it should not all be charged to BigInt arithmetic.

## Measurements

JSSE HEAD `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`; Boa HEAD `f0789a20d87ad52eef6460953929c2b3cdbf8716`; AMD Ryzen AI 9 HX 370, Rust 1.98.1, Node v26.8.2. Both engines use their default fat-LTO release profiles; Boa includes its default allocator/Intl features. Binary snapshots are fixed. Three alternating fresh CLI processes per successful case, sequential on CPU 0; 180 s / 8 GiB limits. Background desktop load was present: diagnostic measurements, with every sample and its range retained. Ratios above 1 mean JSSE is slower. V8 ratios use throughput; other rows use process wall time.

| Workload | Metric | JSSE median | Boa median | JSSE slowdown |
|---|---|---:|---:|---:|
| audit-controls/params-simple | process wall seconds | 0.185810 | 0.141999 | 1.309x |
| audit-controls/params-default-supplied | process wall seconds | 0.291043 | 0.143152 | 2.033x |
| audit-controls/params-default-omitted | process wall seconds | 0.306154 | 0.140527 | 2.179x |
| diagnostic-1010/bigint/bench_bigint_mulmod256 | process wall seconds | 0.345339 | 0.164740 | 2.096x |

JetStream rows are one complete workload iteration, including initialization/preparation and validation, per fresh process. They are not official JetStream scores or the default multi-iteration schedules. Timing uses the uninstrumented release binary; the separate release-nolto counter build is never used for speed ratios.

## Sampling and counter evidence

`audit-controls/params-default-supplied` self samples:

```text
26.02% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
10.87% jsse [.] <jsse::interpreter::Interpreter>::call_function_inner_impl
5.88% jsse [.] <jsse::interpreter::atom::Atom>::intern
5.88% jsse [.] jsse::interpreter::global_access::walk_scope_chain::<jsse::interpreter::atom::Atom>
3.03% jsse [.] <std::hash::random::RandomState as core::hash::BuildHasher>::hash_one::<&str>
2.85% jsse [.] <jsse::interpreter::hoist_cache::HoistCache>::analysis_for
2.67% jsse [.] <jsse::interpreter::DeferredCallArguments>::new
```

Counters: vm_ops=15000015, ast_work_units=0, gc_minor=0, gc_major=0, gc_ms=0, local_slot_misses=0, atom_intern_calls=2000089.

`diagnostic-1010/bigint/bench_bigint_mulmod256` self samples:

```text
9.84% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
7.09% jsse [.] jsse::interpreter::global_access::walk_scope_chain::<jsse::interpreter::atom::Atom>
6.50% jsse [.] <jsse::interpreter::atom::Atom as jsse::interpreter::binding_map::BindingKey>::find_in
5.71% jsse [.] num_bigint::biguint::division::div_rem_core
5.31% libc.so.6 [.] malloc
4.13% jsse [.] jsse::interpreter::bytecode::vm::binary_slow
3.74% jsse [.] <jsse::interpreter::Interpreter>::call_function_inner_impl
```

Counters: vm_ops=9150031, ast_work_units=450012, gc_minor=0, gc_major=0, gc_ms=0, local_slot_misses=0, atom_intern_calls=1350096.

## Reproducer

The measured fixture is `audit-controls/params-default-supplied`. A standalone form of its core workload is:

```js
function add(a,b=1){return a+b;}
function main(){var sum=0;for(var i=0;i<1000000;i++)sum+=add(i,1);return sum;}console.log(main());
```

Save as `/tmp/repro.js`, then run the same file with `taskset -c 0 /path/to/jsse /tmp/repro.js` and `taskset -c 0 /path/to/boa /tmp/repro.js`. Interleave at least three process runs and compare medians/checksums. For profiling use `perf record -F 499 -e cpu-clock:u --call-graph dwarf -- /path/to/jsse /tmp/repro.js`; keep instrumented counts separate from wall time.

## Acceptance / semantic constraints

Compare simple, supplied-default and omitted-default forms at equal call counts. Preserve default evaluation order, parameter TDZ, var/parameter copies, captured initializer closures, rest/destructuring and arguments aliasing. Counter acceptance should keep name interning essentially flat for repeated calls to an unchanged literal.

Any implementation should retain the existing test262 pass baseline and run the full required repository gates. This audit changes no engine implementation.

## Related observations in this audit

These rows share the investigated path. Very small or overlapping differences remain observations; the sizeable reproducible cases and profiles above motivate this issue.

| Workload | Observed JSSE slowdown |
|---|---:|
| diagnostic-1010/bigint/bench_bigint_mulmod256 | 2.096x |
| audit-controls/params-default-omitted | 2.179x |
| audit-controls/params-default-supplied | 2.033x |

Raw samples, immutable generated workloads, all 110 sampling profiles, counters and paired falsification probes are retained under `docs/perf/2026-10-14/boa-head-audit/` locally. The pinned source/reproducer and inline evidence above make the finding independently reproducible.
