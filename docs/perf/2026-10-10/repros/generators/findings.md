# generators — generator / iterator-protocol performance

jsse HEAD `2b621c5` vs boa 0.22.0, buildbox3, 2026-10-10. Repros: `bench_generators_*.js` in this directory. All checksums are identical on jsse, boa and node.

## Summary

- **Bytecode VM:** generator state bodies and terminator expressions run on the tree-walker. `let`/`const` and for-of consumers bail the compiler (`lexical declaration`, `statement:ForOf`). `--no-bytecode` is within 1% of default on every repro.
- **Per-step cost:** a generator `next()` plus for-of step costs ~27k instructions. A hand-written closure iterator costs 20-25k, so the generator machinery is not an outlier.
- **Waste inside that step:** terminator AST clone and drop, stack re-sync, per-iteration `Environment`, and `{value, done}` allocation plus two lookups.
- **Cross-cutting:** SipHash on `Environment.bindings` costs 13-16%, malloc/free another 10-12%.
- **Algorithmic bugs:** Map/Set iteration (O(n) per step), and the generator transform re-running per call.
- **js-tokens:** not a generator problem. 85% of instructions are in `RegExp.exec`.
- **sync-fs/async-fs:** 69-74% of AST work units are one yield-free Uint8Array fill loop in `randomFileContents`.

## Gap table

Median of 5, loadavg ~7 for the seven rows re-run at low load, 10-12 for the rest. The `--no-bytecode` column is a median of 3.

| repro | jsse s | boa s | ratio | no-bytecode s |
|---|---|---|---|---|
| map_set_iteration (8000-entry for-of/spread) | 4.53 | 0.22 | 21.1 | 4.53 |
| lazy_collections (realistic reduction) | 8.05 | 1.17 | 6.9 | 8.15 |
| manual_next | 3.03 | 0.47 | 6.5 | 3.08 |
| simple_yield | 5.43 | 1.02 | 5.3 | 5.39 |
| pipeline | 6.34 | 1.27 | 5.0 | 6.39 |
| spread_from | 2.12 | 0.52 | 4.1 | 2.12 |
| array_forof_control (CONTROL, no generator) | 4.99 | 1.72 | 2.9 | 4.98 |
| tokenizer | 3.43 | 1.27 | 2.7 | 3.43 |
| yield_star_tree | 4.63 | 1.77 | 2.6 | 4.68 |
| sticky_regex | 2.98 | 1.22 | 2.4 | 2.98 |
| sticky_lookahead (CONTROL: boa also O(n)) | 1.72 | 1.32 | 1.3 | n/a |

Whole JetStream benchmarks, one iteration, uninstrumented build:

| benchmark | jsse s | boa s | ratio |
|---|---|---|---|
| lazy-collections | 8.31 | 1.29 | 6.4 |
| js-tokens | 25.3 | 7.2 | 3.5 |
| sync-fs | 8.94 | 1.92 | 4.7 |

Per-phase sync-fs times (jsse/boa, s): setup 2.59/0.40, swapByteOrder 2.28/0.57, delete+size 2.34/0.51, visit 1.18/0.28.

## Root causes

callgrind on the perf-counters build; raw `.out` files in `$R/work-generators/{small,cg}`. Calibration, instructions per iteration (top-level script on the tree-walker, 20k iterations):

| loop | instr/iter |
|---|---|
| trivial loop | 7.3k |
| one user call | 13.1k |
| closure iterator via `next()` | 24.7k |
| closure iterator via for-of | 19.5k |
| generator via for-of | 26.7k |

**RC1. Map/Set `next()` is O(n) per step.**
- `builtins/collections.rs:89` clones `map_data()`, and `:813` clones `set_data()`, on every step. On a 3000-entry Map, 70% of the `next` closure is `Vec::clone` plus drop.
- `Map.get` (`:216`), `has` (`:269`) and `Set.has` (`:960`) also clone, then linear-scan. Map/Set are association vectors, not hash tables.
- Side check: 200k `get`+`has` on 5000 entries takes 6.07 s vs 0.136 s on boa (45x). Cross-cutting (hash-map benchmark, proxy-vue WeakMap).
- In JetStream the maps are small, so the direct gain is modest (2.1% of sync-fs).

**RC2. Generator transform per call.**
- `eval.rs:5589` calls `transform_generator(body, params)` on every call and never caches it.
- sync-fs: 87,800 calls, ~31k instructions each, 7.76% of the run. lazy-collections: 9,555 calls, 1.8%.

**RC3. Terminator deep clone.**
- `generator_runtime.rs:945` and `:4415` run `terminator.clone()` every state step although the machine is already an `Rc`.
- lazy-collections: 5.07M clones. `StateTerminator::clone` is 6.4%, its drop 9.9%.
- The drop goes through `Drop for ExprBox` (`ast.rs:465`), which allocates a `Vec` per drop; `push_children` alone is 2.0%.

**RC4. No fast path for intrinsic iterators.**
- `create_iter_result_object` (`iterators.rs:4472`) allocates with `"value".to_string()` / `"done".to_string()` and interns the keys: 11.2% of simple_yield.
- Consumers then do two generic property gets (`iterators.rs:4875,4888`). Same pattern in for-of (`exec.rs:2422`), `ForOfHead` (`generator_runtime.rs:1787`), `iterate_to_vec` and `yield*`.
- ArrayIterator `next` also does `index.to_string()` plus two map lookups per element.

**RC5. Driver bookkeeping** (`generator_runtime.rs:645+`).
- `iterator_state().cloned()` (`:666`); the object kind is rebuilt twice per resume.
- Scope-stack and for-of-stack side tables are re-synced every step (`:7145`, `:7080`; 4.3%).
- `IcStore::enter_body` runs per state (3-4%).
- `clear_terminator_ic_sites` (`generator_transform.rs:300`) forces terminators onto the slow path; `eval_expr` is 24.9% of simple_yield.

**RC6. Fresh `Environment` per loop iteration.**
- `reconcile_scope_stack` with `CopyForward` (`exec.rs:2070/2124`) is 13.5% of simple_yield and 7.7% of lazy-collections. `exec_for` (`exec.rs:1911-2008`) and for-of (`:2422`) do the same on the consumer side.

**RC7. js-tokens: `RegExp.exec`.**
- 15,593 execs. `regexp_exec_raw` is 81.5% of instructions; `fancy_regex::vm::run_with` is 62.7% (11,579 calls, 239k instructions per call).
- Sticky patterns with lookahead or backrefs run `captures_from_pos` (unanchored); the result is rejected afterwards at `regexp.rs:8165`.
- The single-entry subject cache (`regexp.rs:665`) is thrashed by `.test(shortString)` calls, so the 14 KB subject is reconverted to UTF-8 on nearly every exec. `js_string_to_regex_input_mode` is 13.1%.
- Per-exec constants: pattern and flags rebuilt (`:8071-8079`), cache key allocates (`:5203`), `reset_quantifier_inner_captures` rescans the source (2.6%).
- Microbench: failed `/(?!0)\d/y` on 14 KB takes 1.1 ms per exec, against 23 µs for `/\d/y`.

**RC8. Inherited costs.**
- `Environment.bindings` uses SipHash `RandomState` (`types.rs:847`): 13.4% of simple_yield, 14.0% of lazy-collections, 15.7% of sync-fs.
- `compile_var_declaration` rejects every `let`/`const` (`compiler.rs:553`); no for-of or try support (`compiler.rs:988-989`).
- Typed-array store `view[i] = x`: 1.24M `number_ops::to_string` calls (2.6%) plus `set_object_with_key_result` (6.65%). The index is stringified on every store.

## Proposed work items

Size: S is hours, M is 1-2 days, L is a week or more. All must keep test262 green and `JSSE_GC_STRESS=7` on `test262-extra/` green.

1. **`perf(collections): O(1) Map/Set iterator step; no entry-vector clone in get/has`** (S)
   - Borrow instead of `.cloned()` at `collections.rs:76-150` and `:799-870`, and in `Map.get/has` and `Set.has`.
   - Acceptance: near-linear scaling (doubling N roughly doubles time) and `map_set_iteration` ≤ 3x boa (now 21x). The remaining gap is generic iteration cost, which item 4 addresses.
   - Follow-up for the collections owner: a hashed index so `get`/`set`/`add`/`delete` are O(1).
2. **`perf(generators): cache GeneratorStateMachine per function body`** (S)
   - Cache by the body `Rc` pointer.
   - Acceptance: `transform_generator_inner_opts` < 0.1% in the sync-fs callgrind (now 7.76%); `yield_star_tree` and sync-fs wall time down ≥ 5%.
3. **`perf(generators): no per-step terminator clone or per-resume state cloning`** (M)
   - Borrow the terminator from the `Rc`. `mem::take` the driver stacks instead of side-table re-sync. Add an `ExprBox` drop fast path with no `Vec` allocation.
   - Acceptance: `StateTerminator::clone` plus drop < 2% in the lazy-collections callgrind (now ~16%); `simple_yield` and `pipeline` ≥ 15% faster.
   - Risks: "already executing" re-entrancy, async generators, GC visibility of taken stacks.
4. **`perf(iterators): allocation-free IteratorStep for intrinsic iterators`** (M)
   - Add `iterator_step_value` that bypasses the result object when `next` is pointer-equal to the intrinsic generator/array/map/set/string `next`. Use it in for-of, the driver's `ForOfHead`, `iterate_to_vec` and `yield*` (non-forwarding).
   - Separately (S): pre-interned `value`/`done` keys and a no-`to_string` ArrayIterator.
   - Acceptance: `array_forof_control` ≤ 2.4x boa (now 2.9x); `spread_from` ≤ 3.2x (4.1x); `simple_yield` ≥ 12% faster.
5. **`perf(regexp): anchored sticky exec, per-regexp cached pattern, multi-entry subject cache`** (M)
   - Anchored search via `fancy_regex::RegexInput::new(s).from_pos(p).anchored(true)` and `regex_automata` `Anchored::Yes`.
   - **UNVERIFIED:** only confirmed that fancy-regex 0.19.2's `Wrap` path honours `is_anchored()`; not confirmed that `vm::run` for the `Fancy` variant does. Check that first.
   - Replace the single-entry cache with a 2-4 entry LRU. Cache the compiled regex on the RegExp slot.
   - Acceptance: js-tokens `fancy_regex::vm::run_with` < 20% and `js_string_to_regex_input_mode` < 2% in callgrind; `tokenizer` ≤ 1.8x; `sticky_lookahead` ≤ 0.2 s.
   - The "removes ~63% of js-tokens instructions" figure is an upper bound, not a measurement.
6. **`perf(interpreter): skip per-iteration env copy when the loop body cannot capture`** (M)
   - Cache a static "no closure/eval/with/class" flag per AST node.
   - Acceptance: `simple_yield` further ≥ 10%; `language/statements/for*/scope-*` unchanged.
7. **`perf(generators): inline caches for terminator expressions`** (M)
   - Acceptance: `eval_expr` share of `simple_yield` drops from 24.9% to < 15%.
8. **`perf(typedarray): numeric-key fast path for ta[i] get/set`** (M, cross-theme)
   - Acceptance: no `ryu_js::format64` from member assignment; the sync-fs setup loop < 1.2x boa.
9. **`perf(bytecode): lexical declarations, for-of, generator state bodies on the VM`** (L, strategic)
   - Acceptance: `vm_ops` > 90% of work units on `simple_yield`, ratio ≤ 2x.
10. **`perf(interpreter): faster Environment.bindings hashing`** (M, cross-cutting)
    - Per-process-seeded fast hasher or interned keys.

Estimate (not measured): items 1-4, 6 and 7 together remove ~40-45% of instructions on the simple_yield/pipeline shapes. Parity needs items 9 and 10.

## What did NOT reproduce

- **InlineYield replay:** 0 probe hits on lazy-collections, js-tokens, sync-fs, async-fs and the pipeline/yield*/spread repros. The probe was a temporary `eprintln` at `eval.rs:903` and the driver `Completion::Yield` sites, in a scratch copy. A positive control (`({ a: o.b = yield 1 } = {})`) fires 2x `INLINE_YIELD_EVAL` and 1x `INLINE_YIELD_REPLAY_SYNC`, so the probe works. The other 8 shapes tried (`[x = yield] = arr`, `x += yield`, `f(yield, yield)`, `o[yield]=yield`, tagged template, `new C(yield)`, `?:`, `for-of [yield…]`) did not fire.
- **Bytecode VM:** all repros are within 1% between default and `--no-bytecode`.
- **Sticky scanning vs boa:** boa is also O(remaining input). With a literal-prefix pattern jsse is faster (0.077 s vs 1.19 s at 56 KB). The js-tokens gap is per-exec constants plus the fancy VM.
- **GC:** `gc_ms` is 3-8% of run time, with at most 90 collections each of minor and major.
- **`yield*` depth:** 2.6x boa at depth 7, not super-linear.
- **For-of over arrays:** slow (2.9x) but linear.
- **Claim narrowed:** "every consumer bails the compiler" holds only for generator bodies and `let`/for-of consumers. Small arrow predicates in lazy-collections do compile.
- **async-fs:** not measured uninstrumented. The generator side is the same typed-array fill loop (74% of work units), plus async-generator driver costs that mirror RC3 and RC5.

## Reproduction

```
R=/home/igalia/pmatos/dev/bench-workspace/run-2026-10-10
python3 $R/microbench.py '{"jsse":["'$R'/jsse-head/target/release/jsse"],"boa":["'$R'/boa-v0.22-src/target/release/boa"]}' <dir with bench_generators_*.js> out.json 5
python3 $R/microbench.py '{"jsse_nobc":["'$R'/jsse-head/target/release/jsse","--no-bytecode"]}' <dir> out-nobc.json 3
# counters: perf-counters build at $R/work-generators/target
$R/work-generators/target/release/jsse bench_generators_simple_yield.js 2>&1 >/dev/null | head -40
# function-level: scaled inputs in $R/work-generators/small
valgrind --tool=callgrind --callgrind-out-file=cg.out $R/work-generators/target/release/jsse small/simple_yield.js
callgrind_annotate --inclusive=yes cg.out | head -90
callgrind_annotate --tree=caller --inclusive=yes cg.out
```

Whole-benchmark harness: the JetStream file plus `const b=new Benchmark(); b.prepareForNextIteration?.(); b.runIteration(); b.validate(1)`.
