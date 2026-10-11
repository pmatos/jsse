# Plan: atom-keyed function environments with verified local-slot ops (#898)

## Problem restated

A user function call costs ~6,960 instr (fib), ~45% of it in the name-keyed
`Environment.bindings: HashMap<String, Binding>` (std SipHash, `types.rs:847`):
per call ~6.5 `hash_one::<&str>` lookups, 3 `String`-allocating inserts (`this`,
`arguments`, each parameter), a `clear()` that frees every key, plus the
`chunk.var_names` probe+insert in `run_chunk_inner` (`bytecode/vm.rs:244-253`).
The bytecode VM resolves every identifier by name (`LoadName`, `ResolveName`,
`StoreResolvedName`, `LoadCalleeName`, `UpdateName`). A hasher swap is ruled out
(#393, `docs/specs/2026-07-25-environment-binding-hashing-design.md`): script
names must not be predictably hashed. This plan removes string hashing from the
call path by (1) interning identifiers to canonical `Atom`s whose hash is computed
once, by the *randomised* interner, (2) replacing the map with an
insertion-ordered vec plus a lazily built atom index, (3) making `this` an
`Environment` field and the `arguments` binding conditional, and (4) letting the
compiler emit `LoadLocal/StoreLocal/UpdateLocal` slot ops whose slot is only a
*hint* verified by atom identity at run time, so eval/`with`/removal/non-call-path
entry degrade to the atom lookup, never to a wrong answer.

## Spec basis

No JavaScript-visible change; the representation must keep honouring these
clauses of `spec/spec.html` (clause name, emu-clause id, line):
- Declarative Environment Records (`sec-declarative-environment-records`, :10233):
  HasBinding, CreateMutableBinding, InitializeBinding, SetMutableBinding,
  GetBindingValue, DeleteBinding.
- Function Environment Records (`sec-function-environment-records`, :10669),
  BindThisValue (`sec-bindthisvalue`, :10758): `this` binding status, TDZ for
  derived constructors.
- CreateImportBinding (`sec-createimportbinding`, :11350).
- GetIdentifierReference (`sec-getidentifierreference`, :11377), ResolveBinding
  (`sec-resolvebinding`, :11876): chain walk, `with` object records, unscopables.
- PrepareForOrdinaryCall (`sec-prepareforordinarycall`, :13332),
  OrdinaryCallBindThis (`sec-ordinarycallbindthis`, :13359).
- FunctionDeclarationInstantiation (`sec-functiondeclarationinstantiation`,
  :13719): parameter/var/`arguments` binding order, duplicate parameter names
  (last wins), `argumentsObjectNeeded`. The binding is observable only through an
  `arguments` identifier reference, direct `eval`, or the legacy
  `Function.prototype.arguments`; `func_uses_arguments` (`ast.rs:992`) already
  counts all of those (including `var arguments`, `ast.rs:1020`, `eval(...)`,
  `ast.rs:1161`, and it descends into arrows), so omitting an unobservable
  binding conforms.
- CreateMappedArgumentsObject (`sec-createmappedargumentsobject`, :14559) and
  Arguments Exotic Objects (`sec-arguments-exotic-objects`, :14387): aliasing to
  parameter bindings.
- PerformEval / EvalDeclarationInstantiation (`sec-performeval` :29729,
  `sec-evaldeclarationinstantiation` :29823): eval vars are deletable and land in
  the function var scope.
- The `with` statement (`sec-with-statement`, :22761).

## Assumptions

- Atom hash = randomised SipHash from the interner's std `RandomState`, so the
  #393 decision holds: the interner is the only place script-controlled strings
  are hashed, once per distinct name per body analysis, never per call. Index
  maps hash an `Atom` by writing its precomputed `u64` through an identity hasher;
  no unkeyed hash ever sees script bytes. (best guess; unkeyed Fx over atoms rejected)
- AST identifiers stay `String`; making them atoms is a parser-wide refactor.
  Tree-walker lookups keep the `&str` API (linear scan on small scopes; at most one
  interner probe per resolution on spilled ones, i.e. no worse than today).
- Slot hints are verified (`Rc::ptr_eq` of the entry atom with the op's name)
  rather than trusted via a layout-validity flag, so removal, eval injection,
  non-call-path entry and the `has_parameter_expressions` second env need no
  invalidation protocol.
- No `valgrind`/`boa` on this host: deterministic perf-counter gates (interner
  probe counts, OP histogram) are the portable acceptance proof; callgrind
  instr/call and the boa ratio are recorded only if the tools are available
  (buildbox3). Wall-time thresholds in the issue are buildbox3-specific; judge
  on the ratio vs the `origin/main` binary on the same host.
- #899 (`remove fixed per-call work…`, in flight) also edits
  `call_function_inner_impl`. Split: **#898 owns** `this` as an `Environment`
  field and the conditional `arguments` *binding*; **#899 owns** global-`this`
  caching on `Realm`, `JsFunction` clone avoidance, `DeferredCallArguments`
  cloning, VM operand-stack slicing/pooling, `IcStore::enter_body` hashing and
  `create_arguments_object` cost. Keep #898 hunks there minimal; rebase on
  `origin/main` before the PR and reconcile if #899 lands first. The planning run
  posted a comment stating this split on #898.
- One PR, one commit per slice, **commit and push after each green phase** (the
  implementation run is a single turn). If the gates are not met after D the PR
  lands as `Refs #898` with the measured numbers, never `Closes`.

## Key files

| File | Role | Lines |
|---|---|---|
| `src/interpreter/atom.rs` [new] | `Atom` (constructible only via the interner), liveness-bounded randomised interner, identity hasher | — |
| `src/interpreter/binding_map.rs` [new] | `BindingMap`: `Vec<(Atom, Binding)>` + lazy `HashMap<Atom,u32,IdentityBuild>`; `BindingKey` trait for `str`/`Atom` | — |
| `src/interpreter/types.rs` | `Environment.bindings`, new `this_binding`, ctors, `reset_function_scope`, `declare/get/set/has/is_in_tdz/find_binding_env`, `resolve_indirect_binding`, Debug | 843-872, 952-1060, 1078-1290 |
| `src/interpreter/mod.rs` | env pool, realm `declare("this")`, `MAX_POOLED_*`, `create_arguments_object` | 546-547, 2211-2235, 2564, 3280, 3774 |
| `src/interpreter/eval.rs` | `call_function_inner_impl` (this/arguments/params/body_env), `bind_function_parameters`, `resolve_this_binding`/`this_is_in_tdz`/`initialize_this_binding`, `resolve_identifier*`, `put_value_by_ref`, `eval_identifier_update`, `shared_bytecode_verdict`, `dispatch_body`, `delete ident` | 162-270, 1851-1945, 2346-2420, 4994-5030, 5693-5870, 7549-7900 |
| `src/interpreter/env_helpers.rs` | `env_set/env_get/env_has/env_check_set_binding` | 42-300 |
| `exec.rs`, `eval/generator_runtime.rs`, `eval/literals.rs`, `eval/modules.rs`, `eval/access.rs`, `builtins/global_object.rs`, `builtins/function_constructors.rs` | mechanical `.bindings.*` / `"this"` sites (134 + ~33) | per survey |
| `src/interpreter/gc.rs` | trace `this_binding` in `collect_env_roots`; tests insert String keys (1377,1382,1394,1408,1723) | 1235-1274 |
| `src/interpreter/bytecode/{compiler,op,chunk,vm}.rs` | atom names, `FrameLayout` input, local-slot ops | compiler.rs:91,202,217,293-325,383,549; vm.rs:244-400 |
| `src/interpreter/hoist_cache.rs`, `body_memo.rs` | `FrameLayout` memo built the same way | whole |
| `src/interpreter/perf_counters.rs` | counters | — |
| `tests/perf_counters_env_hashing.rs` [new] | counter gate | — |
| `.github/workflows/ci.yml` | add a step running `cargo test --features perf-counters --test perf_counters_env_hashing --test perf_counters_report_paths` (no job runs perf-counters tests today; otherwise B4 is a local-only gate) | near :49 |
| `docs/adr/2026-10-11-1200-atom-keyed-function-environments.md` [new], `CONTEXT.md`, `docs/specs/2026-07-25-environment-binding-hashing-design.md` (status note), `AGENTS.md` (counters, architecture note), `docs/perf/2026-10-11/898-atom-environments.md` [new] | decisions, vocabulary, measurements | — |

Survey facts the steps rely on: `.bindings` uses only `contains_key`(48),
`insert`(36), `get`(16), `get_mut`(6), `remove`(5), `values`(2: `gc.rs:1247`,
`function_constructors.rs:454`), `reserve/clear/capacity/is_empty`; no iteration
order dependence; `remove` is used for eval-deletable vars (`eval.rs:813,835`),
builtin stripping at setup (`global_object.rs:200`, large global env) and
`__await` temps (`eval.rs:9445`, `generator_runtime.rs:5400`); `insert` is
insert-or-replace (duplicate params, `initialize_this_binding`, TDZ re-insert).
Compiled bodies contain no closures, `let/const/class/function` declarations,
`with`, `try` or direct `eval` (`compiler.rs:512,551,718`) and `Statement::Block`
creates no scope, so a compiled chunk runs in one function-scope environment.
`smallvec`/`rustc-hash`/`getrandom` are already direct dependencies; no
`hashbrown` — use std `HashMap` with an identity `BuildHasher`.

## Steps (TDD slices; each ends green on `cargo test` + `./scripts/lint.sh`)

### Phase A — representation (no behaviour change)

**A1. `Atom` + interner** (`atom.rs`). Tests first (`#[cfg(test)]`): same text →
`ptr_eq`; `lookup` of never-interned text is `None` and inserts nothing; dropped
atoms are purged (interner len bounded after churning 100k distinct short-lived
names); non-ASCII round-trips; `Hash` writes the stored u64. Impl:
`Atom(Rc<AtomData{hash:u64,text:Box<str>}>)`, constructible only through
`Atom::intern`; thread-local `HashMap<Box<str>, Weak<AtomData>>` (std
`RandomState`); purge dead `Weak`s when len doubles. Pointer equality is valid
because every stored key is canonical. Follow `key_intern.rs`'s `thread_local!`
pattern, but not its 4096 cap (that cache returns uninterned keys past the cap,
so identity cannot be assumed).

**A2. `BindingMap`** (`binding_map.rs`). Tests first: insert-or-replace keeps
position; `get/get_mut/contains_key/remove` agree by `&str` and by `&Atom`;
identical behaviour below and above `LINEAR_MAX` (16) incl. a 100-name case;
`remove` = `swap_remove` + index fixup — a test that the moved entry is still
found by both the linear scan and the spilled index, and a 5,000-name
remove-all completes in linear time (global-env builtin stripping);
`values()`; `clear()` keeps capacity; `entry_at(i)`; `push_unique`;
`debug_assert!(name != "this")` on `insert`, `get`, `contains_key`. Impl: linear
scan below threshold (`&str`: len + memcmp; `&Atom`: pointer eq); lazily built
index above it (built on the first by-name probe, maintained by insert/remove,
never built by slot-only access). A `&str` probe on a spilled map does one
`Atom::lookup` (a miss means the name is bound nowhere in this map).

**A3. Swap the type.** `Environment.bindings` → `BindingMap`; mechanically fix
the ~134 sites (`insert("x".to_string(), b)` → `insert("x", b)`; Debug impl;
`gc.rs` and `tests.rs:145-184` tests that insert/inspect `String` keys). Update
the comment at `types.rs:845` to point at the ADR. `indirect_bindings` unchanged
(module-only, off the call path).

### Phase B — drop per-call allocation/hashing on the call path

**B1. `this` becomes `Environment.this_binding: Option<Binding>`.**
Tests first: a `gc.rs` unit test beside :1374-1408 — an env whose only
reference to an object is `this_binding` keeps it alive across a major *and* a
minor collection; a `tests.rs` unit test that a recycled pooled env has
`this_binding == None` and does not pin the old receiver; plus the
`test262-extra` cases (Test surface). Impl: `Environment::{this_value,
this_initialized, set_this, declare_this_uninit}` and `Environment::get_this(env)`
(parent walk, same skipping as the old `get("this")`); replace the ~33 `"this"`
sites (`eval.rs:164-201,267,2462,2633,2854,3369,3813,4180,4712,5340,5518-5535,
5705,5735,5847,6727,7104,7977,7991`, `eval/access.rs:29,650`,
`literals.rs:529,1200`, `global_object.rs:97`, `mod.rs:2564,3280`);
`collect_env_roots` marks `this_binding.value`; `reset_function_scope` clears the
field. This also makes the sloppy-nullish `global_env.get("this")` fallback
(`eval.rs:5718`) a field read, hash-free regardless of #899.

**B2. Conditional `arguments`.** Tests first (`test262-extra`, both modes):
`function f(){var arguments;return typeof arguments}` → `"object"`;
`function f(){var arguments = 1}` leaves `"arguments" in globalThis` false;
`function f(arguments){return arguments}`; arrow/`eval`/`with`/nested-arrow
uses; legacy `f.arguments` inside a function that never names `arguments`.
Impl: skip `declare("arguments")` when `!uses_arguments` at `eval.rs:5777`
(and the generator/async-generator twins :5381/:5573) — only the *binding*;
deferred frame args untouched (#899). `arguments` is excluded from
`FrameLayout.var_atoms` only when the arguments slot is actually declared.

**B3. `FrameLayout` memo — the single source of slot order.** Per Body, via
`BodyMemo` with the same pinning/bounds/eviction as `HoistCache`
(`hoist_cache.rs`, #165/#468): `{ arguments_slot: bool (non-arrow &&
uses_arguments), param_atoms (deduped; param position → entry index), var_atoms
(var names − params − arguments-if-slot, in first-occurrence source order, not
`HashSet` order) }`. Order = `[arguments?] ++ deduped params ++ var_atoms`.
Everything that populates a function env iterates this layout:
`bind_function_parameters` simple path (`eval.rs:4994`; `push_unique` for the
deduped prefix, replace-in-place on duplicate names), the C2 var prologue, and the
non-simple-params `body_env` build (`eval.rs:5808-5830`, today a fresh
`HashSet<String>` per call: random order and SipHash per call).
`reset_function_scope` clears entries (atom refcount decs; capacity kept for the
pool; the `mod.rs:2227` capacity gate retained).

**B4. Counters + gate test.** `perf-counters` fields `atom_intern_calls`,
`atom_lookup_calls`, `local_slot_hits`, `local_slot_misses`, reported as `PERF`
rows (`perf_counters.rs`; document in `AGENTS.md`). Test
`tests/perf_counters_env_hashing.rs` (`#![cfg(feature = "perf-counters")]`,
child-process style of `perf_counters_report_paths.rs`): a leaf-call loop at
N=1000 and N=2000 reports identical `atom_intern_calls`/`atom_lookup_calls`
(zero per call) on the default (bytecode) path.

### Phase C — VM identifier ops by atom

**C1.** `Chunk.names: Vec<Rc<str>>` → `Vec<Atom>` (`chunk.rs`; `compiler.rs:91`
`add_name`); property-name ops use `Atom::as_str()`. `LoadName`, `ResolveName`,
`StoreResolvedName`, `LoadCalleeName` (gains the same current-scope fast path),
`UpdateName` use atom lookups. Make `resolve_identifier`, `resolve_identifier_ref`,
`get_identifier_value_by_ref`, `put_value_by_ref`, `env_check_set_binding`,
`env_set`, `eval_identifier_update`, `find_binding_env` generic over
`K: BindingKey` (monomorphised; `&str` callers unchanged) so the chain walk —
including the spilled global env — never hashes a string; `&str` callers intern
lazily once per resolution at the first spilled env (a `None` means no binding
anywhere on the chain). Tests (`bytecode/tests.rs`): outer-scope read/write,
global-function callee among >100 globals redefined between calls, missing
global → unchanged ReferenceError text, strict write to undeclared,
`typeof undeclared`; the `direct_call_*with_base*` tests stay green.

**C2.** `run_chunk_inner` var prologue (`vm.rs:244-253`) iterates
`FrameLayout.var_atoms` (not the compiler's own `chunk.var_names` order); bulk
`push_unique` when the env holds exactly `[arguments?] ++ params`, else per-name
declare-if-absent by atom.

### Phase D — compile-time local slots

**D1. Compiler input.** `compile_body(stmts)` → `compile_function_body(stmts,
&FrameLayout, is_arrow)`; `shared_bytecode_verdict` (`eval.rs:1851`) passes the
layout (a Body-keyed memo is sound: params are fixed per literal). Locals =
simple params ∪ `var_atoms` ∪ `arguments` (non-arrow with slot only); for
non-simple parameter lists only the body-env vars. Slot hint = index in the B3
order.

**D2. Ops.** `LoadLocal name:u16 slot:u16`, `StoreLocal`, `UpdateLocal name slot
mode`. The compiler emits `<e>; StoreLocal; Pop` in place of `ResolveName; <e>;
StoreResolvedName; Pop`, and `LoadLocal` for `LoadName` on locals (reference
resolution order is unobservable: no `with`/eval in a compiled body, and a
function-scope binding cannot be removed during it). Free names keep C1 ops. VM
hit path: `entry_at(slot)` with `Atom::ptr_eq` **plus today's
`StoreResolvedName` guards** (`initialized`, kind `Var|Let`,
`global_object_id.is_none()`, no `indirect_bindings`); on miss fall back to the C1
path and bump `local_slot_misses`. Update `Op::from_u8` and op-histogram names.
Tests (`bytecode/tests.rs`): local read/write/update; param+var same name; hint
miss path (pre-declare a name so the order differs); `arguments[0]=x` aliasing
both directions in a compiled body (`test262-extra`).

**D3. Measure + document** (`docs/perf/2026-10-11/898-atom-environments.md`).
Release build vs the `origin/main` binary snapshot, interleaved, median of 5:
`bench_calls_{fib,leaf,nested,closure_call}` and
`bench_numeric_{intarith,bail_none,gaussblur}` (repros are in
`docs/perf/2026-10-10/repros/`, not `benchmarks/scripts/`), default and
`--no-bytecode`; **cold Mandreel wall-time A/B** with the `docs/perf/2026-10-08`
driver (#389/#393 were judged there; the spilled global env + interner is where a
regression would appear); callgrind fib(22) instr/call and the boa ratio if
`valgrind`/`boa` exist (recipe: `findings.md` "Reproduction commands",
`CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`); gaussblur OP histogram
(perf-counters build, never timed) shows no `LoadName/ResolveName/
StoreResolvedName` in the loop; `atom_*` counters flat per call.

## Testing

- `cargo test` (debug; GC root asserts), `cargo test --release`,
  `./scripts/lint.sh`, `cargo clippy --features perf-counters -- -D warnings`,
  `cargo test --features perf-counters --test perf_counters_env_hashing`.
- `git submodule update --init --depth 1 test262` first (empty in fresh
  workspaces). Targeted test262 after each phase (directories confirmed from
  `test262-pass.txt`): `language/{eval-code,function-code,arguments-object,
  block-scope,global-code,module-code,identifier-resolution,destructuring,
  rest-parameters}`, `language/statements/{with,function,for,for-of,for-in,try,
  let,const,variable,class}`, `language/expressions/{function,arrow-function,call,
  delete,typeof,assignment,super,generators}` (incl. `dflt-params-*`),
  `language/types/reference`, `annexB/language/{eval-code,function-code,
  global-code}`, `built-ins/{Function,eval,GeneratorFunction,AsyncFunction}`; also
  `language/expressions/{this,new.target,postfix-increment,prefix-increment}`
  where present. Then the full `uv run python scripts/run-test262.py` against the
  `origin/main` baseline (never `--update-baseline`).
- `JSSE_GC_STRESS=7 uv run python scripts/run-test262.py test262-extra/` normal
  and `--no-bytecode`; same with `--binary target/release-checked/jsse`
  (`cargo build --profile release-checked`); `tests/gc_stress.rs`.
- `uv run python scripts/run-custom-tests.py`; `./scripts/run-library-tests.sh
  acorn` and `decimal.js`.

## Test surface

New `test262-extra/` files [new] (test262 header style with `esid:`; run in
bytecode default and `--no-bytecode`; extend, don't duplicate, the existing
`function-call-environment-reuse.js` and `resolved-local-binding-write.js`):

- `function-declaration-instantiation-duplicate-parameter-compiled.js`
  (`sec-functiondeclarationinstantiation`).
- `arguments-mapped-aliasing-compiled-local-slots.js`
  (`sec-createmappedargumentsobject`).
- `function-arguments-binding-not-observable-without-reference.js` (B2 list).
- `function-this-binding-field-derived-constructor-tdz.js`,
  `function-this-binding-field-arrow-and-nullish-receiver.js`
  (`sec-function-environment-records`, `sec-ordinarycallbindthis`).
- `direct-eval-var-in-function-with-many-bindings.js`
  (`sec-evaldeclarationinstantiation`): >16 vars + eval-injected + `delete`d var.
- `with-shadows-local-in-uncompiled-function.js` (`sec-with-statement`).
- `closure-escape-after-env-pool-reuse-recursive.js`.
- `identifier-resolution-global-function-callee-in-large-global-env.js`
  (`sec-getidentifierreference`).

Host/perf-only, in `tests/`: `perf_counters_env_hashing.rs` (not spec-visible).

## Regression risk

- `test262-pass.txt` movement most likely in `language/eval-code`,
  `language/arguments-object`, `language/function-code`, derived-constructor `this`
  TDZ, `annexB` hoisting, module indirect bindings.
- GC: `this` leaves the traced map (B1 adds the explicit root + reset); there is no
  env write barrier, minor GC relies on persistent scans of User/Iterator/Arguments
  objects and `call_stack_envs` — `BindingMap` lives inside `Environment`, so it
  adds no new holder; the pool is safe only because `recycle` requires
  `strong_count==1` and `reset_function_scope` clears everything (including
  `this_binding`). Pre-existing, verify-only: `func_env` is pushed to
  `call_stack_envs` after `bind_function_parameters` (`eval.rs:5802` vs :5836), so
  values produced by an earlier parameter default may be unrooted while a later
  default runs — if `JSSE_GC_STRESS` default-param runs expose it, file a separate
  issue rather than widening this PR.
- Shared machinery: tree-walker `resolve_identifier`/`env_set`; generator/async
  state machines declaring by name into `exec_env` (`eval.rs:5449-5470,5640-5660`,
  `generator_runtime.rs`); mapped-arguments `(EnvRef, name)` map (name-keyed
  `Environment::get/set`, `types.rs:2637…3550`); `BodyMemo` bounds (the
  `FrameLayout` memo must pin Bodies like `HoistCache`); consumers of the
  `perf_counters` report format.
- HashDoS: only the interner hashes names (randomised); state the argument in the
  ADR. Interner memory is bounded by liveness (`Weak` purge) under `eval`/
  `new Function` churn.
- Perf: mid-size envs pay a linear scan (pointer-eq for atoms); the lazy index
  covers >16; multi-realm/ShadowRealm share the thread-local interner (safe:
  canonical atoms are realm-agnostic).

## Out of scope / follow-ups

- AST identifiers as atoms; parser changes.
- `(hops, slot)` for free/captured variables and per-site inline caches.
- Slot ops for the tree-walker and generator/async state-machine bodies.
- #899 items (global `this` cache, `JsFunction` clone, deferred args clone, VM
  operand slicing/pooling, IC body-handle hashing, `create_arguments_object`).
- Per-closure bytecode cache / closure-creation cost; `hoist_function_decl`
  per-call `params.clone()`; `indirect_bindings` and `PropertyMap` hashing.
- Baseline roll-forward (`test262-pass.txt`) — a main-branch operation.
