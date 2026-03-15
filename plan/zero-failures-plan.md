# Plan: Zero test262 Failures (118 remaining)

Based on investigation of all failing tests. Phase 1 parser fixes completed 2026-03-14 (+26 passes, 0 regressions). Phase 2 interpreter property/prototype fixes completed 2026-03-14 (+12 passes, -1 regression). Phase 2 continued: generator/proxy/global/Reflect fixes completed 2026-03-14 (+9 passes, 0 regressions → 101,065 / 101,234 = 99.83%). Phase 3 RegExp engine fixes completed 2026-03-14 (+16 net passes, 0 regressions → 101,081 / 101,234 = 99.85%). Phase 4 Iterator helper fixes completed 2026-03-15 (+16 net passes, 0 regressions → 101,097 / 101,234 = 99.86%). Phase 5 TypedArray fixes completed 2026-03-15 (+19 net passes + 11 bonus, 0 regressions → 101,116 / 101,234 = 99.88%). Phase 6 Set operation fixes completed 2026-03-15 (+6 passes, 0 regressions → 101,122 / 101,234 = 99.89%).

## Breakdown by Category (updated 2026-03-14)

| Category | Failing Tests | Unique Files |
|----------|:---:|:---:|
| Intl402/Temporal | 36 | 18 |
| RegExp (SM + built-ins) | 8 | 5 |
| ~~Iterator helpers~~ | ~~16~~ | ~~8~~ |
| ~~TypedArray (SM + built-ins)~~ | ~~22~~ | ~~11~~ |
| ~~Set operations~~ | ~~6~~ | ~~3~~ |
| Generators | 3 | 2 |
| Async functions | 4 | 2 |
| Class features | 2 | 1 |
| Expressions/destructuring | 0 | 0 |
| Function | 3 | 2 |
| Explicit resource management | 4 | 2 |
| Other (Proxy, Reflect, JSON, etc.) | 49 | 28 |
| **Total** | **~118** | **~65** |

---

## Phase 1: Quick Wins — Parser & Spec-Compliance Fixes ✅ COMPLETED (2026-03-14)

**Result: +26 passes, 0 regressions.**

### 1.1 Strict-mode destructuring defaults (strict-only) — 6 tests ⏭️ SKIPPED
**Verdict:** These are **test262 bugs**, not JSSE bugs. They fail on Node too. No fix needed.

### 1.2 `using`/`await using` inside switch-case blocks — 4 tests ❌ WRONG FIX
**Verdict:** The spec (§14.3.1.1) explicitly prohibits `using`/`await using` directly in CaseClause/DefaultClause StatementLists. The staging tests that expect this to work are outdated. JSSE's existing rejection is correct. However, `dispose_resources` was missing from switch statement exits and class static blocks — that was fixed (+4 passes from `call-dispose-methods.js` static block case and other disposal tests).

### 1.3 `new o?.C()` should be parse-time SyntaxError ✅ DONE
**Fix:** Added `Token::OptionalChain` check in `parse_new_expression` member access loop.

### 1.4 Future reserved words as function names in strict mode — 2 tests ✅ DONE (+2)
**Fix:** Added `yield`/`let`/`static` to `is_strict_reserved_word`. Added retroactive `is_strict_reserved_word` check in both `parse_function_declaration` and `parse_function_expression` when `body_strict` is true.

### 1.5 `async await => expr` — 2 tests ✅ DONE (+2)
**Fix:** Added `await` rejection after `check_strict_binding_identifier` in async single-param arrow path.

### 1.6 `await` as identifier in non-async function inside async — 2 tests ✅ DONE (+2)
**Fix:** Reset `in_async = false` in `parse_function_expression()` before parsing params. Also fixed `in_formal_parameters` not being reset in `parse_function_body_inner`, which caused `await` in nested async function bodies inside parameter defaults to be wrongly rejected.

### 1.7 `{async async: value}` in destructuring context — 2 tests ✅ DONE (+2)
**Fix:** In `parse_object_pattern`, reject `async` shorthand followed by non-property tokens (i.e., looks like async method modifier, which is invalid in binding patterns).

### 1.8 Parenthesized destructuring sub-patterns — 2 tests ✅ DONE (+2)
**Fix:** Wrap parenthesized Object/Array/Assign expressions in single-element `Sequence` marker during `parse_primary_expression`. In `validate_destructuring_target`, reject single-element Sequence containing Object/Array, delegate for Identifier/Member. Also removed sloppy-mode call expression acceptance from `validate_destructuring_target` (call expressions should never be valid destructuring targets).

### 1.9 `{a = 0}.x` — CoverInitializedName in non-pattern context — 2 tests ✅ DONE (+2)
**Fix:** In `parse_left_hand_side_expression`, after getting primary expression, check `has_cover_initialized_name` and reject if followed by member access/call/template tokens.

### 1.10 `{if}` destructuring with reserved word — 2 tests ✅ DONE (+2)
**Fix:** In `parse_object_pattern` shorthand path, check `is_reserved_identifier(name, false)` and reject reserved words as binding identifiers.

### 1.11 `await` in class field initializer (module mode) — 1 test ✅ DONE (+1)
**Fix:** In `parse_field_initializer_value`, increment `in_function` to prevent module-level await condition (`is_module && in_function == 0`) from matching inside field initializers.

### 1.12 Function constructor parameter list validation — 2 tests ✅ DONE (+2)
**Fix:** Parse parameter string and body string independently before combining, per spec §20.2.1.1. Applied to all four constructors: Function, AsyncFunction, GeneratorFunction, AsyncGeneratorFunction.

### 1.13 `eval('super.prop')` in non-method nested function — 2 tests ✅ DONE (+2)
**Fix:** In `perform_eval` env walk, track `function_boundary_count`. Only find `__home_object__` and `is_derived_constructor_scope` when `function_boundary_count <= 1` (allows the method's own closure but blocks nested non-method functions).

### 1.14 `import.source` wrong error type — 2 tests ⏭️ ALREADY PASSING
**Verdict:** Already uses `create_error("SyntaxError", ...)`. Tests were already passing.

### Additional fixes (not in original plan)
- **`dispose_resources` in switch exits**: All exit paths in `exec_switch` now call `dispose_resources` on the switch environment (+2 passes from `await-using-in-switch-case-block.js` disposal).
- **`dispose_resources` in class static blocks**: Static block body result now goes through `dispose_resources` (+2 passes from `call-dispose-methods.js` static block case).

---

## Phase 2: Interpreter Property/Prototype Fixes ✅ COMPLETED (2026-03-14)

**Result: +21 net passes (+22, -1 regression) across two sub-sessions.**

### 2.1 `try/finally` property assignments silently rolled back — 2+ tests ⏭️ PARTIALLY ADDRESSED
**Note:** The `generators/return-finally.js` tests are fixed via 2.2. The `RegExp/lastIndex-match-or-replace.js` test remains a timeout (likely a separate infinite loop issue, not try/finally).

### 2.2 Generator `yield` in `finally` block skipped on `try { return }` — 2 tests ✅ DONE (+2)
**Fix:** Three changes in `eval.rs` state machine:
- `Completion::Return` from `exec_statements` checks try_stack for enclosing finally blocks before completing
- `StateTerminator::Return` checks try_stack for enclosing finally blocks
- `TryExit` propagates `pending_return` through enclosing try-finally blocks
- `generator_throw_state_machine` preserves `stored_pending_return` when routing to catch/finally handlers
- `StateTerminator::Completed` uses `pending_return` as the completion value

### 2.3 Generator `yield` inside `throw` — sent value lost in try-catch — 1 test ⏸️ NOT ADDRESSED
**Tests:** `generators/iteration.js` — pre-existing failure (likely `$262.gc()` issue)

### 2.4 Generator.return() doesn't close inner for-of iterators — 2 tests ✅ DONE (+2)
**Fix:** `StateTerminator::Yield` handler now saves `pending_iter_close` to `generator_inline_iters`, matching the `Completion::Yield` path.

### 2.5 `access_property_on_value` missing Symbol/BigInt primitives — 3+ tests ✅ DONE (prior session)

### 2.6 `super.prop = value` doesn't trigger proxy set trap — 2 tests ✅ DONE (prior session, +2)
**Additional fix:** `object/proto-property-change-writability-set.js` fixed via global identifier set using `[[Set]]` through prototype chain (+1).

### 2.7 Class name TDZ in computed property keys — 2 tests ✅ DONE (prior session, +2)

### 2.8 `eval("var x;")` replaces accessor property on global object — 1 test ✅ DONE (prior session, +1)

### 2.9 Annex B: block-scoped function doesn't override `arguments` — 2 tests ✅ DONE (prior session, +2, -1 regression)

### 2.10 Array destructuring holes should consult prototype — 2 tests ✅ DONE (prior session, +2)

### 2.11 Proxy `[[GetOwnProperty]]` missing enumerable invariant check — 2 tests ✅ DONE (prior session, +2)

### 2.12 Proxy global prototype `has` trap not consulted for bare names — 2 tests ✅ DONE (+2)
**Fix:** `resolve_global_getter` walks prototype chain via `proxy_has_property`. `resolve_identifier_ref` checks prototype chain for resolution. `put_value_by_ref` uses `proxy_set` on global object for writes, respecting setters and Proxy traps. Added `HasProperty` re-check per §9.1.1.2.5 SetMutableBinding for strict mode deleted-then-assigned scenario.

### 2.13 `Function.prototype.apply` doesn't call `ToUint32` on length — 1 test ✅ DONE (prior session, +1)

### 2.14 Function name not set for for-in initializer — 1 test ⏸️ PARTIAL (prior session)
**Note:** Parser issue blocks full fix.

### 2.15 `Reflect.apply` wrong error type — 1 test ✅ DONE (prior session, +1)

### 2.16 `Reflect.set` cross-realm — 2 tests ✅ DONE (+2)
**Fix:** Added `sync_global_object_binding` helper that syncs property writes to realm global env bindings when the target object is a realm's global object. Also fixed Reflect.set builtin to check for undefined setter before calling it (bonus fix for `Reflect.set` with getter-no-setter).

### 2.17 `await using` disposal timing without await — 2 tests ⏸️ NOT ADDRESSED
**Reason:** Requires deep architectural change — each async disposal needs to yield to the microtask queue rather than running synchronously via `await_value`. The `dispose_resources` function would need to be integrated into the async function's state machine.

---

## Phase 3: RegExp Engine Fixes ✅ COMPLETED (2026-03-14)

**Result: +16 net passes (+26 new, 0 regressions). Includes bonus +10 from regexp-modifiers `\b`/`\w` fixes.**

### 3.1 `\b`/`\B` word boundary: ASCII-only vs Unicode — 2 tests ✅ DONE (+2, +10 bonus)
**Fix:** Always emit custom ASCII lookaround for `\b`/`\B` instead of Rust's Unicode-aware `\b`. Under `/iu` (current modifier state), expand word char set to include U+017F/U+212A. Use `(?-i:...)` wrappers to prevent Unicode case folding from expanding char classes. Handle `\b` inside char class as backspace U+0008. Bonus: 10 previously-passing `regexp-modifiers` tests that test `(?i:\b)`/`(?-i:\b)`/`(?-i:\w)` now also pass.

### 3.2 `\w`/`\W` with `/iu` — Unicode case-folding includes U+017F, U+212A — 2 tests ✅ DONE (+2)
**Fix:** When `unicode && icase` (current modifier), expand `\w` to `[A-Za-z0-9_\x{017F}\x{212A}]` and `\W` to its complement. Uses current `icase` state (not `icase_base`) so `(?-i:\w)` inside `/iu` correctly uses ASCII-only.

### 3.3 `\-` in char class with `/u` flag — 2 tests ✅ DONE (+2)
**Fix:** In identity escape handler, when `in_char_class && next == '-'`, emit `\\-` instead of calling `push_literal_char` (which didn't escape the dash).

### 3.4 `get_substitution` panics on multi-byte UTF-8 — 2 tests ✅ DONE (+2)
**Fix:** Changed `get_substitution` to accept `tail_pos` (byte offset) instead of `match_length`. Call site computes `tail_pos` via `utf16_to_byte_offset(s_slice, position_utf16 + match_length_utf16)` ensuring char-boundary-safe slicing. Guarded `$`` and `$'` with `.get()` for safety.

### 3.5 RegExp constructor spec step ordering — 4 tests ✅ DONE (+4)
**Fix:** Set `deferred_construct = true` on RegExp constructor. Restructured constructor body to match spec §22.2.3.1: (1) extract source from pattern, (2) `get_prototype_from_new_target_realm` (prototype lookup), (3) `ToString` flags. Uses `RawFlags` enum to defer flag stringification. Fixes both `constructor-ordering.js` (+2) and `constructor-ordering-2.js` (+2).

### 3.6 `RegExpBuiltinExec` re-read after `ToLength(lastIndex)` side effects — 4 tests ✅ DONE (+4)
**Fix:** In `regexp_exec_raw`, moved `ToLength(lastIndex)` before flag derivation. After ToLength, re-read `regexp_original_source` and `regexp_original_flags` from internal slots (may have changed via `compile()` in `lastIndex.valueOf()`). Re-derive `global`/`sticky`/`unicode`/`has_indices` from refreshed flags.

### 3.7 Lone surrogates in `/u` mode char classes and `\P{...}` — 4 tests ⏸️ DEFERRED
**Reason:** Tests timeout (120s) due to enormous character ranges in property escape tests. The `unicode-class-braced.js` test creates a 2^24-character string that overwhelms the parser. Needs performance optimization, not correctness fix.

### 3.8 Duplicate named groups — unified capture slots — 2 tests ✅ DONE (+2)
**Fix:** Modified `rename_groups_and_backrefs` to rename ALL capturing groups (named and unnamed) in earlier qi-expanded iterations with `__jsse_qi` prefix, not just dup-named ones. Unnamed groups `(...)` are converted to `(?<__jsse_qi{idx}_u{n}>...)`. This ensures `strip_renamed_qi_captures` removes all extra groups from unrolled iterations.

### 3.9 Capture groups not reset in quantified alternation — 2 tests ⏸️ DEFERRED
**Reason:** Fundamentally hard — `fancy_regex` doesn't implement per-iteration capture reset (spec §22.2.2.5.1). The match result itself depends on stale backreferences, so post-hoc correction is impossible. Would require pattern rewrite to unroll quantified groups with backreferences.

### 3.10 `lastIndex-match-or-replace.js` timeout — 2 tests ⏸️ DEFERRED
**Reason:** Infinite loop in `@@match` global loop with custom `exec` on DuckRegExp subclass. Separate investigation needed into lastIndex advancement logic.

---

## Phase 4: Iterator Helper Fixes ✅ COMPLETED (2026-03-15)

**Result: +16 net passes, 0 regressions.**

### 4.1 `flatMap` doesn't close outer iterator when inner value throws — 2 tests ✅ DONE (+2)
**Fix:** Added `IfAbruptCloseIterator` pattern to the inner `iterator_value` error path — sets alive=false and calls `iterator_close_getter` on the outer iterator before rethrowing.

### 4.2+4.4 WrapForValidIteratorPrototype simplification + cross-realm state — 6 tests ✅ DONE (+4 wrapper, +2 cross-realm)
**Fix:** Two spec violations fixed together:
- Removed `alive` boolean tracking — spec has no such state. `next()` always delegates `Call(nextMethod, iterator)` directly without validating result type or checking completion. `return()` always looks up and calls `return` on the underlying iterator.
- Eliminated per-realm `HashMap<u64, (JsValue, JsValue, bool)>` state map. Iterator record now stored directly on the wrapper object via `wrap_iter_record: Option<(JsValue, JsValue)>` field on `JsObjectData`. This fixes cross-realm calls where `thisWrap.next.call(otherWrap)` failed because `otherWrap` was in a different realm's map.
**Tests:** `modify-return.js`, `wrap-next-not-object-throws.js`, `wrap-functions-on-other-global.js` (x2 each)

### 4.3 `Iterator.from` prototype check bypasses Proxy `getPrototypeOf` — 6 tests ✅ DONE (+6)
**Fix:** Replaced manual `Rc::ptr_eq` prototype chain walk with `ordinary_has_instance(&iterator_ctor, &iter_val)`, which uses `proxy_get_prototype_of` and properly consults Proxy `getPrototypeOf` handlers.
**Tests:** `proxy-not-wrapped.js`, `proxy-wrap-next.js`, `proxy-wrap-return.js` (x2 each)

### 4.5 Cross-realm iterator helper prototype methods — 2 tests ✅ DONE (+2)
**Fix:** Created proper `%IteratorHelperPrototype%` per realm with shared `next` and `return` methods that read state from `this`. Per-instance closures and generator state stored on `JsObjectData` fields (`helper_next_closure`, `helper_return_closure`, `helper_gen_state`). Previously `next`/`return` were own closure properties per instance, so cross-realm `OtherHelper.prototype.next.call(thisHelper)` failed. Added `iterator_helper_prototype` to `Realm` struct and GC root list.
**Tests:** `iterator-helpers-from-other-global.js` (x2)

---

## Phase 5: TypedArray Fixes ✅ COMPLETED (2026-03-15)

**Result: +19 net passes (+30 total including bonus), 0 regressions.**

### 5.1+5.2 TypedArray `[[PreventExtensions]]` + integrity levels — 11 tests ✅ DONE (+11)
**Fix:** Added `is_typed_array_fixed_length` helper (§10.4.5). Implemented TypedArray-aware checks in:
- `Object.preventExtensions` — throws TypeError for non-fixed-length TAs
- `Reflect.preventExtensions` — returns `false` for non-fixed-length TAs
- `Object.seal` — preventExtensions check first (sets extensible=false), then throws if TA has elements (§10.4.5.3 rejects configurable:false)
- `Object.freeze` — same pattern; also fixed to use `typed_array_length(ta)` for length-tracking TAs
- `proxy_own_keys` — uses `typed_array_length` + `is_typed_array_out_of_bounds` for TA virtual indices
**Tests:** `seal-and-freeze.js`, `test-integrity-level.js`, `test-integrity-level-detached.js`, `preventExtensions-variable-length-typed-arrays.js`, `seal-variable-length-typed-arrays.js`, `Reflect/preventExtensions-variable-length-typed-arrays.js` (x2 each except seal-and-freeze which is onlyStrict)

### 5.3 TypedArray constructor buffer-path step ordering — 2 tests ✅ DONE (+2)
**Fix:** Moved `get_proto` (GetPrototypeFromConstructor) before `to_index(byteOffset)`. Moved modulo check (step 7) before detach check (step 9). Added detach check in no-length path. Re-read `buf_len` after potential side effects.
**Tests:** `constructor-buffer-sequence.js` (x2)

### 5.4 `TypedArray.from` step ordering and constructor check — 2 tests ✅ DONE (+2)
**Fix:** Rewrote `TypedArray.from` per §23.2.2.1: added `IsConstructor(C)` check (step 2), get `@@iterator` from raw source before `ToObject` (step 4), separate iterable vs array-like paths. Array-like path: get length → create TA → get elements one by one. Fixed `ToLength` to use `interp.to_number_value` (calls `valueOf`).
**Tests:** `from_errors.js` (x2)

### 5.5 `typed_array_create` cross-realm prototype — 4 tests ✅ DONE (+4)
**Fix:** Fast path now reads prototype from constructor's `.prototype` property instead of using `self.get_typed_array_prototype(kind)` (current realm). This correctly handles cross-realm constructors.
**Tests:** `from_realms.js`, `of.js` (x2 each)

### 5.6 Bonus passes from improved TypedArray.from — +11 bonus
The `iterate_with_function` helper and restructured array-like path fixed additional edge cases in other `TypedArray/from_*` tests.

---

## Phase 6: Set Operation Fixes ✅ COMPLETED (2026-03-15)

**Result: +6 passes, 0 regressions.**

### 6.1 Set operations don't handle concurrent modification — 6 tests ✅ DONE (+6)
**File:** `src/interpreter/builtins/collections.rs`
**Fix:** Three changes:
- `union`: moved `get_keys_iterator()` before `set_data` snapshot (spec requires copying `O.[[SetData]]` after `GetIteratorFromMethod`, which may trigger `.next` getter side effects)
- `symmetricDifference`: same reorder as union
- `intersection` has-path: replaced frozen snapshot iteration with index-based live iteration (matching `isSubsetOf` pattern), with `set_data_has` duplicate guard
**Tests:** `Set/intersection.js`, `Set/symmetric-difference.js`, `Set/union.js` (x2 each)

---

## Phase 7: Intl402 & Temporal (~36 tests)

### 7.1 Non-ISO calendar support for PlainMonthDay/PlainYearMonth — 14 tests
**File:** Temporal builtins
**Bug:** Only accepts `iso8601` calendar; needs `islamic-civil`, `hebrew`, etc.

### 7.2 Intl.DateTimeFormat hour formatting — 6 tests
**File:** Intl builtins
**Bug:** Leading zeros in 12-hour format (`09:23` vs `9:23`).

### 7.3 Locale-specific date formatting — 4 tests
**File:** Intl builtins
**Bug:** German locale uses US format instead of `DD.MM.YYYY`.

### 7.4 Intl constructor cross-realm prototype fallback — 6 tests
**File:** Intl builtins
**Bug:** `Reflect.construct` with cross-realm newTarget falls back to wrong prototype.

### 7.5 Temporal timezone offset parsing — 8 tests
**File:** Temporal string parser
**Bug:** Doesn't accept offset formats without colons (`-040000`).

### 7.6 Removed Temporal methods still exposed — 2 tests
**File:** Temporal builtins
**Bug:** `getISOFields`, `getCalendar`, `getTimeZone`, etc. still present.

### 7.7 Other Intl fixes — 2 tests
- PluralRules locale data incomplete (1 test)
- Collator `usage: "search"` not implemented (1 test)

---

## Phase 8: Miscellaneous (~16 tests)

### 8.1 Stack overflow crashes instead of catchable error — 1 test
**File:** `src/main.rs` or interpreter loop
**Fix:** Use `stacker` crate or manual stack depth tracking.

### 8.2 `Error.prototype.stack` not implemented — 2 tests
**File:** `src/interpreter/builtins/mod.rs`
**Fix:** Add `.stack` property to Error objects (non-standard but widely used).

### 8.3 `JSON.rawJSON` symbol property leak — 2 tests
**File:** `src/interpreter/builtins/mod.rs`
**Fix:** Ensure `rawJSON` is stored as string property, not symbol.

### 8.4 `Array.prototype.toLocaleString` this-boxing in strict mode — 1 test
**File:** `src/interpreter/builtins/array.rs`
**Fix:** Don't box primitive `this` in strict mode calls.

### 8.5 `Array.prototype[Symbol.unscopables]` includes `groupBy` — 1 test
**File:** `src/interpreter/builtins/array.rs`
**Fix:** Remove `groupBy` from unscopables.

### 8.6 `Array.from` over-closing iterator on value getter throw — 2 tests
**File:** `src/interpreter/builtins/array.rs`
**Fix:** Don't call iterator `return()` when value getter throws.

### 8.7 `Array.prototype.filter` species sets spurious length — 2 tests
**File:** `src/interpreter/builtins/array.rs`

### 8.8 `Function.prototype.toString` for Symbol-keyed built-ins — 2 tests
**File:** `src/interpreter/builtins/mod.rs`
**Fix:** Include `[Symbol.split]` etc. in toString output.

### 8.9 Unicode 16.0 case mapping gaps — 2 tests
**File:** `src/interpreter/helpers.rs`

### 8.10 `BigInt.asIntN` OOM on large bit counts — 2 tests
**File:** `src/types.rs`
**Fix:** Optimize for small values with large bit counts.

### 8.11 Date parser: non-ISO space-separated format — 2 tests
**File:** `src/interpreter/builtins/date.rs`

### 8.12 Atomics: detached buffer check after argument coercion — 2 tests
**File:** `src/interpreter/builtins/` (Atomics)

### 8.13 Decorators: auto-accessor incomplete — 2 tests
**File:** `src/interpreter/exec.rs`

### 8.14 Memory/performance (OOM under 512MB) — 3 tests
**Tests:** `parse-mega-huge-array.js`, `regress-610026.js`, `regress-1507322-deep-weakmap.js`
**Fix:** Reduce memory overhead for large JSON arrays, deeply nested blocks, deep WeakMap chains.

### 8.15 `String.replace` timeout on huge backreference — 1 test
**Tests:** `String/replace-math.js`
**Fix:** Optimize replacement with capture-group backreferences on large strings.

---

## Priority Order (by impact and feasibility)

| Priority | Phase | Tests Fixed | Difficulty |
|:---:|:---:|:---:|:---:|
| ~~1~~ | ~~2.1 try/finally bug~~ | ~~4+~~ | ~~DONE~~ |
| ~~2~~ | ~~1.x parser fixes~~ | ~~+26~~ | ~~DONE~~ |
| ~~3~~ | ~~2.x interpreter fixes~~ | ~~+21~~ | ~~DONE~~ |
| ~~4~~ | ~~3.x RegExp fixes~~ | ~~+16~~ | ~~DONE~~ |
| ~~5~~ | ~~4.x Iterator helpers~~ | ~~+16~~ | ~~DONE~~ |
| ~~6~~ | ~~5.x TypedArray~~ | ~~+30~~ | ~~DONE~~ |
| ~~7~~ | ~~6.x Set operations~~ | ~~+6~~ | ~~DONE~~ |
| 8 | 7.x Intl/Temporal | ~36 | Hard (locale data) |
| 9 | 8.x Miscellaneous | ~16 | Mixed |
| — | Test262 bugs (1.1) | 6 | N/A |
| — | RegExp deferred (3.7-3.10) | 8 | Hard (perf/engine) |

**Progress: 101,122 / 101,234 (99.89%). Remaining fixable: ~112 tests** (6 are test262 bugs, ~7 are OOM/timeout performance issues, ~8 are deferred RegExp engine/perf issues, ~6 are hard locale data issues).
