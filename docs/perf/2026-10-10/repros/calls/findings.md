# Theme `calls`: function-call, method-call and closure overhead

Investigated 2026-10-10 on buildbox3, jsse HEAD 2b621c5 (v0.11.0) vs boa 0.22.0. Repros: `bench_calls_*.js`; raw data in `data/`.

## Summary

- User-call cost: jsse ~6,958 instr (1.35 µs) vs boa ~2,260 (0.43 µs) on fib; time ratio 3.1x equals instruction ratio.
- ~45% of a call is the name-keyed environment: `HashMap<String,Binding>` built and torn down per call (10.5 SipHash computations, 6.3 malloc/free pairs). The VM still resolves identifiers by name through it.
- Closure creation is worst (5.0x; 5.9x with a realistic body): bytecode is cached per function OBJECT (recompiled per closure), `func_uses_arguments`/params clone repeat per closure, and `create_function` eagerly builds 5-6 own properties plus a `.prototype` object.
- Estimated share of the gap to boa that is call overhead: delta-blue ~87%, richards ~41%, cdjs ~33%, raytrace ~29%, Air ~27%, earley-boyer ~22%.
- Not the problem: method lookup (1.7x), call-site IC, GC root push/pop, depth guard, Completion plumbing.

## Gap table

Median of 5, microbench.py, buildbox3; checksums identical on both engines. Rows for args, closure_call, ctor and closure_create were re-run at loadavg 3.5-5 because the first pass was above 12.

| repro | jsse s | boa s | ratio | jsse --no-bytecode s | loadavg |
|---|---|---|---|---|---|
| bench_calls_closure_bigbody | 3.93 | 0.67 | 5.89 | 3.73 | 6.4 |
| bench_calls_closure_create | 2.83 | 0.57 | 4.98 | 2.98 | 5.0 |
| bench_calls_args | 3.78 | 0.92 | 4.12 | 5.19 | 3.5 |
| bench_calls_leaf | 3.68 | 1.17 | 3.15 | 5.64 | 9.8 |
| bench_calls_fib | 3.63 | 1.17 | 3.10 | 4.08 | 11.1 |
| bench_calls_nested | 2.63 | 0.87 | 3.02 | 3.73 | 8.3 |
| bench_calls_closure_call | 3.38 | 1.12 | 3.02 | 5.04 | 4.9 |
| bench_calls_global_loop (control, cross-cutting) | 1.72 | 0.87 | 1.98 | 1.57 | 10.4 |
| bench_calls_method | 3.88 | 2.27 | 1.71 | 5.69 | 9.3 |
| bench_calls_ctor (borderline) | 3.03 | 1.97 | 1.53 | 4.18 | 4.6 |

Per-call: fib(30) = 2,692,537 user calls, so jsse 1,348 ns/call and boa 434 ns/call. Callgrind instr/call (startup subtracted): jsse 6,958, boa 2,260.

Experiments (same protocol, not shipped):

| repro | HEAD | A: Fx hasher on Environment.bindings | C: fat LTO + cgu=1 |
|---|---|---|---|
| fib | 3.63 | 3.13 (-14%) | 3.33 (-8%) |
| leaf | 3.68 | 3.13 (-15%) | 3.53 (-4%) |
| closure_call | 3.38 | 2.73 (-19%) | 3.38 (0%) |
| nested | 2.63 | 2.22 (-16%) | 2.42 (-8%) |
| method | 3.88 | 3.58 (-8%) | 3.78 (-3%) |
| closure_create | 2.83 | 2.63 (-7%) | 2.58 (-9%) |

Callgrind Ir change for A: fib -18.8%, leaf -22.5%, closure_call -28.3%, method -14.8%, ctor -14.7%. For C: fib -4.2%, leaf -3.3%, closure_create -5.0%.

## Root causes (ranked)

### RC1. Name-keyed per-call environment (~45% of a call)

Callgrind fib(22), 57,313 calls (`data/callgrind-categories.txt`):

| category | instr/call | share |
|---|---|---|
| SipHash (std HashMap<String,..>) | 1,658 | 23.8% |
| VM dispatch (run_chunk_inner, push_value) | 1,110 | 16.0% |
| malloc/free | 1,027 | 14.8% |
| call_function_inner_impl and eval.rs | 703 | 10.1% |
| Environment get/reset/resolve_indirect | 696 | 10.0% |
| hashbrown insert/find/drop | 641 | 9.2% |
| alloc shims and mem* | 349 | 5.0% |

Per call: 6.5 `hash_one::<&str>` (lookups), 3.0 `hash_one::<&String>` (3 inserts), 1.0 hash of the body pointer (`IcStore::enter_body`); 6.3 mallocs and 6.3 frees.

Sources (src/interpreter/eval.rs, `call_function_inner_impl`):
- `acquire/recycle_function_environment` (:5646, :5802; mod.rs:2199, 2214). `reset_function_scope` (types.rs:1033) calls `bindings.clear()` and drops every String key; the pool only saves the Rc and bucket array (585 instr/call).
- `"this".to_string()` + insert (:5686): String alloc + SipHash per call.
- `declare("arguments", Var)` (:5729) runs for every non-arrow function even when `uses_arguments` is false (419 instr/call). Same branch: `DeferredCallArguments::new(args)` (:5732) clones args.
- `bind_function_parameters` (:4940): `name.clone()` + insert per parameter (438 instr/call).
- `self.realm().global_env.borrow().get("this")` (:5671) on every sloppy call with undefined this (~270 instr).
- `last_call_this_value = func_env.borrow().get("this")` (:5799) on every call, though only `[[Construct]]` reads it (~270 instr).
- `obj.borrow().callable.clone()` (:5183) clones the whole `JsFunction::User` including the `name` String (~150 instr + 1 malloc).
- VM: `LoadName` (vm.rs:272) probes `scope.bindings` by name; `LoadCalleeName` (vm.rs:301) always takes full `resolve_identifier` (eval.rs:7696), ~560 instr/call just to find `fib`.

Hasher swap alone is insufficient (A: -14..-19% time; boa still ~2.5x faster). PR #393 (`docs/specs/2026-07-25-environment-binding-hashing-design.md`) deliberately reverted Fx for script-controlled names, so the fix must remove string hashing from the call path, not swap the hasher.

Corroboration: delta-blue (callgrind, 2.27M calls, 8,273 instr/call all-in): SipHash 23.2%, Environment 13.7%, malloc/free 8.7%, hashbrown 7.8%.

### RC2. Compiled chunk and per-function metadata cached per closure object, not per AST

- `dispatch_body` (eval.rs:1801-1845) stores `BytecodeCacheState` in the callee object (types.rs:2012). Each closure from the same function expression is `Untried` and compiles again; failed compiles are retried per closure.
- Evidence: bigbody has 5,001 compiles for 5,000 closures. `compile_body` is 21.1% and `func_uses_arguments` 13.6% of instructions. JetStream `compile_ok`: doxbee-promise 40,019 (+40,009 failed), cdjs 36,070 (+6,024; one `expression:Array` body retried 6,017 times), Air 9,701, lazy-collections 7,906 (+7,991). Share on those workloads is small (~1-3% by estimate); large on closure-heavy code.
- `Expression::Function` (eval.rs:609-660, methods :6242): `Rc::new(f.params.clone())` (deep pattern clone), `func_uses_arguments` (full body walk), `params_are_simple`, name/source_text clones. All are pure functions of the AST.
- On bigbody the tree-walker is no slower than the VM (3.73 s vs 3.93 s, ~5%, single 5-run set).

### RC3. `create_function` eagerly materialises function-object properties

mod.rs:1799-1950: `length`, `name`, `caller`, `arguments`, `prototype` (5 inserts, over `PropertyMap::INLINE_CAP=4`, so it spills into a SipHash HashMap) plus a prototype object with `constructor`, `class_name` String, `function_realm_map.insert`. closure_create (39.2k instr/iter): `create_function` 13.1k per closure (`insert_property` 8.6k), GC safepoint 5.7k, compile 1.9k, `func_uses_arguments` 0.85k; malloc/free 29% of instructions.

### RC4. `arguments` object and non-simple params

- `create_arguments_object` (mod.rs:2262) is ~15k instr for 3 args (24.6% of bench_calls_args): one `define_own_property` per index with `i.to_string()` keys, plus `length`/`callee`/`@@iterator`.
- Non-simple param functions rebuild `HashSet<String>` var/param name sets every call (eval.rs:5764-5777; `collect_var_names_from_pattern` 2.9% of args).

### RC5. Smaller per-call items (1-3% each)

- `take_call_operands` (vm.rs:68) `split_off` allocates a Vec per call.
- `Vec::with_capacity(max_stack)` per chunk entry (vm.rs:255).
- `with_ic_body`/`enter_body` (ic_store.rs:78, :291) SipHashes the body pointer per call (~90 instr).
- Call-site IC (#432): the hit path skips only 3 predicate checks, about as cheap as the probe; no measurable benefit (net effect UNVERIFIED).

## Proposed work items

Sizes: S under 1 day, M 2-4 days, L 1-2 weeks. Estimates are instruction-based, not implemented.

### WI-1 (L) `perf(interpreter): slot/atom-keyed function environments without per-call hashing or allocation`

Related: #387 is the existing call-overhead epic and this is its open levers 1-2 (per-call String keys, slot-indexed lookup). #389 landed the Fx swap and #393 reverted it for HashDoS. Link them to avoid a duplicate issue.
- Scope: `Environment.bindings` (types.rs:844, ~132 direct `.bindings` uses across eval.rs, exec.rs, generator_runtime.rs, env_helpers.rs, gc.rs, vm.rs), `call_function_inner_impl`, `bind_function_parameters`, VM `LoadName`/`LoadCalleeName`/`ResolveName`.
- Approach: intern identifiers to an `Atom` (u32 or pointer-identity `Rc<str>`) at parse/compile time. The interner is the only place script-controlled strings are hashed (randomized hasher), so the #393 decision holds. Function scopes use `SmallVec<[(Atom, Binding); 8]>` (linear scan, pooled, no per-call allocations) spilling to an atom-keyed map. `this` becomes an `Environment` field. `arguments` is declared only when `uses_arguments` or direct eval is present. Optional step 2: compile-time slot resolution of locals when no eval/with.
- Expected: removes SipHash (24%), hashbrown (9%), ~4 of 6.3 mallocs, memcmp; ~3,100 of 6,958 instr/call. fib/leaf/nested/closure_call ratio ~3.0 → ~1.7-1.9.
- Risks: eval, `with`, module indirect bindings, TDZ, mapped `arguments` aliasing, generator/async state machines capturing `func_env`, GC tracing, `has_parameter_expressions` second env. Run full test262, test262-extra, `JSSE_GC_STRESS=7`, release-checked.
- Acceptance: callgrind fib(22) ≤ 4,200 instr/call; zero `hash_one::<&str>` per call in a call-only loop; bench_calls_{fib,leaf,nested,closure_call} ratio ≤ 1.9; delta-blue time ratio ≤ 1.7 (from 2.29); test262 pass list unchanged.

### WI-2 (M) `perf(interpreter): remove fixed per-call work in call_function_inner_impl`

1. Cache the realm's global `this` value on `Realm`.
2. Compute `last_call_this_value` only when `was_construct`.
3. Avoid the per-call full `JsFunction` clone (`Rc<UserFunctionData>` or clone only used fields).
4. Skip `declare("arguments")` and `DeferredCallArguments::new(args)` when `arguments` cannot be observed (`!uses_arguments`, no direct eval).
5. VM: pass the operand-stack slice to `call_function` instead of `split_off`; pool the operand-stack Vec.
6. Store the IC body handle on the chunk/function instead of re-hashing the body pointer.
- Expected: ~1,000-1,500 of 6,958 instr/call (15-20%).
- Risks: `new.target`, `fn.arguments`/`caller` legacy accessors, GC rooting of the borrowed args (must stay rooted in `gc_bytecode_roots` for the whole nested call).
- Acceptance: fib(22) instr/call down ≥ 15%; ≤ 2 mallocs per call in a leaf loop; test262 pass list unchanged.

### WI-3 (S-M) `perf(bytecode): cache compiled chunk and function metadata per function AST`

- Scope: `dispatch_body` (eval.rs:1801), `BytecodeCacheState` (types.rs:2012), function/arrow/method creation (eval.rs:609, :6242).
- Approach: share the `Untried/Compiled/Ineligible` cache across closures of one `Body` (a cell in `Body`, or key by `Rc::as_ptr(&body.statements)` like `IcStore`). Precompute `uses_arguments`, `has_simple_params` and a shared `Rc<Vec<Pattern>>` per AST node.
- Expected: bigbody ratio 5.9 → ~3.5 (~35% of its instructions); closure_create ~-8%; no more repeated failed compiles.
- Risks: low; chunks hold no per-closure state. Memory improves.
- Acceptance: `compile_ok` equals the number of distinct executed function ASTs (bigbody: 2, not 5,001); bigbody jsse ≤ 2.5 s on buildbox3.

### WI-4 (M) `perf(interpreter): lazily materialise function-object own properties`

- Scope: `create_function` (mod.rs:1799), `JsObjectData`/`PropertyMap`, property MOP.
- Approach: build `length`/`name`/`caller`/`arguments`/`prototype` (and the prototype object and `constructor`) on first own-property access, enumeration, delete or define, preserving key order. Cheaper first step: pre-interned keys, a per-realm descriptor template, `INLINE_CAP` 6.
- Expected: closure_create 5.0 → ~3.0.
- Risks: key order, `getOwnPropertyNames`, proxies of functions, class `prototype` attributes, generator `.prototype`, GC tracing. Needs `test262-extra` key-order tests.
- Acceptance: instr per closure creation ≤ 5k (from ~14.3k); `built-ins/Function`, `language/expressions/{function,arrow-function,class}`, `language/statements/{function,class}` unchanged.

### WI-5 (M) `perf(interpreter): cheap arguments objects and cached var-name sets`

- Scope: `create_arguments_object` (mod.rs:2262), eval.rs:5764-5777.
- Approach: build `arguments` from the args slice in one step (array-like element store instead of per-index `define_own_property` with String keys); cache var/param name collection per function AST (same cell as WI-3).
- Expected: bench_calls_args 4.1 → ~2.5.
- Risks: mapped aliasing, `callee`, `@@iterator`, strict poison pills, `Object.keys(arguments)`.
- Acceptance: `create_arguments_object` ≤ 3k instr for 3 args; ratio ≤ 2.5.

### WI-6 (S) `build: fat LTO + codegen-units=1 for the release profile`

- Measured: -4 to -9% time, -3 to -5% instructions; build time 2m20s → 5m00s on buildbox3 under load. `Cargo.toml` has no `[profile.release]` today.
- Risks: low; re-check the recursion-limit custom tests (stack frame sizes change). `panic=abort` was not measured: IC-store tests use `catch_unwind`.

## What did NOT reproduce / not the bottleneck

- Method-call and prototype lookup: bench_calls_method is 1.71x (boa is itself slow on methods); `member_get` is 16% of its instructions (`get_own_property_full` clones a `PropertyDescriptor`), and call overhead dominates. A `GetProp`/`LoadMethod` property IC would help but does not explain the 3x gap (property-access theme).
- Constructors: bench_calls_ctor is 1.53x; cost is `this.x = v` (`set_object_with_key`, ~1.9k instr per new-property store), not the call.
- Call IC (#432), `call_depth` bookkeeping (~15 instr), `Completion` plumbing (the `call_function` TailCall loop is ~40 instr; this answers #387 lever 3), operand root push/pop (150-230 instr/call): all small.
- Hasher alone (A): 14-19% on call-bound repros, not enough.
- VM vs tree-walker: `--no-bytecode` is 12-53% slower on call-bound repros, but no faster-by-much than the VM on closure_create/bigbody/global_loop (closure creation outpaces calls there).
- Global-scope loop variables: 1.98x on bench_calls_global_loop; a global-object property resolution cost distinct from call cost. Kept as a labelled control.

## Estimated share of call overhead in JetStream gaps

Per-call gap (1,348 − 434 ns, from fib) × `calls_to_user` from the perf-counters build. Real calls carry more args/locals, so these are lower bounds.

| benchmark | user calls | jsse s | boa s | gap s | est. call gap s | share |
|---|---|---|---|---|---|---|
| delta-blue | 2,266,466 | 4.20 | 1.83 | 2.37 | 2.07 | ~87% |
| richards | 2,024,357 | 6.89 | 2.42 | 4.47 | 1.85 | ~41% |
| cdjs | 2,878,080 | 16.40 | 8.50 | 7.90 | 2.63 | ~33% |
| raytrace | 2,031,202 | 11.37 | 4.89 | 6.48 | 1.86 | ~29% |
| Air | 298,315 | 1.70 | 0.69 | 1.01 | 0.27 | ~27% |
| earley-boyer | 779,625 | 5.17 | 1.89 | 3.28 | 0.71 | ~22% |

Remaining gap in richards (4 bodies bail on `update target`, i.e. `this.x++`, carrying 100% of its 6.6M tree-walker work units), raytrace (14 of 20 bails are `let/const`) and earley-boyer/lazy-collections (`binary op` bails) is bytecode-compiler coverage, a different theme. Counters are in `data/jetstream-perf-counters.txt`.

## Reproduction commands

```
R=/home/igalia/pmatos/dev/bench-workspace/run-2026-10-10
python3 $R/microbench.py \
  '{"jsse":["'$R'/jsse-head/target/release/jsse"],"boa":["'$R'/boa-v0.22-src/target/release/boa"],"jsse_nobc":["'$R'/jsse-head/target/release/jsse","--no-bytecode"]}' \
  <dir with bench_calls_*.js> out.json 5

# counters (perf-counters build; never time it)
cargo +nightly build --release --features perf-counters
target/release/jsse bench_calls_closure_create.js 2>&1 >/dev/null | grep -E '^(PERF|BAIL)'

# instruction-level profile (small iteration counts)
CARGO_PROFILE_RELEASE_DEBUG=line-tables-only cargo +nightly build --release
valgrind --tool=callgrind --callgrind-out-file=fib22.out target/release/jsse fib22.js
callgrind_annotate fib22.out | head -60

# experiments
# A: types.rs Environment.bindings / indirect_bindings -> rustc_hash::FxHashMap (as PR #389)
# C: CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 cargo +nightly build --release
```

Callgrind scale: fib22 57,313 calls; closure_create 10,000 iterations (loop 300000 → 10000); closure_call 40,000; leaf 40,000; method 10,000 (5 calls each); ctor 10,000 (2 constructs each); args 10,000; bigbody 5,000. Startup (empty script: 12.4M instr jsse, 40.3M boa) subtracted.
