# Plan: Zero test262 Failures (190 remaining)

Based on investigation of all failing tests. Phase 1 parser fixes completed 2026-03-14 (+26 passes, 0 regressions → 101,044 / 101,234 = 99.81%).

## Breakdown by Category (updated 2026-03-14)

| Category | Failing Tests | Unique Files |
|----------|:---:|:---:|
| Intl402/Temporal | 36 | 18 |
| RegExp (SM + built-ins) | 24 | 12 |
| Iterator helpers | 16 | 8 |
| TypedArray (SM + built-ins) | 22 | 11 |
| Generators | 8 | 4 |
| Async functions | 4 | 2 |
| Class features | 2 | 1 |
| Expressions/destructuring | 0 | 0 |
| Set operations | 6 | 3 |
| Function | 3 | 2 |
| Explicit resource management | 6 | 3 |
| Other (Proxy, Reflect, JSON, etc.) | 54 | 31 |
| **Total** | **~190** | **~95** |

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

## Phase 2: Interpreter Property/Prototype Fixes (~40 tests)

### 2.1 `try/finally` property assignments silently rolled back — 2+ tests
**File:** `src/interpreter/exec.rs`
**Bug:** Property assignments (e.g., `this.lastIndex = val`) inside `finally` blocks are not persisted when `try` block has a `return`. This also causes the RegExp `lastIndex-match-or-replace.js` test to timeout (infinite loop).
**Tests:** `RegExp/lastIndex-match-or-replace.js`, `generators/return-finally.js` (x2 each)
**Impact:** This is a critical interpreter bug affecting correctness broadly.

### 2.2 Generator `yield` in `finally` block skipped on `try { return }` — 2 tests
**File:** `src/interpreter/exec.rs`
**Bug:** `try { return X; } finally { yield Y; }` skips the yield.
**Tests:** `generators/return-finally.js` (x2) — overlaps with 2.1

### 2.3 Generator `yield` inside `throw` — sent value lost in try-catch — 1 test
**File:** `src/interpreter/eval.rs`
**Bug:** `throw (yield expr)` doesn't use the sent value as the thrown value.
**Tests:** `generators/iteration.js`

### 2.4 Generator.return() doesn't close inner for-of iterators — 2 tests
**File:** `src/interpreter/exec.rs`
**Bug:** Inner for-of iterator's `.return()` not called when outer generator returns.
**Tests:** `generators/yield-iterator-close.js` (x2)

### 2.5 `access_property_on_value` missing Symbol/BigInt primitives — 3+ tests
**File:** `src/interpreter/eval.rs` (lines 1247-1283)
**Bug:** Optional chaining on Symbol/BigInt primitives returns `undefined`.
**Fix:** Add `JsValue::Symbol(_)` and `JsValue::BigInt(_)` arms that look up prototypes.
**Tests:** Iterator `proxy-not-wrapped.js`, `proxy-wrap-next.js`, `proxy-wrap-return.js` (partially)

### 2.6 `super.prop = value` doesn't trigger proxy set trap — 2 tests
**File:** `src/interpreter/eval.rs`
**Bug:** `super` property set creates own property instead of consulting prototype chain Proxy.
**Tests:** `class/superPropProxies.js`, `object/proto-property-change-writability-set.js` (x2 each)

### 2.7 Class name TDZ in computed property keys — 2 tests
**File:** `src/interpreter/eval.rs` or `exec.rs`
**Bug:** `class Bar { [Bar]() {} }` doesn't throw ReferenceError for TDZ access.
**Tests:** `class/innerBinding.js` (x2)

### 2.8 `eval("var x;")` replaces accessor property on global object — 1 test
**File:** `src/interpreter/exec.rs`
**Bug:** `eval("var x;")` overwrites existing accessor with `undefined` data property.
**Tests:** `global/bug-320887.js`

### 2.9 Annex B: block-scoped function doesn't override `arguments` — 2 tests
**File:** `src/interpreter/exec.rs`
**Bug:** `{ function arguments() {} }` should override `arguments` binding per Annex B.3.3.
**Tests:** `lexical-environment/block-scoped-functions-annex-b-arguments.js`, `regress/regress-602621.js`

### 2.10 Array destructuring holes should consult prototype — 2 tests
**File:** `src/interpreter/eval.rs`
**Bug:** `[x, y, z] = ['x', , 'z']` treats hole as `undefined` instead of absent property.
**Tests:** `regress/regress-469625-02.js` (x2)

### 2.11 Proxy `[[GetOwnProperty]]` missing enumerable invariant check — 2 tests
**File:** `src/interpreter/eval.rs`
**Bug:** Proxy handler can report different `enumerable` for non-configurable target property.
**Tests:** `regress/regress-1383630.js` (x2)

### 2.12 Proxy global prototype `has` trap not consulted for bare names — 2 tests
**File:** `src/interpreter/eval.rs`
**Bug:** Variable resolution doesn't call `has` trap on global's prototype Proxy.
**Tests:** `Proxy/global-receiver.js` (x2)

### 2.13 `Function.prototype.apply` doesn't call `ToUint32` on length — 1 test
**File:** `src/interpreter/eval.rs`
**Bug:** `apply` doesn't coerce `arguments.length` via `ToUint32`.
**Tests:** `Function/15.3.4.3-01.js`

### 2.14 Function name not set for for-in initializer — 1 test
**File:** `src/interpreter/exec.rs`
**Bug:** `for (var f = function() {} in {})` should set `f.name`.
**Tests:** `Function/function-name-for.js`

### 2.15 `Reflect.apply` wrong error type — 1 test
**File:** `src/interpreter/builtins/mod.rs`
**Bug:** Non-callable throws ReferenceError instead of TypeError.
**Tests:** `Reflect/apply.js`

### 2.16 `Reflect.set` cross-realm — 1 test
**File:** `src/interpreter/builtins/mod.rs`
**Bug:** Cross-realm property writes fail.
**Tests:** `Reflect/set.js`

### 2.17 `await using` disposal timing without await — 2 tests
**File:** `src/interpreter/exec.rs`
**Bug:** Both disposals run eagerly instead of only the first.
**Tests:** `await-using-in-async-function-call-without-await.js` (x2)

---

## Phase 3: RegExp Engine Fixes (~20 tests)

### 3.1 `\b`/`\B` word boundary: ASCII-only vs Unicode — 2 tests
**File:** `src/interpreter/builtins/regexp.rs`
**Bug:** Always uses Unicode word boundary; should use ASCII-only except under `/iu`.
**Tests:** `unicode-ignoreCase-word-boundary.js` (x2)

### 3.2 `[^\W]` / `[^\w]` with `/iu` — Unicode case-folding in negated classes — 2 tests
**File:** `src/interpreter/builtins/regexp.rs`
**Bug:** Negated `\W`/`\w` in char class doesn't include U+017F, U+212A under `/iu`.
**Tests:** `unicode-ignoreCase-escape.js` (x2)

### 3.3 `\-` in char class with `/u` flag — 2 tests
**File:** `src/interpreter/builtins/regexp.rs`
**Bug:** `\-` not treated as literal dash, interpreted as range operator.
**Tests:** `unicode-disallow-extended.js` (x2)

### 3.4 `get_substitution` panics on multi-byte UTF-8 — 2 tests
**File:** `src/interpreter/builtins/regexp.rs`
**Bug:** Mixed byte/char indexing causes panic on CJK strings.
**Tests:** `replace-twoBytes.js` (x2)

### 3.5 RegExp constructor reads source/flags after prototype access — 2 tests
**File:** `src/interpreter/builtins/regexp.rs`
**Bug:** Wrong spec step ordering — source/flags should be read before `new.target.prototype`.
**Tests:** `constructor-ordering.js` (x2)

### 3.6 `RegExpBuiltinExec` doesn't re-read matcher after `ToLength(lastIndex)` side effects — 4 tests
**File:** `src/interpreter/builtins/regexp.rs`
**Bug:** `compile()` inside `lastIndex.valueOf()` doesn't affect current exec.
**Tests:** `match-local-tolength-recompilation.js`, `replace-local-tolength-recompilation.js` (x2 each)

### 3.7 Lone surrogates in `/u` mode char classes and `\P{...}` — 4 tests
**File:** `src/interpreter/builtins/regexp.rs`
**Bug:** Lone surrogates dropped/paired during UTF-8 encoding.
**Tests:** `unicode-class-braced.js`, `General_Category_-_Private_Use.js`, `General_Category_-_Surrogate.js` (x2 each)

### 3.8 Duplicate named groups share capture slots — 2 tests
**File:** `src/interpreter/builtins/regexp.rs`
**Bug:** Each `(?<a>...)` gets its own slot instead of sharing.
**Tests:** `duplicate-named-groups.js` (x2)

### 3.9 Capture groups not reset in quantified alternation — 2 tests
**File:** `src/interpreter/builtins/regexp.rs`
**Bug:** `(?:^(a)|\1(a)|(ab)){2}` doesn't reset captures per iteration.
**Tests:** `regress-613820-3.js` (x2)

---

## Phase 4: Iterator Helpers (~16 tests)

### 4.1 WrapForValidIteratorPrototype `.return()` alive-state tracking — 2 tests
**File:** `src/interpreter/builtins/iterators.rs`
**Bug:** `return` method incorrectly tracks alive state; should always delegate.
**Tests:** `Iterator/from/modify-return.js`, `proxy-wrap-return.js` (partially)

### 4.2 Wrapper `next` incorrectly validates return type — 2 tests
**File:** `src/interpreter/builtins/iterators.rs`
**Bug:** Non-object results rejected instead of passed through.
**Tests:** `Iterator/from/wrap-next-not-object-throws.js` (x2)

### 4.3 `flatMap` doesn't close outer iterator when inner value throws — 2 tests
**File:** `src/interpreter/builtins/iterators.rs`
**Bug:** Missing `IfAbruptCloseIterator` for inner value extraction.
**Tests:** `flatMap/close-iterator-when-inner-value-throws.js` (x2)

### 4.4 `Iterator.from` prototype check bypasses Proxy `getPrototypeOf` — 6 tests
**File:** `src/interpreter/builtins/iterators.rs`
**Bug:** Reads Rust struct field instead of calling `[[GetPrototypeOf]]`.
**Tests:** `proxy-not-wrapped.js`, `proxy-wrap-next.js`, `proxy-wrap-return.js` (x2 each)

### 4.5 Cross-realm iterator wrapper/helper state — 4 tests
**File:** `src/interpreter/builtins/iterators.rs`
**Bug:** State stored in per-realm map; needs to be on the object itself.
**Tests:** `wrap-functions-on-other-global.js`, `iterator-helpers-from-other-global.js` (x2 each)

---

## Phase 5: TypedArray Fixes (~22 tests)

### 5.1 `Object.seal`/`Object.freeze` on TypedArrays — 6 tests
**File:** `src/interpreter/builtins/mod.rs`
**Bug:** Doesn't enumerate TypedArray virtual index properties.
**Fix:** Use `[[OwnPropertyKeys]]` to get all keys including indices.
**Tests:** `seal-and-freeze.js`, `test-integrity-level.js`, `test-integrity-level-detached.js` (x2 each)

### 5.2 `preventExtensions` on resizable-buffer-backed TypedArrays — 6 tests
**File:** `src/interpreter/builtins/mod.rs`
**Bug:** Doesn't implement TypedArray's custom `[[PreventExtensions]]` that returns `false` for RABs.
**Tests:** `preventExtensions-variable-length-typed-arrays.js`, `seal-variable-length-typed-arrays.js`, `Reflect/preventExtensions-variable-length-typed-arrays.js` (x2 each)

### 5.3 TypedArray constructor buffer-path step ordering — 2 tests
**File:** `src/interpreter/builtins/typedarray.rs`
**Bug:** `get_proto` called after `ToIndex(byteOffset)` instead of before.
**Tests:** `constructor-buffer-sequence.js` (x2)

### 5.4 `TypedArray.from` reads all elements before calling constructor — 2 tests
**File:** `src/interpreter/builtins/typedarray.rs`
**Bug:** Collects all values into Vec first, then creates TypedArray.
**Tests:** `from_errors.js` (x2)

### 5.5 `typed_array_create` uses wrong prototype for cross-realm constructors — 4 tests
**File:** `src/interpreter/builtins/typedarray.rs`
**Bug:** Fast path always uses current realm's prototype.
**Tests:** `from_realms.js`, `of.js` (x2 each)

### 5.6 GC/state corruption during heavy cross-realm TypedArray iteration — 1 test
**File:** `src/interpreter/gc.rs`
**Bug:** Object constructors get corrupted after many allocations with cross-realm ops.
**Tests:** `map-and-filter.js` — may be fixed by 5.5.

---

## Phase 6: Set Operations (~6 tests)

### 6.1 Set operations don't handle concurrent modification — 6 tests
**File:** `src/interpreter/builtins/collections.rs`
**Bug:** `intersection`, `union`, `symmetricDifference` don't handle set mutation during `has()`/`keys()` callbacks.
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
| 1 | 2.1 try/finally bug | 4+ | Medium (critical correctness) |
| 2 | 1.x parser fixes | ~30 | Easy |
| 3 | 2.x interpreter fixes | ~30 | Medium |
| 4 | 3.x RegExp fixes | ~22 | Medium-Hard |
| 5 | 4.x Iterator helpers | ~16 | Medium |
| 6 | 5.x TypedArray | ~22 | Medium |
| 7 | 6.x Set operations | ~6 | Medium |
| 8 | 7.x Intl/Temporal | ~36 | Hard (locale data) |
| 9 | 8.x Miscellaneous | ~16 | Mixed |
| — | Test262 bugs (1.1) | 6 | N/A |

**Estimated total fixable: ~190 tests** (6 are test262 bugs, ~7 are OOM/timeout performance issues, ~6 are hard locale data issues).
