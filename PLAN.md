# Plan: remove fixed per-call work in `call_function_inner_impl` and make `arguments` objects cheap (#899)

Complexity: Large (7 files across eval/VM/IC/types/gc, GC-rooting sensitive). Baseline HEAD `80aa515e`.
Line numbers below are **current** (the issue's numbers are stale; #900/#943 moved code).

## Problem restated

Every JS call through `call_function_inner_impl` (`src/interpreter/eval.rs:5182`) pays work that is
unobservable on the common path: a SipHash lookup of `"this"` in the global env for sloppy
undefined-`this` calls, a second `"this"` lookup to fill the construct-only `last_call_this_value`
side channel, a full `JsFunction` clone (including the `name: Option<String>`, a malloc) from
`obj.borrow().callable.clone()`, a `declare("arguments")` String-key insert plus a
`DeferredCallArguments` Vec clone for functions that never mention `arguments`, and in the VM a
`split_off` Vec per call, a `Vec::with_capacity(max_stack)` per chunk entry and a std-SipHash lookup of
the body pointer in `IcStore::enter_body`. Separately `create_arguments_object` (`mod.rs:2283`) builds
an `arguments` object with one validating `define_own_property` + `i.to_string()` + `PropertyDescriptor`
per index, a `HashMap<String,(EnvRef,String)>` of mapped params and a dynamic
`Array.prototype[@@iterator]` property lookup, ~15k instr for 3 args; non-simple-parameter functions
re-walk the whole body for var names per call (`eval.rs:5812-5825`). Goal: remove all of this without
any observable change.

## Spec basis

(`spec/` is not checked out in this workspace — `git submodule update --init --depth 1 spec` before
implementing and confirm the clause numbers below, which are ES2025 numbering.)

- 10.2.1 `[[Call]]`; 10.2.1.1 PrepareForOrdinaryCall; 10.2.1.2 OrdinaryCallBindThis (undefined/null
  `this` → callee realm's `[[GlobalEnv]].[[GlobalThisValue]]`; primitives → ToObject; strict functions
  untouched) — governs the cached global `this` and the sloppy-`this` tests.
- 10.2.2 `[[Construct]]` (step: result of non-object return → `thisArgument`/`GetThisBinding`) —
  governs why `last_call_this_value` is only needed on construct.
- 10.2.11 FunctionDeclarationInstantiation steps 15-22 (argumentsObjectNeeded, which names suppress
  it, ordering relative to parameter binding, separate var Environment when
  `hasParameterExpressions`) — governs skipping the binding/object when not observable and the
  non-simple-params var-name copy.
- 10.4.4 Arguments Exotic Objects, 10.4.4.1-10.4.4.5 internal methods (`[[GetOwnProperty]]`,
  `[[DefineOwnProperty]]`, `[[Get]]`, `[[Set]]`, `[[Delete]]` with `[[ParameterMap]]`), 10.4.4.6
  CreateUnmappedArgumentsObject, 10.4.4.7 CreateMappedArgumentsObject (property creation order
  `length`, indices, `@@iterator` = **%Array.prototype.values%** intrinsic, `callee`), 10.4.4.7.1
  MakeArgGetter / 10.4.4.7.2 MakeArgSetter.
- 10.2.4 AddRestrictedFunctionProperties / %ThrowTypeError% (strict `callee` poison pill);
  17 Forbidden Extensions (sloppy-function `caller`/`arguments` own properties are the only
  implementation-defined surface; the engine's Annex-style accessor reading call frames must be
  preserved — `fn.arguments`/`fn.caller`).
- 13.3.6.1 CallExpression evaluation / 13.3.6.2 EvaluateCall and 13.3.5.1 NewExpression evaluation
  (argument evaluation order; operands stay live across `[[Call]]`/`[[Construct]]`),
  13.3.12 Meta Properties + 9.4.5 GetNewTarget (`new.target` undefined on `[[Call]]`, inherited by
  arrows), 9.4.4 ResolveThisBinding, and 19.2.1.1 PerformEval / 19.2.1.3
  EvalDeclarationInstantiation (direct eval can read `arguments`, so `uses_arguments` must stay true
  for it).
- Observable-equivalence argument: every change is a pure performance change *except* one spec
  deviation it fixes in passing (`@@iterator` must be the intrinsic %Array.prototype.values%, not the
  current value of `Array.prototype[@@iterator]`) and one latent bug it must fix first (arrow-function
  *parameter* defaults referencing `arguments` are not scanned by `func_uses_arguments`, `ast.rs:1094`+;
  today masked because `declare("arguments")` leaves an `undefined` binding — wrong, but it is the
  behaviour that skipping the declaration would turn into a ReferenceError).

## Assumptions

- The "≤ 2 mallocs per call in a leaf loop" criterion is split: this issue removes every allocation it
  *owns* (callable clone, Deferred Vec, `split_off`, per-chunk stack Vec, mapped-param maps). The
  remaining per-call allocations are the `String` binding keys (`"this"` and each parameter name) in
  `Environment.bindings: HashMap<String, Binding>`, which belong to **#898** (slot/atom-keyed
  environments; its issue states `this` becomes an `Environment` field). `fib(n)` and `add(a,b)` will
  therefore show 2 and 3 allocs/call after this PR and reach ≤ 2 only with #898. This is recorded in the
  PR; we do **not** add a competing `uses_this` analysis or change the `bindings` key type here.
  (A peer session is working #898 concurrently and edits the same function — keep commits small and
  confined to the lines named below to ease rebasing.)
- `Rc<UserFunctionData>` (issue proposal) would touch 47 `JsFunction::User` sites; we instead snapshot
  only the fields the call path needs (decision: smaller, local, no ADR).
- `valgrind` is absent on this machine. Acceptance numbers are produced with (a) a feature-gated heap
  allocation counter (slice 0) for the malloc criterion, (b) `perf-counters` op/BODY rows, and
  (c) callgrind on the reference box if available; if no instruction counts can be taken the PR says so
  and reports medians-of-5 wall time (`docs/perf/2026-10-10/data/microbench.py`) instead of claiming
  instr/call.

- **PR body must list criteria met vs deferred** (fib(22) instr/call ≥ 15%, ≤ 2 mallocs/call,
  `create_arguments_object` ≤ 3k, `bench_calls_args` ≤ 2.5×, test262 unchanged), mark any that could
  not be measured on this machine, and point to #898 for the binding-key allocations — not claim full
  closure.

## Files to touch

| File | Role | Lines of Interest |
|------|------|-------------------|
| `src/interpreter/eval.rs` | `call_function_inner_impl`, `bind_function_parameters`, `dispatch_body`, sloppy-`this` sites | 1898-1940, 4994-5031, 5182-5878 (5237 callable clone, 5244 `was_construct`, 5518/5721/7977 global-this lookups, 5745-5785 arguments, 5808-5829 body_env var names, 5847 last_call_this_value) |
| `src/interpreter/mod.rs` | `DeferredCallArguments`/`CallFrameArguments`, `create_arguments_object`, `materialize_call_frame_arguments`, new `vm_stack_pool`/`vm_ref_pool` Interpreter fields | 289-296, 563-599, 2237-2406 |
| `src/interpreter/types.rs` | `Realm` (add `array_prototype_values`, `:501` region), `JsFunction`, `ObjectKind::Arguments` + `ParameterMap` + `parameter_map*` users | 501, 1290-1380, 2217-2218, 2385, 2629-3560 |
| `src/interpreter/bytecode/vm.rs` | `take_call_operands`/`take_construct_operands`, `Op::Call`/`Op::Construct`, `run_chunk_with_var_prologue`, `run_chunk_inner` stack alloc | 68-100, 202-257, 556-675 |
| `src/interpreter/ic_store.rs` | `IcStore.index` SipHash map | 40-130, 285-310 |
| `src/interpreter/gc.rs` | frame-argument tracing, `ObjectKind::Arguments` tracing, test constructor | 472-495, 588, 1120, 1711 |
| `src/interpreter/builtins/array.rs` | `values_fn` creation: stash the intrinsic | 3217-3235 |
| `src/interpreter/builtins/reflect.rs`, `src/interpreter/eval/access.rs` | `[[Delete]]` users of `parameter_map_mut` | reflect.rs:260, access.rs:458 |
| `src/interpreter/key_intern.rs` | small fixed index-key table (or key pre-interning for fixed keys) | 1-80 |
| `src/interpreter/hoist_cache.rs` | reuse memoised `var_names` for non-simple params (read-only use) | 28-80 |
| `src/ast.rs` | `expr_uses_arguments` arrow-parameter gap | 1094-1215 |
| `src/lib.rs`, `src/perf_alloc.rs` [new], `src/interpreter/perf_counters.rs` | feature-gated heap-allocation counter + `heap_allocs` PERF row | lib.rs:4-10, perf_counters.rs:178 |
| `AGENTS.md` | "Execution Counters": document `heap_allocs` row | — |
| `docs/specs/2026-07-20-function-call-environment-pooling-design.md` | update design note (inline deferred args, field-copy call snapshot, pooled VM stacks) | — |
| `docs/perf/2026-10-11/899-call-fixed-work.md` [new] | before/after table (allocs/call, ops, wall medians) | — |
| `src/interpreter/tests.rs`, `src/interpreter/bytecode/tests.rs` | in-crate structural tests | tests.rs ~217, ~2856; bytecode/tests.rs ~1400, ~2092 |
| `tests/perf_counters_report_paths.rs`, `tests/gc_stress.rs` | heap_allocs row test; call-heavy GC-stress PROGRAM | — |
| `test262-extra/*.js` [new] | see Test surface table | — |

## TDD slices (each is one small commit and ends green: `cargo test`, targeted test262 dirs)

### Slice 0. Measure first: heap-allocation counter + baseline (tooling)
Separable and optional: keep it only if it stays a small self-contained change (one new file, one cfg-gated
registration, one report row); if it grows, drop it and report instruction/wall numbers only.
- **Test (red)**: `tests/perf_counters_report_paths.rs` — a new case asserting the report contains a
  `PERF\theap_allocs\t<n>` row with `n > 0` for a trivial script (fails: row absent).
- **Prod**: [new] `src/perf_alloc.rs` (declared `#[cfg(feature = "perf-counters")] mod perf_alloc;` in
  `src/lib.rs`, next to `pub mod interpreter;` — `src/main.rs` is a 3-line shim over `jsse::run_cli`) —
  a `GlobalAlloc` wrapper over `System` bumping a relaxed `AtomicU64`, registered with
  `#[global_allocator]` in `lib.rs` under the same cfg (feature-gated, so shipped binaries and test262
  runs are unaffected); `perf_counters.rs:178 report()` prints the row. Document the row in
  `AGENTS.md` "Execution Counters". Counter is process-wide, so benchmarks report
  `(allocs_after - allocs_startup) / calls`.
- **Measure**: build `--features perf-counters`, run `bench_calls_leaf.js` / `bench_calls_fib.js` /
  `bench_calls_args.js` (in `docs/perf/2026-10-10/repros/calls/`), record allocs/call into
  `docs/perf/2026-10-11/899-call-fixed-work.md` [new] (before table; after table filled in slice 10).

### Slice 1. Precondition: `uses_arguments` must see arrow-function *parameters*
- **Test (red on HEAD — verify first)**: [new] `test262-extra/arguments-object-arrow-parameter-default-reference.js`
  (`esid: sec-functiondeclarationinstantiation`): `function f(){ return ((x = arguments) => x)(); }`
  returns the arguments object (`.length` correct, `[0]` correct); same with `(x = eval("arguments")) => x`
  and a destructuring default `({a = arguments[0]}) => a`.
- **Prod**: `src/ast.rs` `expr_uses_arguments` `Expression::ArrowFunction` arm: also
  `a.params.iter().any(pattern_uses_arguments)`. Reuse `pattern_uses_arguments` (ast.rs:1200).
- If the test is already green on HEAD (parser/other path handles it), drop the prod change and keep the test.

### Slice 2. `last_call_this_value` only for `[[Construct]]`
- **Test (red)**: in-crate `src/interpreter/tests.rs` `non_construct_call_leaves_last_call_this_value_unset`:
  run a script calling a sloppy and a strict user function, assert `interp.last_call_this_value.is_none()`
  afterwards (field is `pub(super)`-level; add `#[cfg(test)]` accessor if needed). Existing
  `test262-extra/construct-tail-call-*.js`, `constructor-call-must-not-mutate-this.js` guard the construct path.
- **Prod**: `eval.rs:5847` → `if was_construct { self.last_call_this_value = func_env.borrow().get("this"); }`
  (`was_construct` already in scope from `eval.rs:5244`; it is `true` only for the four
  `call_constructor_body` sites at 6662/6880/6993/7212, which all set `calling_as_construct` first). Native
  save/restore at 5279-5281 stays. Comment the invariant at `last_call_this_value` (`mod.rs:289`).

### Slice 3. Cache the realm's global `this` value
- **Test (red)**: [new] `test262-extra/function-call-sloppy-this-global-substitution.js`
  (`esid: sec-ordinarycallbindthis`, `flags: [noStrict]`): undefined/null `this` → `globalThis`
  (sloppy), primitives wrapped (`typeof this === "object"`), strict untouched, cross-realm
  (`$262.createRealm()`: a function from the other realm called with undefined `this` sees *its own*
  realm's global). Characterisation (green-on-HEAD) — it is the guard for this slice.
- **Prod**: no new field — `Realm.global_object: Option<u64>` (`types.rs:501`, set in
  `install_global_object`, `builtins/global_object.rs:187`) is the same object the global env's `"this"`
  binding holds (`global_object.rs:~97`). Replace the three
  `self.realm().global_env.borrow().get("this")` sites — `eval.rs:5518` (async-gen), `5721` (main
  path, also reached by sync generators), `7977` (async fn) — with
  `self.realm().global_object.map(JsValue::object).unwrap_or(this_val.clone())`. (Realm is already the
  callee's realm: `current_realm_id` is switched at `eval.rs:5311` before these sites, matching
  OrdinaryCallBindThis's callee-realm rule.) Already a GC root; nothing new to trace. Before replacing, confirm every realm-creation
  path (`$262.createRealm`, ShadowRealm) goes through `create_global_object` → `install_global_object`
  so the global env's `"this"` always equals `realm.global_object` (module `this`, `mod.rs:2564`, is a
  separate binding and is untouched).

### Slice 4. Stop cloning the whole `JsFunction` per call
- **Test**: behaviour guards only (the change is allocation-only): existing call IC tests
  (`tests.rs:2856-3147`), `test262/built-ins/Function`, `language/expressions/call`, plus a new
  characterisation file for `new.target` (the arrow/`new.target` save-restore at `eval.rs:5243-5262`
  consumes the same data): [new] `test262-extra/function-call-new-target-call-construct-arrow.js`
  (`esid: sec-getnewtarget`): plain call → `undefined`; `new F` → `F`; arrow inside a constructor
  inherits; arrow created in a plain call sees `undefined`; `Reflect.construct(F, [], G)` → `G`; a plain
  call made *inside* a constructor body resets it to `undefined` and restores on return.
- **Prod**: no new public type. In `eval.rs:5237-5304`, take the borrow once and copy out only what the
  call path reads — for `JsFunction::User`: `params` (Rc), `body` (`Body`: Rc + `Copy` info), `closure`
  (EnvRef), the five flags, `uses_arguments`, `has_simple_params`, and `captured_new_target` (only when
  `is_arrow`); for `Native`: the `Rc<dyn Fn>` and nothing else. Neither `name: Option<String>`
  (a malloc per call, **including native calls**, whose name `String` is cloned today) nor
  `source_text` is cloned. A small private local enum in `eval.rs` is fine; `JsFunction` is unchanged
  (47 construction/match sites untouched). Leave `proxy.rs:38/93` clones alone.

### Slice 5. Skip the unobservable `arguments` binding; no heap Vec for the deferred frame args
- **Test (red/green mix)**: [new] `test262-extra/function-arguments-binding-not-observable-edge-cases.js`
  (`esid: sec-functiondeclarationinstantiation`, `flags: [noStrict]`): `function f(){ return typeof arguments; function arguments(){} }`
  → `"function"`; `function g(){ class arguments {} return typeof arguments; }`; `var arguments` in body;
  `function h(arguments){ return arguments; }`; nested arrow using `arguments`; arrow containing direct
  `eval("arguments")`; `with(o){ arguments }`; `function k(){ return k.arguments; }` still returns an
  arguments-like object with correct values while active and `null` when inactive; unused-arguments
  function called with 0/1/5/9 args then inspected through `fn.arguments` from a nested callee
  (exercises the inline/spill boundary); **Annex B block functions named `arguments`**:
  `function m(){ { function arguments(){} } return typeof arguments; }` called with args — and afterwards
  `!("arguments" in globalThis)` (no sloppy-global leak; `exec.rs:353` skips `arguments` in function
  scopes, but the test pins it); `function n(a = 1){ var arguments; return typeof arguments; }`;
  strict-mode variants of the same. Plus in-crate `tests.rs` structural test using a native probe
  that reads `interp.call_stack_envs.last()` and asserts there is **no** `"arguments"` binding when
  `uses_arguments == false`, and that `CallFrameArguments::Deferred` keeps ≤ 6 args inline.
- **Prod**:
  1. `eval.rs:5776-5785`: in the `else` branch (`!uses_arguments`) drop `declare("arguments", Var)`;
     keep building the `CallFrameArguments::Deferred` frame for sloppy functions (needed by
     `fn.arguments`, `function_constructors.rs:405-420`). Safety argument: `func_uses_arguments` is true for
     any identifier `arguments`, any declaration/param/pattern named `arguments`, any direct `eval(...)`
     call (incl. nested arrows, extends/computed keys), after slice 1 also arrow params.
  1b. Audit the `"arguments"` string sites in `src/interpreter` (`grep -rn '"arguments"'`:
     `exec.rs:348/353`, `eval.rs:5438/5627/5820/6131`, `mod.rs:1915/1920`, `literals.rs:1505`,
     `function_constructors.rs:330`) for any code assuming the function-scope binding exists;
     `eval.rs:6131` reads declared names only (direct eval ⇒ `uses_arguments`), `exec.rs:353` skips it.
  2. `mod.rs:563-583`: `DeferredCallArguments` stores `SmallVec<[JsValue; 6]>` (smallvec is already a
     dependency); `values()`/`to_vec()` iterate it; GC tracing at `gc.rs:481` unchanged in shape.
  3. Leave the generator/async-generator/async duplicates (`eval.rs:5375/5381`, `5567/5573`,
     `8025/8031`) untouched — not hot, and #898 will rewrite them (see Out of scope).

### Slice 6. VM: operand slice into `call_function`, pooled operand/ref stacks
- **Test (red)**: in-crate `bytecode/tests.rs`: `vm_operand_stacks_are_pooled_and_reentrant` — run a
  script whose native callback (`[1,2,3].map(f)`) re-enters the VM during an outer call, then assert
  `interp.vm_stack_pool.len() > 0` and ≤ the pool cap, and result correctness; plus parity tests
  `direct_call_*` (tests.rs:1853-2048) stay green. GC rooting guards:
  `pending_argument_survives_gc_during_later_call_argument` (2092) and new
  [new] `test262-extra/vm-call-operands-stay-rooted-during-nested-call.js` (callee runs `$262.gc()`
  while the only reference to an argument object is the caller's operand stack; also callee that
  throws, and callee that re-enters via `Array.prototype.map`).
- **Prod** (`vm.rs`):
  1. `Op::Call`: compute `base = stack.len() - argc - 2`; read `callee`/`this` as clones of
     `stack[base]`/`stack[base+1]` (cheap `JsValue` clones, needed anyway for the IC probe and the
     `call_function(&callee, &this, ..)` signature) and pass `&stack[base+2..]` as `args`; **keep the
     values on the stack** during the call (`stack` is a local, nested `run_chunk` gets its own); after
     the call pop `argc+2` entries via `pop_value` (reverse order ⇒ same LIFO unroot as
     `release_call_operands`, which is deleted). Strict `ReturnCall`: build the owned `Vec` with
     `to_vec()` only on that path. Same for `Op::Construct` (`take_construct_operands` deleted).
     `gc_bytecode_roots` bookkeeping is unchanged (values were never unrooted until release).
  2. `run_chunk_with_var_prologue`: take `stack`/`refs` Vecs from `interp.vm_stack_pool:
     Vec<Vec<JsValue>>` / `vm_ref_pool` (new fields, `mod.rs`), `clear()` and return them after
     `run_chunk_inner` (in the wrapper, since `run_chunk_inner` has many early returns); cap pool at
     64 buffers and drop buffers with `capacity() > 1024`. `run_chunk_inner` takes `&mut Vec`.
  3. Drop the unused `_this: JsValue` clone in `dispatch_body` (`eval.rs:1922`) if it is still unused.

### Slice 7. `IcStore.index` off SipHash
- **Test**: existing `ic_store.rs` unit tests (eviction, generation, shared handle) are the guard; add
  one asserting lookups stay correct after `evict_inactive_older_half` with the new map.
- **Prod**: `ic_store.rs:40`: `index: FxHashMap<*const Vec<Statement>, usize>` (`rustc_hash` already a
  dependency, same as `body_memo.rs:15`). Keys are `Rc` addresses, never script-controlled strings, so
  the HashDoS rationale of #393 (`docs/specs/2026-07-25-environment-binding-hashing-design.md`) does
  not apply — note that in the module comment. A per-Chunk cached handle is **not** planned: a
  stale-able handle needs eviction/generation revalidation and still has to bump `in_flight`/`last_used`
  (see Out of scope).

### Slice 8. Cheap `arguments` objects
- **Tests (characterisation, write first, green on HEAD except the iterator one)**:
  [new] `test262-extra/arguments-object-own-keys-descriptors-and-mapping.js`
  (`esid: sec-createmappedargumentsobject`, `flags: [noStrict]`): `Reflect.ownKeys` order
  (`"0".."n-1"`, `"length"`, `"callee"`, `Symbol.iterator`), descriptors of each, `Object.keys`
  (`length`/`callee`/iterator non-enumerable), `JSON.stringify(arguments)`, `Array.prototype.slice.call`,
  spread, aliasing both directions, aliasing broken by `delete`, by `defineProperty` accessor / by
  `writable:false`, duplicate parameter names (last wins), extra args beyond params unmapped, missing
  args not mapped, `arguments.length` write doesn't resize, `arguments` after the frame returned
  (escaped through closure) keeps its values and mapping; unmapped (strict, defaults, rest, destructuring)
  with `callee` poison pill identical `%ThrowTypeError%` getter/setter, non-configurable.
  [new] `test262-extra/arguments-object-iterator-is-intrinsic-array-values.js`
  (`esid: sec-createunmappedargumentsobject`): after `Array.prototype[Symbol.iterator] = function(){…}`
  *and* after `delete Array.prototype[Symbol.iterator]`, `arguments[Symbol.iterator] ===
  originalValues`, and `[...arguments]` still works — **red on HEAD** (dynamic lookup).
  [new] in-crate `types.rs`/`tests.rs` unit test `arguments_object_uses_parameter_map_without_name_strings`
  (structural, see below).
- **Prod**:
  1. `Realm.array_prototype_values: Option<JsValue>` set in `builtins/array.rs:3217`; replace the
     `get_property_on_id(array_prototype, @@iterator)` at `mod.rs:2388-2402`.
  2. New `ParameterMap` newtype in `types.rs` replacing `HashMap<String,(EnvRef,String)>` in
     `ObjectKind::Arguments`: `{ env: EnvRef, params: Rc<Vec<Pattern>>, mapped: SmallVec<[u32; 8]> }`
     (indices still mapped). Keep the surface the existing users need (`get(&str) -> Option<(EnvRef-ref,
     &str name)>`, `remove(&str)`, `insert`, `is_empty`) so the ~10 `parameter_map()` call sites
     (`types.rs` 2637/2740/3055/3151/3178/3306/3505/3550, `reflect.rs:260`, `eval/access.rs:458`) and
     `gc.rs:588/1120` change minimally; the GC walker traces `env` only; fix `gc.rs:1711` constructor.
     Duplicate-name rule (only the last index with a given name stays mapped) computed once at creation
     with an O(n²) scan for n ≤ 16, `HashSet` above.
  3. `create_arguments_object`: signature takes `params: Option<&Rc<Vec<Pattern>>>` instead of
     `&[String]` (no per-call `Vec<String>` at `eval.rs:5747-5760`, `mod.rs:2249-2261`,
     `eval.rs:5352-5365`, `7999-8012`); build properties with `insert_property` +
     `PropertyDescriptor::data(..)` directly on the fresh ordinary object (spec: CreateDataProperty on
     a fresh extensible object ≡ a plain insert; order preserved: indices, `length`, `callee`,
     `@@iterator` as today — keep exact `property_order` via the same insertion order), index keys from a
     small pre-interned table (`key_intern.rs`; indices are deliberately not in the general cache —
     add a separate fixed `0..32` table, fall back to `i.to_string()` above); skip `class_name`
     String churn only if free (full removal is #909).
  4. Mapping rules to preserve exactly (from the exploration): mapped only when
     `has_simple_params && !is_strict && !env_strict`; index `i` mapped only if `i < argc` and it is the
     last occurrence of its param name; map dropped (not left empty-with-kind) when no index is mapped
     (`ObjectKind` stays `Ordinary`); unmapped `callee` accessor uses `realm().throw_type_error`
     (the per-call `create_thrower_function()` fallback must not run when the realm intrinsic exists).
     Note a live mapped object pins its env (`recycle_function_environment` requires
     `strong_count == 1`), so mapped-arguments calls do not reuse pooled envs — documented, not fixed
     here.
  5. **Cost-model caveat and gate.** A 3-arg object has six own properties (`length`, `0..2`,
     `callee`, `@@iterator`), above `PropertyMap` `INLINE_CAP = 4` (`property_map.rs:11`), so it spills
     into a SipHash `HashMap`; every `insert_property` also runs `intern_js_key` (thread-local
     `HashMap` lookup) and `property_order` reallocates as it grows. Replacing `define_own_property`
     removes validation only. Within this slice: (a) pre-size `property_order`/the map for
     `argc + 3` entries, (b) skip `intern_js_key` for the fixed keys (`length`, `callee`, index table,
     `@@iterator`) by using already-interned `JsPropertyKey`s held on the realm/thread-local, (c) measure.
     **Gate**: if `create_arguments_object` is still > 3k instr (or > ~10 heap allocs) for 3 args, land
     slice 8b: a dedicated `elements: Vec<JsValue>` on `ObjectKind::Arguments` (the issue's
     array-like store) read through the MOP sites already listed (`get_own_property_full`,
     `own_property_lookup`, `own_property_descriptor_lookup`, `define_own_property`, delete at
     `access.rs:458`/`reflect.rs:260`, own keys `property.rs:1876`, `numeric_index_fast_get`
     `property.rs:167`, GC `gc.rs:956/1120`), leaving `ObjectKind::Array` untouched. 8b is in scope if
     the gate trips.
  6. Target: `create_arguments_object` ≤ 3k instr for 3 args (callgrind, or alloc counter ≤ 4 allocs +
     object slot) and `bench_calls_args` ≤ 2.5× boa on the reference box.

### Slice 9. Non-simple parameters: no per-call body walk / HashSets
- **Test**: existing `language/function-code`, `language/arguments-object`, `eval-code/direct` dirs;
  add to the slice-5 file a case with default params + `var` redeclaring a param + `var arguments` +
  closures capturing both environments, called repeatedly (cache reuse) with different args.
- **Prod**: `eval.rs:5808-5829`: replace `collect_var_names_from_stmts` HashSet with
  `self.hoist_cache.analysis_for(&body).var_names` (already memoised per Body, `hoist_cache.rs:28-80`) and
  replace the per-call `param_names` HashSet by a `pattern_binds_name(p, name)` walk over `params`
  (no alloc). Apply the same swap at the async-generator site `eval.rs:5430-5445` only if it is a
  one-line change; otherwise leave (Out of scope).

### Slice 10. Close-out
- Re-run measurements, fill the "after" table in `docs/perf/2026-10-11/899-call-fixed-work.md`.
- Update `docs/specs/2026-07-20-function-call-environment-pooling-design.md` (deferred args inline,
  field-copy call snapshot, pooled VM stacks). `CONTEXT.md`: add "Deferred Call Arguments"
  only if they are used as names in code comments. No ADR (no cross-cutting decision beyond the
  recorded Assumptions).
- `git rm PLAN.md` before opening the PR (stage contract).

## Test surface

### test262 directories to run targeted (submodule is empty here: `git submodule update --init --depth 1 test262`)
`language/arguments-object/` (mapped/unmapped), `built-ins/Function/` (incl. `prototype/{apply,call,bind}`),
`language/expressions/{call,new,new.target,super}/`, `language/function-code/`,
`language/eval-code/direct/` (`arguments`-named files), `annexB/language/function-code/`,
`built-ins/Reflect/construct/`, `built-ins/ThrowTypeError/`, `language/expressions/{function,arrow-function}/`,
`language/statements/function/`, `language/expressions/object/method-definition/`, generator/async
dirs (shared call path). Spec behaviours **not** covered by test262 get new `test262-extra/` files
(flat dir, test262 header format):

| New file (`test262-extra/`) | Clause under test | Slice | Red on HEAD? |
|---|---|---|---|
| `arguments-object-arrow-parameter-default-reference.js` | 10.2.11 FunctionDeclarationInstantiation (arrow params see enclosing `arguments`) | 1 | expected yes |
| `function-call-sloppy-this-global-substitution.js` | 10.2.1.2 OrdinaryCallBindThis | 3 | no (guard) |
| `function-call-new-target-call-construct-arrow.js` | 9.4.5 GetNewTarget / 13.3.12 | 4 | no (guard) |
| `function-arguments-binding-not-observable-edge-cases.js` | 10.2.11 steps 15-22 (+ legacy `fn.arguments`) | 5, 9 | no (guard) |
| `vm-call-operands-stay-rooted-during-nested-call.js` | 13.3.6.2 EvaluateCall operand lifetime (GC) | 6 | no (guard, GC stress) |
| `arguments-object-own-keys-descriptors-and-mapping.js` | 10.4.4.1-10.4.4.7 | 8 | no (guard) |
| `arguments-object-iterator-is-intrinsic-array-values.js` | 10.4.4.6 / 10.4.4.7 `@@iterator` = %Array.prototype.values% | 8 | **yes** |

In-crate (`cargo test`): `non_construct_call_leaves_last_call_this_value_unset`,
`vm_operand_stacks_are_pooled_and_reentrant`, the arguments-binding probe (slice 5),
`arguments_object_uses_parameter_map_without_name_strings`, an IC-store eviction-with-Fx-map test;
`tests/perf_counters_report_paths.rs` `heap_allocs` row; `tests/gc_stress.rs` call-heavy PROGRAM.

### Commands and gates
- Per slice: `cargo build --release` (explicit long timeout, `-j` capped), `cargo test` (debug asserts
  incl. root-stack LIFO), targeted
  `uv run python scripts/run-test262.py test262/test/language/arguments-object/ test262/test/built-ins/Function/ test262/test/language/expressions/{call,new,new.target,super}/ test262/test/language/function-code/ test262/test/language/eval-code/direct/ test262/test/annexB/language/function-code/ test262/test/built-ins/Reflect/construct/ test262/test/built-ins/ThrowTypeError/`
  (initialise the submodule first: `git submodule update --init --depth 1 test262`).
- Before PR: full `uv run python scripts/run-test262.py` (no baseline update), `uv run python
  scripts/run-test262.py test262-extra/` normal and `--no-bytecode`, `JSSE_GC_STRESS=7` over
  `test262-extra/` both modes, `cargo build --profile release-checked` + `--binary
  target/release-checked/jsse` over `test262-extra/` and a 10% `--sample`, `uv run python
  scripts/run-custom-tests.py`, `./scripts/lint.sh`, `tests/gc_stress.rs` (add a call-heavy PROGRAM
  with nested calls and `arguments`).
- Library harness smoke (call/arguments heavy): `./scripts/run-library-tests.sh decimal.js` and `acorn`.
- Perf: the three reproducers + `bench_calls_{fib,nested}` through `microbench.py` median-of-5 (default
  and `--no-bytecode`), `perf-counters` build for alloc/op counts, never timing the instrumented build.

## Regression risk

- `test262-pass.txt` is unchanged by design; the only intended behaviour change is `@@iterator` of
  arguments objects (spec-conformant) and arrow-parameter `arguments` (spec-conformant) — both can only
  turn failures into passes; never `--update-baseline`.
- Hot paths touched: `call_function_inner_impl` (tree-walker + VM entry), VM `Op::Call`/`Op::Construct`,
  `run_chunk_with_var_prologue`, `with_ic_body`.
- GC rooting: operands now live on the VM Vec *and* in `gc_bytecode_roots` for the whole call (same
  rooting as today, just no Vec move); pooled stacks must be cleared on every exit path (wrapper);
  `ParameterMap` traces `env` and must not hold unrooted objects; `DeferredCallArguments` stays traced
  from `call_stack_frames` (`gc.rs:472-495`). Needs `JSSE_GC_STRESS=7` + `release-checked`.
- `ObjectKind::Arguments` payload type change: exhaustive matches in `gc.rs`
  (`trace_object_fields`) must be updated; `property.rs`/`types.rs` mapped-argument MOP paths are
  where semantic regressions would show (`language/arguments-object/mapped/*`).
- Skipping `declare("arguments")` relies on `func_uses_arguments` being conservative; any construct it
  misses becomes a ReferenceError/outer-scope read instead of `undefined`. Mitigated by slice 1 + the
  edge-case test, and by keeping the generator/async paths unchanged.
- Concurrent #898 edits the same function and `Environment` — rebase conflicts, not semantic ones.

## Out of scope

- Slot/atom-keyed environments, `this` as an `Environment` field, binding-key `String` allocations
  (**#898**); `Rc<UserFunctionData>` enum change; a `uses_this` analysis.
- Same-`arguments` cleanups in generator/async/async-generator call paths (rewritten by #898/#914).
- A per-Chunk/function cached IC body handle or merging `IcStore` and `BodyMemo` records.
- Dropping `JsObjectData.class_name: String` (#909), lazy function-object properties (#901), call-site
  IC changes, `bytecode_cache` per-object clone in `dispatch_body`.
- Any `test262-pass.txt` baseline movement.
