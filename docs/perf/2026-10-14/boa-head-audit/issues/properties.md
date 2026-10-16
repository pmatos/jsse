perf(property): bytecode named property access bypasses property ICs and repeatedly enters generic MOP/key paths

Richards is 1.564x Boa, hash-map 1.436x, and the public-property/getter/Proxy controls retain smaller to 1.78x gaps. Richards has 9.13% self samples in get_object_property_with_proxy_depth::<str>; hash-map has 7.77%, in addition to VM and call costs. The no-trap Proxy profile has generic Get 11.73%, PropertyMap lookup 6.29%, invoke_proxy_trap 3.74%, and symbol_key_to_jsvalue::<str> 3.57%. The ownKeys/descriptor case has key-cache lookup and intern each 4.04% plus descriptor/property allocation.

[Op::GetProp](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/bytecode/vm.rs#L1169) calls [member_get](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/bytecode/vm.rs#L428), which takes a string and enters the generic property MOP. It does not use the property IC probe/classification already present in [the AST member-access path](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/eval/access.rs#L738). Named stores already have an Atom-backed property key, but named reads still discard that prepared-key opportunity at this seam. Proxy fallback paths also convert keys into JS-visible values for traps.

Potential direction: give VM named access sites guarded property IC state and keep prepared JsPropertyKey values through the MOP. Start with ordinary own data properties, then safe prototype cases; preserve the rooted generic fallback before any getter or proxy user code runs. Separately measure prepared-key/JS-string reuse on Proxy/Reflect paths. The direct Reflect.set trap control is itself 1.473x Boa, so the reactive case should not all be blamed on Proxy dispatch.

## Measurements

JSSE HEAD `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`; Boa HEAD `f0789a20d87ad52eef6460953929c2b3cdbf8716`; AMD Ryzen AI 9 HX 370, Rust 1.98.1, Node v26.8.2. Both engines use their default fat-LTO release profiles; Boa includes its default allocator/Intl features. Binary snapshots are fixed. Three alternating fresh CLI processes per successful case, sequential on CPU 0; 180 s / 8 GiB limits. Background desktop load was present: diagnostic measurements, with every sample and its range retained. Ratios above 1 mean JSSE is slower. V8 ratios use throughput; other rows use process wall time.

| Workload | Metric | JSSE median | Boa median | JSSE slowdown |
|---|---|---:|---:|---:|
| jetstream/richards | process wall seconds | 0.754779 | 0.482655 | 1.564x |
| jetstream/hash-map | process wall seconds | 1.207650 | 0.840744 | 1.436x |
| jetstream/Box2D | process wall seconds | 0.454732 | 0.406448 | 1.119x |
| jetstream/delta-blue | process wall seconds | 0.496181 | 0.404280 | 1.227x |
| boa-scripts/properties/access | process wall seconds | 0.045700 | 0.038919 | 1.174x |
| diagnostic-1010/proxy-classes/bench_proxy-classes_getter_setter | process wall seconds | 0.312133 | 0.175148 | 1.782x |
| diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_ownkeys_has | process wall seconds | 0.368229 | 0.294789 | 1.249x |

JetStream rows are one complete workload iteration, including initialization/preparation and validation, per fresh process. They are not official JetStream scores or the default multi-iteration schedules. Timing uses the uninstrumented release binary; the separate release-nolto counter build is never used for speed ratios.

## Sampling and counter evidence

`jetstream/richards` self samples:

```text
31.56% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
9.13% jsse [.] <jsse::interpreter::Interpreter>::get_object_property_with_proxy_depth::<str>
6.72% jsse [.] <jsse::interpreter::Interpreter>::call_function_inner_impl
2.28% jsse [.] <jsse::interpreter::Interpreter>::bind_simple_parameters
2.15% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_with_var_prologue
1.77% jsse [.] <hashbrown::map::HashMap<jsse::types::JsPropertyKey, u32, foldhash::fast::RandomState>>::get::<[u8]>
1.65% jsse [.] <jsse::interpreter::Interpreter>::proxy_set_with_outcome_and_proxy_depth::<jsse::types::JsPropertyKey>
```

Counters: vm_ops=43688401, ast_work_units=1, gc_minor=0, gc_major=0, gc_ms=0, local_slot_misses=0, atom_intern_calls=593.

`diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_notrap` self samples:

```text
14.29% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
11.73% jsse [.] <jsse::interpreter::Interpreter>::get_object_property_with_proxy_depth::<str>
9.01% jsse [.] <jsse::interpreter::atom::Atom as jsse::interpreter::binding_map::BindingKey>::find_in
6.29% jsse [.] <hashbrown::map::HashMap<jsse::types::JsPropertyKey, u32, foldhash::fast::RandomState>>::get::<[u8]>
4.25% jsse [.] <alloc::sync::Arc<jsse::types::StrView>>::drop_slow
3.74% jsse [.] <jsse::interpreter::Interpreter>::invoke_proxy_trap
3.57% jsse [.] <jsse::interpreter::Interpreter>::symbol_key_to_jsvalue::<str>
```

Counters: vm_ops=16500022, ast_work_units=15, gc_minor=0, gc_major=0, gc_ms=0, local_slot_misses=0, atom_intern_calls=90.

## Reproducer

The measured fixture is `diagnostic-1010/proxy-classes/bench_proxy-classes_public_access`. A standalone form of its core workload is:

```js
"use strict";
class Acc {
    constructor() { this.a = 1; this.b = 2; this.c = 3; }
    run(n) {
        let s = 0;
        for (let i = 0; i < n; i++) {
            this.a = this.a + 1;
            s += this.a + this.b + this.c;
            this.c = s & 255;
        }
        return s;
    }
}
console.log("public_access", new Acc().run(600000));
```

Save as `/tmp/repro.js`, then run the same file with `taskset -c 0 /path/to/jsse /tmp/repro.js` and `taskset -c 0 /path/to/boa /tmp/repro.js`. Interleave at least three process runs and compare medians/checksums. For profiling use `perf record -F 499 -e cpu-clock:u --call-graph dwarf -- /path/to/jsse /tmp/repro.js`; keep instrumented counts separate from wall time.

## Acceptance / semantic constraints

Use existing property IC mutation/shadowing/prototype tests in default VM mode, accessor receiver tests, Proxy invariants/trap order, cross-realm cases and GC stress. Measure Richards/hash-map plus named own/prototype data reads and the Proxy controls; preserve exotic-object fallback.

Any implementation should retain the existing test262 pass baseline and run the full required repository gates. This audit changes no engine implementation.

## Related observations in this audit

These rows share the investigated path. Very small or overlapping differences remain observations; the sizeable reproducible cases and profiles above motivate this issue.

| Workload | Observed JSSE slowdown |
|---|---:|
| boa-scripts/properties/access | 1.174x |
| boa-scripts/v8-benches/deltablue | 1.221x |
| boa-scripts/v8-benches/raytrace | 1.103x |
| boa-scripts/v8-benches/richards | 1.693x |
| jetstream/Babylon | 1.002x |
| jetstream/Box2D | 1.119x |
| jetstream/delta-blue | 1.227x |
| jetstream/hash-map | 1.436x |
| jetstream/proxy-mobx | 1.053x |
| jetstream/richards | 1.564x |
| diagnostic-1010/proxy-classes/bench_proxy-classes_control_trap_direct | 1.472x |
| diagnostic-1010/proxy-classes/bench_proxy-classes_getter_setter | 1.782x |
| diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_get | 1.207x |
| diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_notrap | 1.111x |
| diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_ownkeys_has | 1.249x |
| diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_set | 1.116x |
| diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_set_plain | 1.244x |
| diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_vue_reactive | 1.319x |
| diagnostic-1010/proxy-classes/bench_proxy-classes_public_access | 1.210x |

Raw samples, immutable generated workloads, all 110 sampling profiles, counters and paired falsification probes are retained under `docs/perf/2026-10-14/boa-head-audit/` locally. The pinned source/reproducer and inline evidence above make the finding independently reproducible.

## Expanded inventory evidence

Expanded registry evidence: 3d-raytrace-SP is 1.070x Boa, n-body-SP 1.147x and esprima-next-wtb 1.093x. Raytrace self samples show PropertyMap hash lookup 7.14%, generic Get 6.97%; n-body shows generic Get 12.17%, get_own_property 5.15%, generic Set path 4.21%. Both are virtually fully native (only five AST work units), with 4 ms / zero GC in separate counter runs. Esprima shows property lookup 7.21%, generic Get 5.17%, but also 19,628,774 AST units against 157,349,633 VM ops, 559 ms GC and 3,285,324 atom interning calls. Its fallback-only bodies are an additional cost; the property profile supports this issue without attributing the entire gap to MOP/IC work.
