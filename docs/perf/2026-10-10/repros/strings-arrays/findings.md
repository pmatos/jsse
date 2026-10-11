# Theme `strings-arrays`: String / Array / property-heavy object code

Date 2026-10-10. jsse v0.11.0 (HEAD 2b621c5) vs boa v0.22.0, buildbox3 (AMD EPYC 7501, shared box, loadavg recorded per row).

## Summary

- Against boa 0.22 the String/Array/Object builtins are 1.8x to 10.8x slower on global-scope repros (`str_indexof` 10.8x, `obj_keys` 9.0x, `str_concat3` 3.6x, `obj_assign_spread` 3.1x, `obj_forin_entries` 2.6x, `str_append` 2.6x, `arr_sort_cmp` 2.5x, `arr_hof` 2.3x). Causes are structural: (1) flat `Arc<Vec<u16>>` strings make `s += x` quadratic, (2) the `indexOf` family round-trips the receiver through a Rust `String` on every call, (3) Array builtins other than `push` are spec-literal per-element property-MOP loops, (4) `Object.keys/assign/entries`/spread/for-in build a descriptor *object* per key, (5) std SipHash is on every property/name lookup (16.6% of UniPoker's instructions).
- Caveat when reading ratios: a plain scalar loop is **4.0x slower than boa inside a function** (`ctl_scalar_fn`) but 1.3x at global scope (`ctl_scalar_global`). `fn-scope/` variants carry that baseline (it belongs to the calls/closures/opmix themes), so the primary repros are global-scope and all acceptance criteria refer to them. Eight repros (`str_scan`, `arr_grid`, `arr_pushindex`, `arr_forof`, `arr_indexof`, `arr_slice_concat`, `arr_queue`, `str_keys_control`) sit within 1.3x of the control and are filed under `no-gap/` as absolute-cost cases, not builtin-gap claims.
- The original `bench_string` gap (2.15x) is only the O(n^2) append; boa is also quadratic (about 3x smaller constant), so a rope/builder would turn the largest string gap into a jsse win. The bytecode VM is not helping anywhere in this theme and is 1.8x slower than the tree-walker on chained concatenation.
- Two conformance bugs surfaced: lone surrogates are corrupted by `from_utf16_lossy` in `indexOf/includes/startsWith/lastIndexOf` (`"\ud800".indexOf("\udc00")` gives 0, should be -1) and default `Array.prototype.sort` orders by UTF-8 bytes, not UTF-16 code units. No file in `test262-extra/` covers either (a grep for the surrogate escapes only matched `RegExp-legacy-statics.js`); test262 coverage was not checked.
- JetStream in scope (UniPoker 3.00x, pdfjs 2.87x, Babylon 2.69x, ML 1.89x, FlightPlanner 1.83x) is additionally gated by bytecode-compiler coverage (`let`/`const`, array/object literals, `typeof`, function expressions bail whole bodies to the tree-walker); see RC7.

## Gap table

Medians of 5 runs through `microbench.py` (default bytecode VM); `--no-bytecode` column is a median of 3. Ratio = jsse / boa (higher = jsse slower).

| repro | global jsse s | global boa s | global ratio | ratio / ctl | jsse --no-bytecode s | load1 | fn-scope jsse s | fn boa s | fn ratio | load1 |
|---|---|---|---|---|---|---|---|---|---|---|
| str_append | 3.53 | 1.37 | 2.58x | 1.96x | 3.73 | 3.1 | 3.60 | 1.04 | 3.46x | 2.6 |
| str_concat3 | 3.53 | 0.97 | 3.64x | 2.77x | 1.97 | 2.8 | 3.45 | 0.94 | 3.66x | 2.9 |
| str_indexof | 2.88 | 0.27 | 10.80x | 8.20x | 2.83 | 2.7 | 3.24 | 0.23 | 14.38x | 2.8 |
| str_scan | 6.84 | 4.03 | 1.70x | 1.29x | 6.84 | 2.5 | 5.43 | 1.04 | 5.23x | 2.8 |
| arr_hof | 4.03 | 1.72 | 2.34x | 1.78x | 4.33 | 3.4 | 4.31 | 1.55 | 2.78x | 4.8 |
| arr_indexof | 4.13 | 2.47 | 1.67x | 1.27x | 4.23 | 3.3 | 4.96 | 2.78 | 1.79x | 4.6 |
| arr_join | 2.63 | 1.47 | 1.79x | 1.36x | 2.68 | 3.7 | 3.04 | 1.66 | 1.83x | 5.0 |
| arr_slice_concat | 7.84 | 5.69 | 1.38x | 1.05x | 7.60 | 2.7 | 8.66 | 5.70 | 1.52x | 3.4 |
| arr_queue | 3.23 | 2.37 | 1.36x | 1.03x | 3.23 | 3.1 | 3.71 | 2.68 | 1.38x | 4.2 |
| arr_sort_cmp | 5.64 | 2.27 | 2.48x | 1.88x | 5.89 | 2.9 | 6.15 | 1.91 | 3.22x | 3.7 |
| arr_sort_default | 5.24 | 2.88 | 1.82x | 1.38x | 5.39 | 2.9 | 5.18 | 2.01 | 2.58x | 3.3 |
| arr_literals | 5.79 | 2.58 | 2.25x | 1.71x | 5.79 | 3.4 | 5.48 | 1.04 | 5.26x | 4.9 |
| arr_spread_destructure | 6.59 | 3.48 | 1.90x | 1.44x | 6.59 | 2.7 | 6.05 | 2.37 | 2.55x | 3.2 |
| arr_pushindex | 4.93 | 3.53 | 1.40x | 1.06x | 4.98 | 3.2 | 3.84 | 1.09 | 3.51x | 4.6 |
| arr_forof | 3.98 | 2.53 | 1.58x | 1.20x | 3.93 | 4.2 | 3.66 | 1.20 | 3.06x | 4.1 |
| arr_grid | 2.93 | 1.82 | 1.61x | 1.22x | 2.93 | 3.5 | 2.42 | 0.79 | 3.08x | 4.5 |
| obj_keys | 4.64 | 0.52 | 8.96x | 6.80x | 4.08 | 3.2 | 4.58 | 0.32 | 14.12x | 2.5 |
| obj_assign_spread | 2.98 | 0.97 | 3.07x | 2.33x | 2.93 | 3.6 | 3.29 | 1.04 | 3.15x | 3.1 |
| obj_forin_entries | 4.08 | 1.57 | 2.60x | 1.97x | 4.08 | 3.5 | 4.27 | 1.15 | 3.72x | 2.8 |
| str_keys_control | 2.93 | 1.97 | 1.48x | 1.13x | 2.93 | 2.6 | 2.58 | 0.79 | 3.28x | 2.9 |
| **ctl_scalar_global** (control) | 9.80 | 7.44 | 1.32x | 1.00x | 10.60 | |  |  |  | |
| **ctl_scalar_fn** (control) |  |  |  | 1.00x |  | | 3.13 | 0.82 | 3.82x | |

Columns: `global` = top-level `var` variant (the files in this directory; isolates builtin cost), `ratio / ctl` = global ratio divided by the scalar-loop control ratio (1.32x), `fn-scope` = same body wrapped in `(function () { ... })()` (files in `fn-scope/`, realistic but includes jsse's ~4x local-variable overhead, see the `ctl_scalar_fn` row). `load1` is the 1-minute loadavg sampled before each benchmark (all rows <= 5). Medians of 5 runs (`--no-bytecode`: median of 3). Files for rows with `ratio / ctl` < 1.35 are in `no-gap/`.


Original suite (benchmarks/scripts), same driver: `bench_string` jsse 1.12 s vs boa 0.52 s = 2.15x; `bench_array` 0.57 vs 0.32 = 1.78x; `bench_object` 0.72 vs 0.57 = 1.26x (medians of 5, loadavg1 ~7; `--no-bytecode` within 5% of default on all three). `bench_string` is just a 100k-iteration `s += "x"` append, i.e. RC1.

## Method and evidence

1. `perf-counters` build (BODY/BAIL/OP tables) of the real JetStream scripts (UniPoker, pdfjs, ML, FlightPlanner, Babylon; first iteration only).
2. `valgrind --tool=callgrind` on the head binary for scaled-down copies of the repros (instruction counts, inclusive and self), plus UniPoker with 150 hands.
3. Micro-repros run against boa 0.22 and against jsse with and without `--no-bytecode`.

## Root causes (numbered by theme; measured repro gaps: RC2 10.8x, RC6 9.0x, RC1 2.6-3.6x, RC3 1.8-2.5x, RC5 1.9x; RC4 and RC7 are cross-cutting)

### RC1. Strings are flat `Arc<Vec<u16>>`: every `+` / `+=` copies the left operand (quadratic)

- `src/types.rs:46-51` (`JsString { code_units: Arc<Vec<u16>> }`), `into_vec` at `src/types.rs:81`.
- Tree-walker: `src/interpreter/eval.rs:548-575` (binary `+` string fast path), `src/interpreter/eval.rs:3524-3548` (`apply_compound_assign`), `eval_binary_slow` Add arm at `src/interpreter/eval.rs:1959-1986`. VM: `Op::Add` at `src/interpreter/bytecode/vm.rs:681-730` calls `eval_binary` for strings too.
- Each of these does `lval.into_string().into_vec()`; `Arc::try_unwrap` only avoids the copy when the Arc is unique, but `lval` was just cloned out of the variable binding (RC >= 2), so the whole buffer is cloned, then `extend_from_slice` reallocates again.
- Evidence: callgrind of 20k appends: 65% of instructions in `__memcpy_avx_unaligned_erms`. Scaling for `s += 'x'`: N=50k/100k/200k/400k -> jsse 0.39/1.05/3.85/14.9 s, boa 0.19/0.52/1.34/4.4 s (both quadratic; jsse's constant is ~3x boa's because it copies 2 bytes per unit and clones+reallocates). `t = t + 'ab' + j` (two copies per iteration): N=50k jsse 3.5 s vs boa 0.9 s, N=100k 14.5 s vs 3.3 s.
- The bytecode VM does not help single `s += x` (3.53 s default vs 3.73 s `--no-bytecode`) and is 1.8x *slower* than the tree-walker on chained concatenation `t = t + "ab" + j` (3.53 s vs 1.97 s): `Op::Add` routes through `eval_binary` -> `eval_binary_slow` which clones both operands via `to_primitive`, so the uniquely-owned intermediate is copied a second time; see WI-1b.

### RC2. `indexOf`/`lastIndexOf`/`includes`/`startsWith`/`endsWith` decode the receiver to a Rust `String` and re-encode it

- `this_string_value()` at `src/interpreter/builtins/string.rs:6-27` returns `String::from_utf16_lossy(..)`; 21 call sites (indexOf :197, lastIndexOf :237, includes :281, startsWith :329, endsWith :371, toLowerCase :469, toUpperCase :480, trim* :529-555, repeat :568, padStart/padEnd :590/:626, concat :662, normalize :1291, localeCompare :1320). `indexOf` then does `utf16_units(&s)` (`:77`, `encode_utf16().collect()`) for both haystack and needle before a naive O(n*m) search.
- Evidence: callgrind of an `indexOf(needle, pos)` scan loop: 41% of instructions in the `from_utf16_lossy` decode iterator and 33% in `Vec<u16>::extend(EncodeUtf16)` (74% together) -- every call on a 98 KB string costs two O(n) conversions plus two allocations regardless of where the match is. Measured: 3,800 `indexOf` calls on a 98 KB string take 3.2 s in jsse vs 0.29 s in boa (11x); a tokenizer-style `while ((p = s.indexOf(x, p)) !== -1)` is therefore O(n * matches).
- The same decode is used by the other 20 methods; only `indexOf` was timed (UNVERIFIED for includes/startsWith/endsWith/lastIndexOf, which share the pattern). `split`, `replace(str,str)`, `toUpperCase`, `toLowerCase`, `trim` on a 12 KB string did NOT show a gap (jsse 0.97 s vs boa 2.12 s), so the cost is per-call O(receiver length) and only matters for long receivers called repeatedly.
- Conformance: lossy decode maps every lone surrogate to U+FFFD, so `"\ud800".indexOf("\udc00")` returns 0 (should be -1), `"\ud800".includes("\udc00")` is true, `"\ud800x".startsWith("\udc00")` is true (jsse prints `0 true 1 true`; boa and node print `-1 false -1 false`).
- `charAt/charCodeAt/codePointAt/slice/substring` already use `this_js_string()` (Arc clone, no conversion), so scans by index are not affected by RC2 (their `str_scan` ratio of 1.7x sits within 1.3x of the scalar control; its callgrind shows name resolution at 32% SipHash, not the method call).

### RC3. Array builtins are spec-literal property-MOP loops; only `push` has a dense fast path

- `src/interpreter/builtins/array.rs`: `shift` :1018, `unshift` :1066, `pop` :988, `indexOf` :1115, `includes` :1215, `join` :1255, `slice` :1480, `concat` :1381, `forEach` :1622, `map` :1661, `filter` :1713, `reduce` :1773, `splice` :2157, `sort` :2369 (gather/scatter around the sort). Each element goes through `k.to_string()` (heap `String`) -> `obj_has_throw` (`proxy_has_property`) -> `obj_get` (`get_object_property`, re-parses the index) -> and for writes `obj_set_throw` (proxy check, `has_own_property`, `get_property_descriptor_on_id` walk of the proto chain, `define_own_property`).
- The only dense fast path is `push` at :886-985, guarded by `class_name == "Array"`, no proxy, extensible, `length` data property in sync, no inherited index setters. `shift`, `unshift`, `splice` could `Vec::remove/insert/drain` under the same guard.
- Evidence (callgrind, shift/push queue of 2000): `JsObjectData::set_property_value` 9.2%, `has_own_property` 9.2%, `PropertyMap::get` 8.7%, `obj_set_throw` 8.4%, `PropertyMap::contains_key` 5.7%, `<u64 as Display>::fmt` 4.2%, plus malloc/free of the index strings. UniPoker profile: `Array.prototype.shift` closure is 17.9% of all instructions (52-element deck, 40k calls), `sort` 11.7%.
- `join` additionally builds a Rust `String` per element and `JsString::from_str(&parts.join(&sep))` converts the whole result UTF-8 -> UTF-16 (array.rs:1255-1310).
- Callback iteration (`forEach` allocates a `vec![kvalue, idx, o.clone()]` per element, array.rs:1642; `map`/`filter` write results with `create_data_property_or_throw(.., &k.to_string(), ..)` which converts the key `String -> JsPropertyKey` again, `src/interpreter/builtins/array.rs:201-215`). In the `slice(0)` profile `create_data_property_or_throw::<String>` alone is 24% inclusive (~2.4k instructions per copied element).

- Ratio to boa after normalising by the scalar control (global scope): `arr_sort_cmp` 1.88x, `arr_hof` 1.78x, `arr_sort_default` 1.38x, `arr_join` 1.36x; `indexOf/includes/slice/concat/shift` sit at the baseline because boa's implementations are equally spec-literal (see `no-gap/`). Fixing them is an absolute-cost win that puts jsse *ahead* of boa, and removes the 17.9% shift cost from UniPoker.

### RC4. Std SipHash (`RandomState`) on every property and variable lookup

- `src/interpreter/property_map.rs:24-31, 58-96`: objects with more than 4 own properties spill to `HashMap<JsPropertyKey, PropertyDescriptor>` with the default hasher (SipHash-1-3). Prototypes (`Array.prototype`, `String.prototype`, `%ArrayIteratorPrototype%`, `Object.prototype`) and the global object are always spilled, so each method lookup/`Symbol.iterator` lookup hashes the key bytes. Environment bindings are `HashMap<String, Binding, RandomState>` (hashbrown insert shows up in `call_function_inner_impl` -> `Environment::declare` for every call) and `resolve_identifier` hashes identifier strings too.
- Evidence: on the real workload (UniPoker, function-scoped code) `Sip13Rounds::write` + `RandomState::hash_one::<&str|&[u8]|&String>` self cost is 16.6% of all instructions (7.7 + 6.3 + 1.4 + 1.2). The global-scope repros show 15-26% (for-of 26.1%, array HOFs 20.6%, array literals >= 20.4%, destructuring 15.0%, `Object.keys` loop 17.5%, charCodeAt scan 32%), but there the callers are largely global-name resolution (`resolve_identifier`, `put_value_by_ref`, `find_binding_env`, global-object property map), see the caller tree below; treat those numbers as an upper bound for function-scoped code. Callers in UniPoker/HOF profiles: `run_chunk_inner` (`LoadName`/`ResolveName`), `Environment::declare` per call, `get_object_property_with_proxy_depth` (method lookups on spilled prototypes).
- This is shared with the `calls`/`closures` themes; it is listed here because it is the largest single self-cost in every array/string/object profile.

### RC5. `for-of`, spread and array destructuring run the generic iterator protocol (absolute cost; plain for-of is at baseline vs boa, destructuring+spread is 1.44x above it)

- `exec_for_of` -> `iterator_next` -> native `%ArrayIteratorPrototype%.next` (`setup_iterator_prototypes::{closure#7}`, via `call_function_inner_impl` including env/argument setup) -> allocates an iterator-result object, then reads `done`/`value` back through the property MOP (`iterator_complete`, `iterator_value`).
- Evidence (callgrind, `for (v of a)` over 60k elements): 1.105 G instructions = ~18.4k instructions per element; 77% inside `exec_for_of`, 27.6% in `iterator_next`, 24% in the native `next` closure; 31% under `call_function_inner_impl`. `[a, b] = q; [...q, a]` costs ~92k instructions per iteration: `get_iterator` 14.7%, `iterator_next` 30.6%, `regexp::get_symbol_key` 8.2% (the Symbol.iterator key lookup goes through a `String` decode) and 2.7% in UTF-16 decode.
- `UniPoker` spends 62% of its instructions in `exec_for_of` (`for (let player of players)`), `eval_spread_args` 37% (nested calls included).

### RC6. Object.keys / Object.assign / spread reify a descriptor object per key

- `Object.keys`: `src/interpreter/builtins/object.rs:905-982`: `proxy_own_keys` (returns `JsValue` strings, i.e. `JsPropertyKey -> JsString` UTF-8 -> UTF-16), `JsPropertyKey::from_js_string` back, `proxy_get_own_property_descriptor` (allocates a descriptor *object*), `to_property_descriptor` to read `enumerable`. `Object.assign` (:1420-1493), `Object.entries` (:1318) and object spread follow the same pattern plus `proxy_set` on the target per key.
- Evidence: callgrind of keys/assign/spread/for-in/entries loop: SipHash 9.1% + `hash_one<&[u8]>` 6.4% + `hash_one<&JsPropertyKey>` 1.9%, malloc/free ~19%, `core::str::from_utf8` 3.8%, `KeyCache::intern` 1.9%, `PropertyMap::insert` 2.7%. `proxy_own_keys` for ordinary objects (`src/interpreter/property.rs:1749`, body ~1850-1915) builds a `BTreeMap<u64,String>` for integer keys, a `String` per key and a `JsString` per key; `proxy_enumerable_keys_with_proto` (property.rs:1923) used by for-in repeats this for *every object on the prototype chain* (including `Object.prototype`) with a descriptor object per key. Per-function inclusive breakdown for `Object.keys` alone was not collected (UNVERIFIED); measured ratios: `Object.keys` loop 9.7x, `assign`+spread 3.0x, for-in+`hasOwnProperty`+`entries` 2.6x.

### RC7. Bytecode compiler coverage: whole bodies bail to the tree-walker (cross-theme)

- `src/interpreter/bytecode/compiler.rs:486` (any expression kind other than literal/identifier/unary/binary/logical/assign/call/member/new -> `Unsupported`), `:553` (`let`/`const`), `:720` (statements). Array and object literals, function expressions, `typeof`, `let`/`const`, for-in/for-of, template literals, try/switch/labeled all bail.
- Evidence (perf-counters build, BAIL/BODY tables): UniPoker: bodies bailing on `lexical declaration` hold 48.1% + 27.8% + 6.8% of AST work; Babylon: 80 of 109 bails are `lexical declaration`; pdfjs: `expression:Array` x50, `expression:Object` x35, `expression:Typeof` x51, and its top bodies (`FlateStream_getCode` 32%, `decrypt` 14%, `Type1Parser_extractFontProgram` 11%) all bail; ML's two hottest bodies (`Matrix` constructor/loops, 91% of AST work) bail on `typeof`; FlightPlanner 31 of 47 bails are `lexical declaration`.
- The array-literal repro runs the same on `--no-bytecode`, i.e. the VM gives no benefit for these bodies today.

### RC8. Default `Array.prototype.sort` compares Rust `String`s inside the comparator

- `src/interpreter/builtins/array.rs:2369-2460`: `interp.to_string_value(x)` / `(y)` per comparison (2 allocations + UTF-16 -> UTF-8), compared with `str::cmp` (UTF-8 byte order). Spec requires UTF-16 code-unit order (CompareArrayElements in Array.prototype.sort): jsse sorts `["\ue000", "\ud800\udc00"]` as `[U+E000, U+10000]` where boa/node give `[U+10000, U+E000]`. Also `toSorted` (:2465+) shares the same code shape.
- With a user comparator the sort itself is `slice::sort_by` (fine); the cost is `call_function` per comparison (82% of instructions under `call_function_inner_impl` in the comparator profile).

## Proposed work items

Ordered by expected return on the in-scope benchmarks. Each item is independently implementable; none requires the others. All ratio-based acceptance criteria refer to the **global-scope** repro files in this directory (control ratio 1.3x); the `fn-scope/` variants additionally contain the ~4x local-variable baseline and should be judged by absolute time or by ratio relative to `ctl_scalar_fn`.

### WI-1 `perf(strings): amortised O(1) string concatenation (rope or append-in-place builder)`

- Scope: `JsString` in `src/types.rs` plus the three concat sites (`eval.rs` binary `+` fast path, `apply_compound_assign`, `eval_binary_slow`) and `Op::Add` in the VM. Two viable designs:
  (a) rope/concat node in `JsString` flattened lazily by an accessor that replaces the public `code_units` field (97 uses in 13 files, mostly read-only; `grep -rn "\.code_units" src`); flatten on first indexed read, hashing, comparison or conversion;
  (b) cheaper first step: `JsString { buf: Arc<Vec<u16>>, len }` where `a + b` appends in place when `a.len == buf.len()` and the Arc is unique-or-tail-owned (like V8/SpiderMonkey "extensible strings"). Handles `s += x` and `s = s + x` without a rope.
- Risks: GC/`Arc` accounting is unaffected (no object arena involvement); lone-surrogate join semantics (a high surrogate at the end of the left operand and a low at the start of the right are plain code units, no normalisation); `JsPropertyKey::from_js_string` must flatten; `Hash`/`Eq` derive on `JsString` must compare flattened content. Memory: a flat 1 MB result held by a rope tree must not retain every intermediate (flatten + drop children).
- Acceptance: `bench_strings-arrays_str_append` (global-scope) jsse <= 0.5x boa (currently 2.58x) and N=400k within 2.3x of N=200k (linear, not quadratic); `bench_strings-arrays_str_concat3` <= 0.5x boa (3.64x); no regression in `bench_json`, `bench_regex`; `uv run python scripts/run-test262.py` green (add `test262-extra/` cases for surrogate-boundary concatenation).

### WI-1b `perf(bytecode): Op::Add must use the owned string-concat fast path`

- Evidence: `str_concat3` (`t = t + "ab" + j`) takes 3.53 s with the default VM but 2.07 s with `--no-bytecode`; single `s += "x"` shows no difference (3.58 vs 3.78 s). `Op::Add` (`src/interpreter/bytecode/vm.rs:681-730`) passes `&l`/`&r` to `eval_binary` -> `eval_binary_slow` (`eval.rs:1959`) which `to_primitive`-clones both operands, so the intermediate `t + "ab"` (uniquely owned on the stack) is copied a second time, whereas the tree-walker fast path at `eval.rs:548-575` moves its operands so `Arc::try_unwrap` succeeds.
- Scope: in the VM's `Op::Add` arm, when both operands are primitives and one is a string, call the same owned-operand concat helper the tree-walker uses (extract it to a shared function) instead of `eval_binary`.
- Acceptance: default-VM time of `str_concat3` <= its `--no-bytecode` time (<= 2.1 s); no change on numeric `+`.

### WI-2 `perf(strings): UTF-16-native String.prototype methods (no Rust String round trip)`

- Scope: `src/interpreter/builtins/string.rs`: replace `this_string_value()` (21 sites) and `utf16_units()` by `this_js_string()` + `&[u16]` operations; implement `indexOf/lastIndexOf/includes/startsWith/endsWith` with `memchr`-style u16 search (or `slice::windows`/first-unit scan); `split`/`replace`/`trim`/`toUpperCase`/`toLowerCase`/`repeat`/`pad*`/`concat` should build `Vec<u16>` directly; only `normalize`/`localeCompare`/case mapping need `char` iteration and may decode with surrogate-preserving logic. `to_str()` (Rust `String` from a JS value) should be avoided for search arguments. Prioritise the search family (`indexOf`, `lastIndexOf`, `includes`, `startsWith`, `endsWith`); split/replace/case mapping already beat boa.
- Also change `Array.prototype.join` (array.rs:1255-1310) to push `u16`s into one `Vec<u16>` instead of `Vec<String>` + `JsString::from_str`.
- Risks: surrogate pairs in case mapping (`toUpperCase` of astral chars), `String.prototype.split("")` splitting into code units, regexp-valued arguments still dispatch through `Symbol.*` first; `localeCompare`/`normalize` remain ICU-based.
- Acceptance: `str_indexof` (global-scope) <= 1.0x boa (currently 10.8x), `arr_join` <= 1.2x boa, no regression on the split/replace/toUpperCase microbenchmark in findings (jsse 0.97 s); lone-surrogate cases (`"\ud800".indexOf("\udc00") === -1`, includes/startsWith/endsWith/lastIndexOf) added to `test262-extra/`; test262 `built-ins/String` and `built-ins/Array/prototype/join` green.

### WI-3 `perf(array): dense-array fast paths for Array.prototype builtins`

- Scope: `src/interpreter/builtins/array.rs`. Generalise the guard already used by `push` (:886-985: genuine Array, not proxy, extensible, `length` data property in sync with `array_elements().len()`, no inherited indexed accessors/proxy in the chain) into a helper returning `&[JsValue]` / `&mut Vec<JsValue>` for hole-free dense arrays, and add fast paths for: `shift`, `unshift`, `pop`, `splice` (Vec::remove/splice/drain), `slice`, `concat`, `indexOf`, `lastIndexOf`, `includes`, `join`, `reverse`, `fill`, `forEach`/`map`/`filter`/`reduce`/`some`/`every`/`find*` (re-check length and element presence per iteration because callbacks may mutate the array), and `sort` write-back (assign the sorted Vec directly instead of per-index `obj_set_throw`). Fall back to the existing slow loops on any guard miss (holes are represented by `UNDEFINED` with no property entry; present-`undefined` has a property entry: `try_append_dense_array_data_property` and `numeric_index_fast_get` already encode this).
- Risks: holes vs present-undefined sentinel; callbacks shrinking/growing the array; species constructors (`array_species_create`) must still be honoured when `constructor` is not the intrinsic; GC write barriers (`gc_write_barrier_value`) for every value stored; `shape_id` invalidation (push currently does `fresh_shape_id()` per push).
- Acceptance (global-scope variants): `arr_hof` <= 1.6x boa (ctl 1.3x; currently 2.34x), `arr_sort_cmp` <= 1.6x boa (2.48x), `arr_join` <= 1.2x boa (1.79x); absolute-time cases from `no-gap/`: `arr_queue` <= 0.5 s (3.2 s; memmove instead of per-element MOP), `arr_indexof` <= 1.5 s (4.1 s), `arr_slice_concat` <= 3.0 s (7.8 s), i.e. jsse ahead of boa; test262 `built-ins/Array/**` green incl. holes/proxy/species/frozen cases.

### WI-4 `perf(runtime): replace SipHash with a fast non-DoS-resistant hasher for property maps and environment bindings`

- Scope: `src/interpreter/property_map.rs` (`HashMap<JsPropertyKey, PropertyDescriptor>` spill), `Environment::bindings` (`HashMap<String, Binding, RandomState>`), global-object and key-intern tables (`KeyCache`): use `rustc_hash::FxHashMap`/`ahash` (hash the bytes once; consider caching the hash inside `JsPropertyKey`, which is `Arc<[u8]>` today). Optionally raise `INLINE_CAP` for prototype objects or switch them to a sorted/flat map.
- Risks: iteration order is already driven by `property_order`, not the map, so behaviour is unchanged; HashDoS is not a concern for an embedded engine; check `deny.toml` for new crates (`rustc-hash` is tiny).
- Acceptance: `Sip13Rounds::write` + `RandomState::hash_one` self cost <= 3% in callgrind of UniPoker (currently 16.6%) and of the `arr_hof`/`arr_literals` repros (15-21% at global scope); >= 10% fewer instructions on UniPoker; no change in test262.

### WI-4b `perf(runtime): cache well-known-symbol property keys (no per-lookup format!/decode)`

- Scope: `get_symbol_key()` (`src/interpreter/builtins/regexp.rs:8321`, wrapper :10699; 49 call sites incl. `get_iterator` iterators.rs:4469, `array_species_create` array.rs:820, concat/spread) and `JsPropertyKey::well_known_symbol()` (`src/types.rs:166`, used by `to_primitive`, eval.rs:1438). Each call does a SipHash `HashMap<&str,_>` lookup, then `JsSymbol::to_property_key()` (`src/types.rs:385-393`): `desc.to_string()` (UTF-16 -> String), `format!("Symbol({})", desc)` (a second decode) and two heap allocations for the `Arc<[u8]>` key. Precompute all well-known keys once into fields/an array indexed by an enum.
- Evidence: `regexp::get_symbol_key` is 8.2% inclusive and its UTF-16 decode another 2.7% in the `[a,b]=q; [...q,a]` profile; it is also paid on every `map/filter/slice/splice/concat` (species) and every `to_primitive` of an object.
- Acceptance: `get_symbol_key`/`well_known_symbol` absent from the top-30 self-cost in callgrind of `arr_spread_destructure` and `arr_hof`; >= 5% fewer instructions on `arr_spread_destructure`; no behavioural change (test262 green).

### WI-5 `perf(iterators): fast path for for-of / spread / array destructuring over pristine arrays`

- Scope: `exec_for_of`, `eval_spread_args`, `get_iterator`/`iterator_next` and the array-pattern binding code. When the iterable is a genuine Array (not proxy, `ArrayData`), `Array.prototype[Symbol.iterator]` is the intrinsic `values`, and `%ArrayIteratorPrototype%.next` is the intrinsic (a realm-level "array iteration pristine" flag invalidated by writes to those two properties), iterate the backing Vec by index (re-reading `length` each step; no result objects, no native-call dispatch). Cache the `Symbol.iterator` key once (the current `get_symbol_key` goes through a String decode: 8.2% inclusive in destructuring).
- Risks: observable protocol: user-patched `next`/`Symbol.iterator`, arrays mutated during iteration, `return()` on abrupt exit (not applicable to the built-in iterator), generators/async variants keep the slow path.
- Acceptance (global-scope variants): `arr_spread_destructure` <= 1.3x boa (currently 1.90x, ctl 1.32x); `no-gap/arr_forof` <= 2.0 s absolute (3.98 s, boa 2.53 s) -- for-of is at baseline versus boa today, so this part is an absolute-cost win; test262 `language/statements/for-of`, `language/expressions/array`, `language/*/dstr` green; add `test262-extra/` cases that patch `Array.prototype[Symbol.iterator]` and `ArrayIteratorPrototype.next` mid-loop.

### WI-6 `perf(object): fast paths for Object.keys/values/entries/assign, object spread and for-in on ordinary objects`

- Scope: `src/interpreter/builtins/object.rs` (:905-982 keys, :1318 entries, :1420-1493 assign) and the object-spread / for-in enumeration code. For non-proxy, non-exotic objects iterate `property_order` once, test `enumerable` directly on the stored `PropertyDescriptor`, and write into the target with one `define_own_property`/`insert` (no descriptor object, no `JsString` <-> `JsPropertyKey` round trip). Keep the spec path for proxies, String wrappers, typed arrays, module namespaces and accessors (accessors must be invoked in order).
- Risks: key order (integer keys ascending first, then insertion order, then symbols), getters that mutate the source during `assign`, `__proto__` handling in spread targets.
- Acceptance (global-scope): `obj_keys` <= 1.5x boa (currently 8.96x), `obj_assign_spread` <= 1.5x boa (3.07x), `obj_forin_entries` <= 1.5x boa (2.60x); test262 `built-ins/Object/{keys,entries,values,assign}`, `language/expressions/object` (spread) and `language/statements/for-in` green.

### WI-7 `fix(array): default sort must compare UTF-16 code units; precompute string keys`

- Scope: `sort` / `toSorted` in array.rs (:2369-2460, :2465+): compute `to_string` once per element (decorate-sort-undecorate) and compare `Vec<u16>`/`JsString` slices lexicographically; do this in the comparator-less case only.
- Acceptance: `["\ue000","\ud800\udc00"].sort()` yields `["\ud800\udc00","\ue000"]`; `arr_sort_default` <= 1.2x boa (currently 1.82x); add a `test262-extra/` test following the `built-ins/Array/prototype/sort` pattern.

### WI-8 (cross-theme pointer) `perf(bytecode): compile array/object literals, let/const, typeof`

- Not an item for this theme, listed because RC7 gates the in-scope JetStream benchmarks (UniPoker, Babylon, FlightPlanner, pdfjs, ML). Acceptance suggestion: BAIL tables for UniPoker/Babylon/FlightPlanner list `lexical declaration`, `expression:Array`, `expression:Object`, `expression:Typeof` at 0; `arr_literals` improves under the default VM relative to `--no-bytecode`.


## What did NOT reproduce

- **Nothing in this theme benefits from the bytecode VM today**: default and `--no-bytecode` are within 10% on every repro except `str_concat3`, where the VM is 1.8x *slower* (3.53 s vs 1.97 s, WI-1b).
- **Below the keep threshold / at the scalar baseline** (kept under `no-gap/` as controls or absolute-cost cases; ratio vs boa is within 1.3x of `ctl_scalar_global`, 1.32x): `arr_queue` (shift/unshift/pop/splice on a 1000-element queue, 1.36x; boa is also O(n) per call, the original 20k-element version exceeded 100 s on both engines; the real-world cost is UniPoker's `dealOneCard`, 17.9% of jsse instructions), `arr_indexof` (1.67x), `arr_slice_concat` (1.38x), `arr_pushindex` (1.40x), `arr_forof` (1.58x), `arr_grid` (1.61x), `str_scan` (charCodeAt scan, 1.70x), `str_keys_control` (computed string keys `o["key"+i]` on a 200k-key object, 1.48x).
- **String methods that did not show a gap** (global scope, 12 KB receiver): `split(' ')`, `toUpperCase`, `toLowerCase`, `replace(str, str)`, `trim` took 0.97 s in jsse vs 2.12 s in boa (jsse 2.2x faster); short-string `indexOf/slice/substring/+/===/<` mix: 4.73 s vs 3.48 s = 1.36x (loadavg 6). So RC2 matters only for long receivers called repeatedly (`indexOf`-family), not for per-call fixed cost.
- **Controls from the brief**: `bench_regex` (1.00x) and `bench_json` (0.68x, jsse wins) were not re-measured here; nothing in this theme touches them. The WI-1/WI-2 changes must not regress them.
- **Not profiled (UNVERIFIED)**: pdfjs/Babylon/ML/FlightPlanner string/array shares beyond the BAIL/BODY tables (a callgrind of pdfjs was started and abandoned for time); `includes/startsWith/endsWith/lastIndexOf` timings (same `this_string_value` pattern as `indexOf`, not timed); `unshift`/`splice` individual costs; per-function split of `Object.keys` cost beyond the inclusive numbers in RC6.

## Reproduction commands

```
R=/home/igalia/pmatos/dev/bench-workspace/run-2026-10-10
python3 $R/microbench.py '{"jsse":["'$R'/jsse-head/target/release/jsse"],"boa":["'$R'/boa-v0.22-src/target/release/boa"]}' <this dir | this dir/fn-scope | this dir/no-gap> out.json 5
python3 $R/microbench.py '{"jsse_nobc":["'$R'/jsse-head/target/release/jsse","--no-bytecode"]}' <dir> out-nobc.json 3
# profile a repro (scale N down 50x first): valgrind --tool=callgrind --callgrind-out-file=cg.out $R/jsse-head/target/release/jsse small.js ; callgrind_annotate [--inclusive=yes] cg.out
# perf-counters (BAIL/BODY/OP) build: cargo +nightly build --release --features perf-counters ; run the JetStream script, read stderr
# conformance checks:
jsse -e 'console.log("\ud800".indexOf("\udc00"), "\ud800".includes("\udc00"), "a\ud800".lastIndexOf("\udc00"), "\ud800x".startsWith("\udc00"))'   # want: -1 false -1 false
jsse -e 'console.log(["\ue000","\ud800\udc00"].sort().map(function(s){return s.length}).join())'                                                      # want: 2,1
```
