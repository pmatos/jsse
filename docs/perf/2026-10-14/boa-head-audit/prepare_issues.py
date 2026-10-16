#!/usr/bin/env python3
"""Prepare focused performance findings, with measured evidence and repros."""
import json
from pathlib import Path
from audit import HERE, ROOT, ensure_workloads

REV='2ea6417535037ca58d78a433a72a2a0e2dfb4d77'
def link(path,line):
    return f'https://github.com/pmatos/jsse/blob/{REV}/{path}#L{line}'

GROUPS = {
'lexicals': dict(title='perf(bytecode): verified slots miss function lexicals and captured/global lexical bindings',
 reps=['audit-controls/loop-let','boa-scripts/basic/nested-loop','boa-scripts/basic/closure','diagnostic-1013/bench_script_let_loop','diagnostic-1010/bigint/bench_bigint_compare_numeric_control'],
 profile=['audit-controls/loop-let','boa-scripts/basic/nested-loop'], repro='audit-controls/loop-let',
 cause=f'''The compiler's [frame layout]({link('src/interpreter/bytecode/compiler.rs',3490)}) includes `arguments`, simple parameters, and `var` declarations, but function-top-level `let`/`const` are absent. Its [local-slot resolver]({link('src/interpreter/bytecode/compiler.rs',479)}) can address explicitly opened block environments, so a `for (let i...)` counter may be fast while a function-top-level `let sum` still takes by-name resolution. Free/captured/global declarative bindings also repeatedly use [walk_scope_chain]({link('src/interpreter/global_access.rs',81)}); the existing Global Data Fast Path covers global-object data properties.

The paired `loop-let`/`loop-var` programs differ only in declaration kind: JSSE is 1.540x Boa for `let`, but 0.917x for `var`. Both execute natively, without GC or AST hand-offs. A five-repeat follow-up moved only the constant `n=3000` from outside `main` into it: JSSE improved from 0.251274 to 0.222757 s, while Boa changed from 0.210888 to 0.209895 s. That narrows this independent free-binding gap from 1.191x to 1.061x. The original nested-loop profile spends 21.93% self samples in atom-key BindingMap lookup and 18.80% in scope walking, despite zero local-slot misses.

Potential direction: include lexical declarations in a verified frame binding plan, then add guarded `(environment hops, slot)` hints for captured and global declarative bindings. Validate atom identity, scope/lifetime and TDZ state; keep the generic path for invalidated layouts, `with`, eval and indirect bindings. This follows up the remaining scope described by #898/#905; it does not require changing identifier hashing policy.''',
 acceptance='Retain TDZ errors, const-write RHS ordering, closure mutation visibility, shadowing, direct-eval behavior and per-iteration bindings. Gate on the paired var/let and free/local controls, plus lexical/captured-name test262 directories.'),
'calls': dict(title='perf(function): compiled leaf calls still spend substantial time building and tearing down frames',
 reps=['jsse/bench_fib','diagnostic-1010/calls/bench_calls_leaf','audit-controls/call-sloppy-local','audit-controls/call-strict-local','audit-controls/opmix-called','diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_arr_sort_cmp'],
 profile=['jsse/bench_fib','diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_arr_sort_cmp'],repro='diagnostic-1010/calls/bench_calls_leaf',
 cause=f'''The [compiled user-call path]({link('src/interpreter/eval.rs',5486)}) still constructs a CallTarget, acquires/resets an environment, binds parameters/this, records CallFrame/CallFrameArguments, switches IC/body state, enters the VM, disposes resources and tears down the frame. Fibonacci's counter run has 2,692,537 compiled function invocations, zero AST work, zero collections, zero slot misses and only 85 atom interning calls. Recompilation, GC and per-call identifier hashing therefore do not explain that case.

Its uninstrumented profile has 14.67% self samples in call_function_inner_impl, 5.19% in CallFrame Vec push, 4.83% in DeferredCallArguments::new, 3.58% in bind_simple_parameters, and further frame/reset/dispatch costs. The comparator-sort profile has 18.95% in call_function_inner_impl; sorting's callback machinery itself is only 6.64% self. Both strict and sloppy leaf controls remain slower than Boa, so legacy sloppy arguments retention is only part of the cost.

Potential direction: cache a compiled invocation/binding plan per immutable literal, avoid revalidating immutable parameter metadata on every call, and investigate a guarded fast call path for small compiled functions. Preserve observable callers/arguments, realm switching, this, new.target and user-code reentry. This is a measured residual after #898/#899/#900, whose earlier larger-gap targets were met.''',
 acceptance='Use leaf, recursive and sort-comparator controls, and functions with observable caller/arguments/this/new.target. Preserve all existing call and realm tests; remeasure real object-heavy workloads as well as the leaf microbenchmark.'),
'parameters': dict(title='perf(function): default parameters re-intern names and allocate a body environment on every call',
 reps=['audit-controls/params-simple','audit-controls/params-default-supplied','audit-controls/params-default-omitted','diagnostic-1010/bigint/bench_bigint_mulmod256'],
 profile=['audit-controls/params-default-supplied','diagnostic-1010/bigint/bench_bigint_mulmod256'],repro='audit-controls/params-default-supplied',
 cause=f'''The paired controls call `add(a,b)` or `add(a,b=1)` one million times. Even when `b` is always supplied, JSSE takes 0.2910 s versus Boa's 0.1432 s (2.033x); the simple-parameter form is 0.1858 versus 0.1420 s (1.309x). An omitted default is 2.179x Boa. Supplying the value rules out evaluation of the default initializer as the main additional cost.

The supplied-default counter run has **2,000,089 Atom::intern entries**, zero AST work, zero GC and zero fallback executions. Profiling gives Atom::intern 5.88% self and keyed string hashing another 3.03%, plus call and scope-walk overhead. [bind_function_parameters]({link('src/interpreter/eval.rs',5206)}) walks patterns through bind_pattern; the [non-simple call path]({link('src/interpreter/eval.rs',6030)}) additionally creates a fresh body Environment and consults the hoist analysis on every call. That body environment is dropped rather than recycled through the function-environment pool.

Potential direction: cache an atom-based parameter/binding plan for non-simple formals and reuse an uncaptured body environment. Keep distinct parameter/body environments where default-initializer closures, eval, var shadowing or arguments semantics make the separation observable. The noble-style field helper additionally combines this overhead with BigInt operators and unhinted lexical names; it should not all be charged to BigInt arithmetic.''',
 acceptance='Compare simple, supplied-default and omitted-default forms at equal call counts. Preserve default evaluation order, parameter TDZ, var/parameter copies, captured initializer closures, rest/destructuring and arguments aliasing. Counter acceptance should keep name interning essentially flat for repeated calls to an unchanged literal.'),
'arguments': dict(title='perf(arguments): arguments-only calls still materialize property maps and re-intern the arguments binding',
 reps=['audit-controls/args-object','audit-controls/args-parameters','diagnostic-1010/calls/bench_calls_args'],
 profile=['audit-controls/args-object'],repro='audit-controls/args-object',
 cause=f'''The same one-million three-argument call loop is 1.594x Boa when the callee sums `arguments`, versus 1.238x for explicit parameters. The arguments-only counter run has 1,000,095 Atom::intern entries, 61 minor and 61 major collections, zero AST work and zero hand-offs.

[create_arguments_object]({link('src/interpreter/mod.rs',2385)}) eagerly builds length, indexed properties, callee and iterator descriptors in a PropertyMap. [The call path]({link('src/interpreter/eval.rs',5990)}) also declares the literal name `arguments` through the string API for every call. The profile includes PropertyMap append 3.11%, insert 2.42%, key interning 2.07%, numeric-key creation 1.96%, plus property lookup and ownership/destruction costs. Actual `arguments` use still pays these costs even though unused arguments objects were made cheap by #899.

Potential direction: reuse the pre-interned arguments atom and investigate dense/lazy arguments storage or a guarded frame-backed length/index read path, materializing descriptors when observable operations require them. Mapped arguments must retain parameter aliasing and descriptor changes; avoid treating them as ordinary arrays.''',
 acceptance='Validate strict and sloppy mapped/unmapped arguments, duplicate parameters, delete/defineProperty, iteration, callee/caller and parameter mutation. Use the paired arguments/parameter controls and the original mixed default-parameter repro separately.'),
'bigint': dict(title='perf(bigint): VM BigInt operators still pay generic dispatch and ownership costs in fully compiled loops',
 reps=['diagnostic-1010/bigint/bench_bigint_ops_vm_add','diagnostic-1010/bigint/bench_bigint_mulmod256_vm','diagnostic-1010/bigint/bench_bigint_shift_bitops','jetstream/bigint-noble-ed25519','jetstream/bigint-noble-secp256k1','jetstream/bigint-noble-bls12-381'],
 profile=['diagnostic-1010/bigint/bench_bigint_ops_vm_add'],repro='diagnostic-1010/bigint/bench_bigint_ops_vm_add',
 cause=f'''The four-operator BigInt VM loop is 1.726x Boa and has 26,400,035 VM opcodes, zero AST work, zero collections and flat identifier interning. The 256-bit multiply/modulo VM variant is 1.398x; noble ed25519/secp256k1 are 1.402x/1.354x in the cold workload audit. Both engines use num-bigint 0.5.x, so this finding is not a different arbitrary-precision library comparison.

In the operator-loop profile, binary_slow is 12.95% self, bigint_binary 4.77%, eval_binary 4.26%, Arc<BigInt> drop_slow 4.77%, and the VM dispatcher 31.52%. [binary_arm]({link('src/interpreter/bytecode/vm.rs',898)}) specializes Number pairs, while BigInt pairs pop operands and enter [binary_slow]({link('src/interpreter/bytecode/vm.rs',109)}) → [eval_binary/bigint_binary]({link('src/interpreter/eval.rs',2141)}). Results use [Arc-backed JsBigInt]({link('src/types.rs',673)}); Boa uses Rc-backed BigInts. Atomic ownership overhead is a hypothesis, not an isolated causal measurement.

Potential direction: add native BigInt-pair VM arms and reduce redundant operand ownership/refcount transitions, retaining the existing exact BigInt operations. Measure owned/in-place opportunities where uniqueness is known. Keep the default-parameter and lexical-binding gaps separate: the default-form mulmod repro is 2.096x, while the simpler VM form is 1.398x.''',
 acceptance='Preserve mixed Number/BigInt errors, exact division/remainder/sign rules, negative exponents, shift semantics and exception ordering. Compare the pure VM operator loop, Number control, 256-bit arithmetic and real noble workloads. Never time the counter build.'),
'gc': dict(title='perf(gc): nursery collections repeatedly trace large environment/object graphs in Three.js and deep promise queues',
 reps=['jetstream-extra/threejs','diagnostic-1010/promises/bench_promises_queue_deep','jetstream/splay','diagnostic-1010/alloc-gc/bench_alloc-gc_splay_mini'],
 profile=['jetstream-extra/threejs','diagnostic-1010/promises/bench_promises_queue_deep'],repro='diagnostic-1010/promises/bench_promises_queue_deep',
 cause=f'''Three.js is 1.170x Boa and the deep queue is 1.100x, despite other Promise cases favoring JSSE. The Three.js default-binary sampling profile has 22.65% self in gc_safepoint, 14.62% in trace_object_fields_in and 13.42% in collect_env_roots. The deep-queue profile has 28.84% in trace_object_fields_in and 25.44% in collect_env_roots, plus 11.84% in gc_safepoint.

Separate counter runs report Three.js **850 minor / 33 major collections**, 14,699 ms collected GC time of which 13,614 ms is minor; the queue reports **29 minor / 5 major**, 661 ms GC of which 609 ms is minor. These timings belong to the separate instrumented runs and are not divided by the default-binary wall times. Splay reports 141 minor / 7 major, with 157 of 246 GC ms spent in minors. The sampling profiles independently establish the hot paths.

[collect_env_roots]({link('src/interpreter/gc.rs',1550)}) walks bindings/this/disposal roots and parents, deduplicating environment pointers through a fresh HashSet. [Object tracing]({link('src/interpreter/gc.rs',1254)}) reaches captured environments and other old graph edges. Array and Map/Set card tracking (#912/#988) does not eliminate this environment traversal. Potential root cause: nursery work still scales with large live captured-environment/remembered-object graphs rather than just changed young edges.

Potential direction: measure environment visits and duplicate-parent probes per minor collection; evaluate epoch-based pointer visitation and generation-aware environment/remembered-field tracking. Any young-edge cache requires barriers on every environment mutation path, including VM slot stores, generator bindings, this/new.target and pooled-environment reset. Do not skip old environments without proving those barriers.''',
 acceptance='Retain the deep-queue checksum, Three.js workload and splay checks. GC/root changes require the existing checked-build root assertions, GC stress in both execution modes, and the full conformance baseline. Compare minor visits/time as well as wall time; the prior forced-major pacing problem is a different finding.'),
'allocation': dict(title='perf(runtime): closure and small-array construction still eagerly allocate metadata and property storage',
 reps=['boa-scripts/closures/create','jsse/bench_closure_create','diagnostic-1013/bench_closure_literal','diagnostic-1013/bench_array_literal','diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_arr_literals'],
 profile=['boa-scripts/closures/create','diagnostic-1013/bench_closure_literal'],repro='jsse/bench_closure_create',
 cause=f'''Closure creation is 1.870x Boa in Boa's own closure benchmark; the JSSE factory benchmark is 1.386x, closure literals 1.526x, and small array literals 1.200x. Compilation and literal metadata are shared: the Boa closure counter run has only three successful compiles for 100,001 compiled invocations, almost no AST work, and flat identifier hashing.

The closure-literal profile has create_function_with_name 10.04% self, PropertyMap destruction 7.03%, PropertyMap append 5.42%, alloc_object 3.61%, plus frame and allocator costs. [Current create_function_with_name]({link('src/interpreter/mod.rs',1914)}) still eagerly inserts length/name and, for applicable functions, caller/arguments/prototype; constructable functions allocate a separate prototype and constructor property. The current PropertyMap inline capacity is three, so the larger function layout also requires heap entry storage. The arrow benchmark avoids a prototype but still allocates the function, captured environment and metadata.

A five-repeat allocator falsification probe did **not** narrow the gap: default JSSE 0.066146 s, JSSE with LD_PRELOAD=libjemalloc.so.2 0.066581 s, Boa 0.035369 s. Constructor churn likewise changed 0.382905 → 0.384834 s. An allocator swap alone is not supported as the fix.

Potential direction: reduce construction work/allocation count through compact function metadata, shared immutable descriptor templates or lazy own-property/prototype materialization, and examine captured-environment volume. Keep observable property order/descriptors and prototype identity intact. The earlier #901 targets were looser; this report concerns the residual work visible on current HEAD, rather than per-closure recompilation.''',
 acceptance='Preserve Reflect.ownKeys/getOwnPropertyNames ordering, delete/define behavior, function/class/generator prototype attributes and GC roots. Compare arrow and constructable closures, small dense arrays and real workloads; increasing generic object inline capacity needs a whole-suite size/performance tradeoff measurement.'),
'properties': dict(title='perf(property): bytecode named property access bypasses property ICs and repeatedly enters generic MOP/key paths',
 reps=['jetstream/richards','jetstream/hash-map','jetstream/Box2D','jetstream/delta-blue','boa-scripts/properties/access','diagnostic-1010/proxy-classes/bench_proxy-classes_getter_setter','diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_ownkeys_has'],
 profile=['jetstream/richards','diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_notrap'],repro='diagnostic-1010/proxy-classes/bench_proxy-classes_public_access',
 cause=f'''Richards is 1.564x Boa, hash-map 1.436x, and the public-property/getter/Proxy controls retain smaller to 1.78x gaps. Richards has 9.13% self samples in get_object_property_with_proxy_depth::<str>; hash-map has 7.77%, in addition to VM and call costs. The no-trap Proxy profile has generic Get 11.73%, PropertyMap lookup 6.29%, invoke_proxy_trap 3.74%, and symbol_key_to_jsvalue::<str> 3.57%. The ownKeys/descriptor case has key-cache lookup and intern each 4.04% plus descriptor/property allocation.

[Op::GetProp]({link('src/interpreter/bytecode/vm.rs',1169)}) calls [member_get]({link('src/interpreter/bytecode/vm.rs',428)}), which takes a string and enters the generic property MOP. It does not use the property IC probe/classification already present in [the AST member-access path]({link('src/interpreter/eval/access.rs',738)}). Named stores already have an Atom-backed property key, but named reads still discard that prepared-key opportunity at this seam. Proxy fallback paths also convert keys into JS-visible values for traps.

Potential direction: give VM named access sites guarded property IC state and keep prepared JsPropertyKey values through the MOP. Start with ordinary own data properties, then safe prototype cases; preserve the rooted generic fallback before any getter or proxy user code runs. Separately measure prepared-key/JS-string reuse on Proxy/Reflect paths. The direct Reflect.set trap control is itself 1.473x Boa, so the reactive case should not all be blamed on Proxy dispatch.''',
 acceptance='Use existing property IC mutation/shadowing/prototype tests in default VM mode, accessor receiver tests, Proxy invariants/trap order, cross-realm cases and GC stress. Measure Richards/hash-map plus named own/prototype data reads and the Proxy controls; preserve exotic-object fallback.'),
'vm': dict(title='perf(vm): fully native arithmetic loops still trail Boa after removing fallback, hashing and GC costs',
 reps=['audit-controls/opmix-arith','diagnostic-1010/numeric/bench_numeric_intarith','diagnostic-1010/bigint/bench_bigint_ops_vm_numadd','diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_ctl_scalar_fn'],
 profile=['audit-controls/opmix-arith'],repro='audit-controls/opmix-arith',
 cause=f'''The isolated opcode-mix arithmetic case is 1.282x Boa; the original mixed benchmark's aggregate hid this because JSSE wins its typed-array and labeled-control cases. Numeric integer arithmetic is 1.38x and the Number counterpart of the BigInt operator loop is 1.368x.

The isolated arithmetic counter run has **240,024,083 VM opcodes**, zero AST work, zero fallback executions, zero collections and flat identifier hashing. Sampling puts 80.31% self in run_chunk_inner and 10.72% in its local-operation closure, with 3.70% in gc_safepoint. Some tiny helpers are identically folded by fat LTO into symbols named after unrelated closures; those symbol names must not be interpreted as generator execution in this generator-free program.

Potential root cause: residual dispatch, stack/local-slot read/write and numeric conversion work per native operation. [number_binop]({link('src/interpreter/bytecode/vm.rs',64)}) and [binary_arm]({link('src/interpreter/bytecode/vm.rs',898)}) use f64-backed Numbers; bitwise operations route through [ToInt32/ToUint32]({link('src/types.rs',1254)}). The current data establishes that the hot path is native VM work, not which instruction-level change will close the gap.

Potential next step: annotate run_chunk_inner's sampled assembly and compare dispatch/local-access/number-conversion blocks; test a bounded fused/register or int32-specialized operation change against this fixture. Keep the existing safepoints. Do not extrapolate a dispatcher-wide speedup from a single microbenchmark or time an instrumented build.''',
 acceptance='Retain arithmetic checksums and all Number edge cases (NaN, infinities, signed zero, large ToInt32 inputs). Measure arithmetic, typed-array, call and mixed controls separately, then real JetStream workloads; run the normal full conformance gate for any VM implementation change.'),
'generators': dict(title='perf(generator): state payload execution and resume bookkeeping retain a 2x gap on lazy/manual iteration',
 reps=['jetstream/lazy-collections','diagnostic-1010/generators/bench_generators_manual_next','diagnostic-1010/generators/bench_generators_lazy_collections','diagnostic-1010/generators/bench_generators_pipeline','diagnostic-1010/generators/bench_generators_simple_yield','jetstream/sync-fs'],
 profile=['diagnostic-1010/generators/bench_generators_manual_next','jetstream/lazy-collections'],repro='diagnostic-1010/generators/bench_generators_manual_next',
 cause=f'''Lazy collections is 2.012x Boa, manual generator driving 2.338x, the reduced lazy pipeline 2.050x and a composed generator pipeline 1.59x. The manual counter run has 4,800,021 VM opcodes alongside 4,400,009 AST work units; its acc state-machine body accounts for 3,600,002 AST units across 2,000,004 state steps. Function-invocation compiled counters alone would conceal this work.

Manual iteration's profile includes generator_next_state_machine_impl 7.24%, eval_expr 4.63%, string-key BindingMap lookup 5.23%, scope reconciliation 2.21% and iterator-result creation 2.41%. Lazy collections has generator_next_state_machine_impl 9.88%, sync_generator_scope_stack 3.95%, reconcile_scope_stack 2.80% and sync_generator_for_of_stack 2.47%, plus AST/name-resolution work. GC is a small share in these cases; the manual counter run reports 9 ms GC.

[The driver]({link('src/interpreter/eval/generator_runtime.rs',1124)}) invokes [exec_state_machine_body]({link('src/interpreter/exec.rs',27)}), whose implementation unconditionally enters exec_body_inner and the tree-walker. State terminator evaluation and scope/for-of stack synchronization also recur per resume. Previous machine caching, terminator-clone fixes and intrinsic raw iterator steps (#914/#917/#918/#955) leave this residual path.

Potential direction: compile yield-free state payloads and terminator expressions with an appropriate state execution goal, and reduce repeated scope/try/for-of synchronization when state shape has not changed. Preserve completion routing and externally parked GC roots; a normal Function chunk's default-return behavior is not a drop-in replacement for a state payload. Manual .next() still requires a visible result object, whereas intrinsic for-of may use the existing raw-step protocol.''',
 acceptance='Use manual next(sent), for-of, pipelines, yield*, early return/throw/finally, IteratorClose and async suspension ordering. Keep GC stress and both execution modes covered. Validate compiled/AST work counts per state as well as real lazy-collections wall time.'),
}

def category(name):
    n=name.lower()
    if 'boa-validation/' in n or n.startswith('boa-engine/') or '/intl/' in n:return None
    if 'splay' in n or 'queue_deep' in n or n=='jetstream-extra/threejs':return 'gc'
    if '/generators/' in n or n in ['jetstream/lazy-collections','jetstream/sync-fs']:return 'generators'
    if n.endswith('args-object') or 'bench_calls_args' in n:return 'arguments'
    if 'params-default' in n or n.endswith('bench_bigint_mulmod256'):return 'parameters'
    if 'compare_numeric_control' in n or n.endswith('bench_bigint_compare') or n in ['boa-scripts/basic/closure','boa-scripts/basic/nested-loop'] or n.endswith(('loop-let','bench_script_let_loop','bench_proxy-classes_map_lookup')):return 'lexicals'
    if 'bigdenary' in n:return 'calls'
    if 'bigint' in n and 'numadd' not in n:return 'bigint'
    if any(s in n for s in ['closure_create','closure_bigbody','closure_literal','closures/create','field_arrow_fn','array_literal','arr_literals']):return 'allocation'
    if 'sort_cmp' in n or any(s in n for s in ['bench_fib','bench_calls_','/call-','params-simple','args-parameters','opmix-called','closures/invoke','basic/call-loop','bench_closures']):return 'calls'
    if '/proxy-classes/' in n or '/properties/' in n or any(s in n for s in ['richards','hash-map','box2d','delta-blue','deltablue','raytrace','proxy-mobx','babylon']):return 'properties'
    return 'vm'

def value_row(r):
    j,b=r['engines']['jsse'],r['engines']['boa']
    if r['metric']=='inverse_v8_throughput_score':
        return f"| {r['name']} | V8 throughput score | {j['v8_score_median']:.3f} | {b['v8_score_median']:.3f} | {r['jsse_over_boa']:.3f}x |"
    return f"| {r['name']} | process wall seconds | {j['median_s']:.6f} | {b['median_s']:.6f} | {r['jsse_over_boa']:.3f}x |"

def main():
    ensure_workloads()
    rows=json.loads((HERE/'summary.json').read_text());rs={r['name']:r for r in rows}
    manifest={m['name']:m for m in json.loads((HERE/'manifest.json').read_text())}
    assignment={r['name']:category(r['name']) for r in rows if r.get('jsse_over_boa',0)>1}
    (HERE/'finding-categories.json').write_text(json.dumps(assignment,indent=2)+'\n')
    registry=[]
    for key,g in GROUPS.items():
        text=[g['title'],'',g['cause'],'','## Measurements','',
          'JSSE HEAD `'+REV+'`; Boa HEAD `f0789a20d87ad52eef6460953929c2b3cdbf8716`; AMD Ryzen AI 9 HX 370, Rust 1.98.1, Node v26.8.2. Both engines use their default fat-LTO release profiles; Boa includes its default allocator/Intl features. Binary snapshots are fixed. Three alternating fresh CLI processes per successful case, sequential on CPU 0; 180 s / 8 GiB limits. Background desktop load was present: diagnostic measurements, with every sample and its range retained. Ratios above 1 mean JSSE is slower. V8 ratios use throughput; other rows use process wall time.','',
          '| Workload | Metric | JSSE median | Boa median | JSSE slowdown |','|---|---|---:|---:|---:|']
        text.extend(value_row(rs[n]) for n in g['reps'] if n in rs and 'jsse_over_boa' in rs[n])
        text += ['', 'JetStream rows are one complete workload iteration, including initialization/preparation and validation, per fresh process. They are not official JetStream scores or the default multi-iteration schedules. Timing uses the uninstrumented release binary; the separate release-nolto counter build is never used for speed ratios.','', '## Sampling and counter evidence','']
        for n in g['profile']:
            slug=n.replace('/','__');p=HERE/'profiles'/(slug+'.self.txt')
            text += ['`'+n+'` self samples:','','```text']
            lines=[]
            for line in p.read_text().splitlines():
                if '%' in line and '[.]' in line and '0x' not in line and '::{closure#' not in line:
                    lines.append(' '.join(line.split()).split(' - ')[0])
            text.extend(lines[:7]);text+=['```','']
            p=HERE/'profiles'/(slug+'.counters.txt');d={a[1]:a[2] for line in p.read_text().splitlines() if (a:=line.split('\t'))[0]=='PERF'}
            text += ['Counters: '+', '.join(k+'='+d[k] for k in ['vm_ops','ast_work_units','gc_minor','gc_major','gc_ms','local_slot_misses','atom_intern_calls'] if k in d)+'.','']
        src=(HERE/manifest[g['repro']]['path']).read_text()
        # Remove historical timing comments from the carried-forward repros.
        while src.lstrip().startswith('//'):
            src=src.lstrip().split('\n',1)[1]
        if key=='vm':
            src=src[src.index('function arith(n)'):src.index('function elem(n)')]+'console.log(arith(10000000));\n'
        text += ['## Reproducer','', 'The measured fixture is `'+g['repro']+'`. A standalone form of its core workload is:','','```js',src.strip(),'```','',
          'Save as `/tmp/repro.js`, then run the same file with `taskset -c 0 /path/to/jsse /tmp/repro.js` and `taskset -c 0 /path/to/boa /tmp/repro.js`. Interleave at least three process runs and compare medians/checksums. For profiling use `perf record -F 499 -e cpu-clock:u --call-graph dwarf -- /path/to/jsse /tmp/repro.js`; keep instrumented counts separate from wall time.','',
          '## Acceptance / semantic constraints','',g['acceptance'],'',
          'Any implementation should retain the existing test262 pass baseline and run the full required repository gates. This audit changes no engine implementation.','',
          '## Related observations in this audit','',
          'These rows share the investigated path. Very small or overlapping differences remain observations; the sizeable reproducible cases and profiles above motivate this issue.','',
          '| Workload | Observed JSSE slowdown |','|---|---:|']
        text.extend(f"| {n} | {rs[n]['jsse_over_boa']:.3f}x |" for n,c in assignment.items() if c==key)
        text += ['', 'Raw samples, immutable generated workloads, all 110 sampling profiles, counters and paired falsification probes are retained under `docs/perf/2026-10-14/boa-head-audit/` locally. The pinned source/reproducer and inline evidence above make the finding independently reproducible.']
        p=HERE/'issues'/(key+'.md');p.write_text('\n'.join(text)+'\n')
        registry.append(dict(key=key,title=g['title'],body_file=str(p),workloads=[n for n,c in assignment.items() if c==key]))
    (HERE/'issues-to-create.json').write_text(json.dumps(registry,indent=2)+'\n')
    for r in registry:print(r['key'],len(r['workloads']),Path(r['body_file']).stat().st_size,r['title'])

if __name__=='__main__':main()
