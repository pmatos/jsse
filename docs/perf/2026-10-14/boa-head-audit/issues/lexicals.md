perf(bytecode): verified slots miss function lexicals and captured/global lexical bindings

The compiler's [frame layout](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/bytecode/compiler.rs#L3490) includes `arguments`, simple parameters, and `var` declarations, but function-top-level `let`/`const` are absent. Its [local-slot resolver](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/bytecode/compiler.rs#L479) can address explicitly opened block environments, so a `for (let i...)` counter may be fast while a function-top-level `let sum` still takes by-name resolution. Free/captured/global declarative bindings also repeatedly use [walk_scope_chain](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/global_access.rs#L81); the existing Global Data Fast Path covers global-object data properties.

The paired `loop-let`/`loop-var` programs differ only in declaration kind: JSSE is 1.540x Boa for `let`, but 0.917x for `var`. Both execute natively, without GC or AST hand-offs. A five-repeat follow-up moved only the constant `n=3000` from outside `main` into it: JSSE improved from 0.251274 to 0.222757 s, while Boa changed from 0.210888 to 0.209895 s. That narrows this independent free-binding gap from 1.191x to 1.061x. The original nested-loop profile spends 21.93% self samples in atom-key BindingMap lookup and 18.80% in scope walking, despite zero local-slot misses.

Potential direction: include lexical declarations in a verified frame binding plan, then add guarded `(environment hops, slot)` hints for captured and global declarative bindings. Validate atom identity, scope/lifetime and TDZ state; keep the generic path for invalidated layouts, `with`, eval and indirect bindings. This follows up the remaining scope described by #898/#905; it does not require changing identifier hashing policy.

## Measurements

JSSE HEAD `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`; Boa HEAD `f0789a20d87ad52eef6460953929c2b3cdbf8716`; AMD Ryzen AI 9 HX 370, Rust 1.98.1, Node v26.8.2. Both engines use their default fat-LTO release profiles; Boa includes its default allocator/Intl features. Binary snapshots are fixed. Three alternating fresh CLI processes per successful case, sequential on CPU 0; 180 s / 8 GiB limits. Background desktop load was present: diagnostic measurements, with every sample and its range retained. Ratios above 1 mean JSSE is slower. V8 ratios use throughput; other rows use process wall time.

| Workload | Metric | JSSE median | Boa median | JSSE slowdown |
|---|---|---:|---:|---:|
| audit-controls/loop-let | process wall seconds | 0.231108 | 0.150030 | 1.540x |
| boa-scripts/basic/nested-loop | process wall seconds | 0.030413 | 0.018712 | 1.625x |
| boa-scripts/basic/closure | process wall seconds | 1.398179 | 1.106146 | 1.264x |
| diagnostic-1013/bench_script_let_loop | process wall seconds | 0.237002 | 0.134013 | 1.769x |
| diagnostic-1010/bigint/bench_bigint_compare_numeric_control | process wall seconds | 0.345366 | 0.187424 | 1.843x |

JetStream rows are one complete workload iteration, including initialization/preparation and validation, per fresh process. They are not official JetStream scores or the default multi-iteration schedules. Timing uses the uninstrumented release binary; the separate release-nolto counter build is never used for speed ratios.

## Sampling and counter evidence

`audit-controls/loop-let` self samples:

```text
64.09% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
5.71% jsse [.] <jsse::interpreter::atom::Atom as jsse::interpreter::binding_map::BindingKey>::find_in
5.34% jsse [.] <jsse::interpreter::Interpreter>::get_identifier_value_by_ref_hinted::<jsse::interpreter::atom::Atom>
5.34% jsse [.] jsse::interpreter::global_access::walk_scope_chain::<jsse::interpreter::atom::Atom>
3.50% jsse [.] <jsse::interpreter::Interpreter>::resolve_identifier_ref_hinted::<jsse::interpreter::atom::Atom>
0.92% jsse [.] <jsse::interpreter::Interpreter>::gc_safepoint
0.74% jsse [.] <jsse::interpreter::types::Environment>::resolve_indirect_binding
```

Counters: vm_ops=50000019, ast_work_units=0, gc_minor=0, gc_major=0, gc_ms=0, local_slot_misses=0, atom_intern_calls=88.

`boa-scripts/basic/nested-loop` self samples:

```text
46.51% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
21.93% jsse [.] <jsse::interpreter::atom::Atom as jsse::interpreter::binding_map::BindingKey>::find_in
18.80% jsse [.] jsse::interpreter::global_access::walk_scope_chain::<jsse::interpreter::atom::Atom>
2.17% jsse [.] <jsse::interpreter::Interpreter>::gc_safepoint
1.20% jsse [.] core::ptr::drop_glue::<core::result::Result<jsse::types::NanBoxedValue, jsse::types::NanBoxedValue>>
```

Counters: vm_ops=5012065, ast_work_units=2, gc_minor=0, gc_major=0, gc_ms=0, local_slot_misses=0, atom_intern_calls=101.

## Reproducer

The measured fixture is `audit-controls/loop-let`. A standalone form of its core workload is:

```js
function main(n){let sum=0;for(let i=0;i<n;i++)sum+=i&31;return sum;}
console.log(main(5000000));
```

Save as `/tmp/repro.js`, then run the same file with `taskset -c 0 /path/to/jsse /tmp/repro.js` and `taskset -c 0 /path/to/boa /tmp/repro.js`. Interleave at least three process runs and compare medians/checksums. For profiling use `perf record -F 499 -e cpu-clock:u --call-graph dwarf -- /path/to/jsse /tmp/repro.js`; keep instrumented counts separate from wall time.

## Acceptance / semantic constraints

Retain TDZ errors, const-write RHS ordering, closure mutation visibility, shadowing, direct-eval behavior and per-iteration bindings. Gate on the paired var/let and free/local controls, plus lexical/captured-name test262 directories.

Any implementation should retain the existing test262 pass baseline and run the full required repository gates. This audit changes no engine implementation.

## Related observations in this audit

These rows share the investigated path. Very small or overlapping differences remain observations; the sizeable reproducible cases and profiles above motivate this issue.

| Workload | Observed JSSE slowdown |
|---|---:|
| boa-scripts/basic/closure | 1.264x |
| boa-scripts/basic/nested-loop | 1.625x |
| diagnostic-1010/bigint/bench_bigint_compare | 1.641x |
| diagnostic-1010/bigint/bench_bigint_compare_numeric_control | 1.843x |
| diagnostic-1010/proxy-classes/bench_proxy-classes_map_lookup | 1.147x |
| diagnostic-1013/bench_script_let_loop | 1.769x |
| audit-controls/loop-let | 1.540x |

Raw samples, immutable generated workloads, all 110 sampling profiles, counters and paired falsification probes are retained under `docs/perf/2026-10-14/boa-head-audit/` locally. The pinned source/reproducer and inline evidence above make the finding independently reproducible.
