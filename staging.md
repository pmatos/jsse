# Staging Test Failures Plan

**Current:** 101,013 / 101,234 (99.78%) — 221 failing scenarios
**Previous baseline:** 101,035 / 101,269 (99.77%) — 234 failing scenarios

---

## A. Structural / Hard Changes

### A1. ~~Nested for-of in generators → infinite loop~~ (DONE, +2 passes)

Fixed nested for-of/while/for loops in generator state machine transform:
- Unique temp var names via separate `temp_counter` (was reusing `yield_counter`)
- Conditional post-body finalization to prevent clobbering inner loop terminators
- Save/restore break/continue targets for proper nesting
- Route break/continue inside if-statements through state machine transform

- `staging/sm/TypedArray/slice-bitwise-same.js` — **NOW PASSING**
- `staging/sm/TypedArray/sort-negative-nan.js` — still failing (unrelated: missing `anyTypedArrayConstructors` harness)

### ~~A2. Function.caller / arguments.callee.caller not implemented (12 tests, MEDIUM)~~ DONE

Implemented call stack tracking with `CallFrame` vec and accessor property getters. All 12 tests (13 scenarios) now pass.

### ~~A3. Annex B block-scoped function semantics (6 tests, HARD)~~ PARTIAL (+4 passes)

Fixed labeled function declarations, `with`-scope hoisting, eval var in catch, and `var arguments` shadowing:
- Unwrap `Statement::Labeled` in `collect_annexb_function_names` and `exec_switch` function hoisting
- Added `Statement::With` case to `collect_annexb_function_names`
- Skip `is_simple_catch_scope` in eval intermediate scope conflict check (B.3.5)
- Copy `arguments` from func_env to body_env for non-simple params

2 remaining: `arguments` override tests contradict spec-correct annexB test `block-decl-func-skip-arguments.js` (B.3.3.1 step 22.f adds "arguments" to parameterNames, blocking Annex B hoist).

- `staging/sm/lexical-environment/block-scoped-functions-annex-b-arguments.js` — **NOT FIXABLE** (contradicts spec)
- `staging/sm/lexical-environment/block-scoped-functions-annex-b-label.js` — **NOW PASSING**
- `staging/sm/lexical-environment/block-scoped-functions-annex-b-with.js` — **NOW PASSING**
- `staging/sm/lexical-environment/var-in-catch-body-annex-b-eval.js` — **NOW PASSING**
- `staging/sm/regress/regress-602621.js` — **NOT FIXABLE** (contradicts spec)
- `staging/sm/Function/arguments-parameter-shadowing.js` — **NOW PASSING**

### ~~A4. Intl402 non-ISO calendar support (13 tests, VERY HARD)~~ DONE (+26 passes)

Implemented full non-ISO calendar support for DateTimeFormat component-based formatting:
- Extended `CalendarFields` with cyclic year data (`cyclic_year`, `related_iso`)
- `apply_calendar_conversion()` converts ISO dates to any calendar for all formatting paths (not just dateStyle)
- Chinese/Dangi: emit `relatedYear`/`yearName` parts instead of `year`; `cyclic_year_name()` for sexagenary cycle
- `format_era()` replaces Gregorian-only era functions; supports Japanese, ROC, Buddhist, Hebrew, Indian, Persian, Coptic, Ethiopian
- Non-Gregorian era calendars always display `era_year` (not extended year)
- Hebrew always uses month names even with `month: "numeric"`, D Month Y layout
- Lunisolar leap months: "bis" suffix for Chinese/Dangi numeric months (e.g., "4bis" for M04L)
- `month_code_number()` extracts standard month from month codes (handles leap months correctly)

All 13 tests (26 scenarios) now pass, 0 regressions.

### ~~A5. Temporal ZonedDateTime DST handling (6 tests, HARD)~~ PARTIAL (+10 passes)

Fixed DST handling in ZonedDateTime operations:
- Added `resolve_local_to_epoch` helper for reject-aware disambiguation
- `from()` property bag: restructured to use disambiguation when no offset present; exact matching (`sub_minute: true`) for bag offsets per spec
- `with()`: wired up previously-unused `disambiguation` option; exact offset matching with `offset_match_or_reject`/`offset_match_candidates`
- `until()`/`since()` day correction: replaced `get_tz_offset_ns(tz, BigInt::from(int_local))` (treating wall-clock as UTC) with `disambiguate_instant(tz, int_local, "compatible")`
- `toString()` rounding: re-resolve through timezone after rounding to handle DST gaps
- String parsing: fixed `"ignore"`/`"prefer"` fallback and no-offset `"reject"` paths

5/6 tests (10/12 scenarios) now pass, 0 regressions. Remaining `duration-round.js` failure is in duration rounding code (separate issue).

- `staging/Intl402/Temporal/old/dst-math.js` — **NOW PASSING**
- `staging/Intl402/Temporal/old/duration-round.js` — still failing (duration rounding issue in `duration.rs`)
- `staging/Intl402/Temporal/old/property-bags.js` — **NOW PASSING**
- `staging/Intl402/Temporal/old/zdt-tostring.js` — **NOW PASSING**
- `staging/Intl402/Temporal/old/zdt-with.js` — **NOW PASSING**
- `staging/Intl402/Temporal/old/tzdb-string-parsing.js` — **NOW PASSING**

### ~~A6. Proxy [[Set]] redundant descriptor lookups (1 test, MEDIUM)~~ DONE (+2 passes)

Fixed `proxy_get_own_property_descriptor` to return a normalized descriptor (via `from_property_descriptor`) instead of the raw trap result object. This ensures `to_property_descriptor` runs exactly once inside GOPD for invariant checking, and callers that re-read the descriptor operate on a plain object with no proxy traps. Also propagates `to_property_descriptor` errors per spec step 15.

- `staging/sm/Iterator/prototype/map/proxy-accesses.js` — **NOW PASSING**

---

## B. Medium Impact — Good ROI

### ~~B1. Class field ASI parsing bug (6 tests, MEDIUM)~~ DONE (+12 passes)

Removed incorrect `in_class_field_initializer` flag that broke `[` continuation after newlines. The real fix: ArrowFunction is not a LeftHandSideExpression per spec §13.3, so `parse_left_hand_side_expression()` now skips the member access loop for bare (non-parenthesized) arrow functions. This correctly handles both cases: `x = obj\n['lol']` continues as member access (obj is a PrimaryExpression), while `()=>{}\n[expr]` triggers ASI (arrow function cannot be extended).

- `language/expressions/class/elements/fields-asi-1.js` — **NOW PASSING**
- `language/expressions/class/elements/fields-asi-2.js` — **NOW PASSING**
- `language/expressions/class/elements/fields-asi-3.js` — **NOW PASSING**
- `language/statements/class/elements/fields-asi-1.js` — **NOW PASSING**
- `language/statements/class/elements/fields-asi-2.js` — **NOW PASSING**
- `language/statements/class/elements/fields-asi-3.js` — **NOW PASSING**

### B2. Strict mode SM tests — harness incompatibility (13 tests, FIXED)

FIXED: Test runner now detects `sm/non262-strict-shell.js` in includes and skips the `:strict` scenario, since the harness manages strict/lenient mode internally via `testLenientAndStrict()`.

### B3. RegExp: template literal lone surrogate escapes (3 tests, EASY)

Parser rejects `\uDC38` etc. in template literals. Only `\u{XXXX}` surrogates should be rejected.

- `staging/sm/RegExp/split-trace.js`
- `staging/sm/RegExp/unicode-raw.js`
- `staging/sm/RegExp/unicode-class-raw.js`

### B4. RegExp: `\u{NN}` without /u flag (1 test, EASY)

`/\u{41}/` (no `/u`) should be `\u` + `{41}` quantifier, not Unicode escape.

- `staging/sm/RegExp/unicode-braced.js`

### B5. RegExp: AdvanceStringIndex lone surrogate off-by-one (2 tests, EASY)

Advance by 1 (not 2) when lead surrogate is at string end with no trail.

- `staging/sm/RegExp/match-trace.js`
- `staging/sm/RegExp/replace-trace.js`

### B6. RegExp: Rust panic on multi-byte replace (1 test, EASY — critical bug)

Byte vs char index at `regexp.rs:6154`. Crash bug affecting real usage.

- `staging/sm/RegExp/replace-twoBytes.js`

### B7. RegExp: `\-` in char class with `/u` (1 test, EASY)

`[A\-Z]` with `/u` should be `{A, -, Z}`, not range `[A-Z]`.

- `staging/sm/RegExp/unicode-disallow-extended.js`

### B8. RegExp: `\W`/`[^\W]` negation broken with `/iu` (1 test, MEDIUM)

Negation logic for `\W` and `[^\w]` with unicode+ignoreCase doesn't account for expanded word character set.

- `staging/sm/RegExp/unicode-ignoreCase-escape.js`

### B9. RegExp: `\b` word boundary too broad with `/i` alone (1 test, MEDIUM)

Without `/u`, U+017F should not be a word character even with `/i`.

- `staging/sm/RegExp/unicode-ignoreCase-word-boundary.js`

### B10. RegExp: Symbol.match infinite loop with RegExp subclass (1 test, MEDIUM)

DuckRegExp with overridden `exec` causes infinite loop in `Symbol.match`.

- `staging/sm/RegExp/lastIndex-match-or-replace.js`

### B11. RegExp: `compile()` side-effect not honored during exec (2 tests, MEDIUM)

When `compile()` is called as side-effect of `ToLength(lastIndex)`, recompilation not applied.

- `staging/sm/RegExp/match-local-tolength-recompilation.js`
- `staging/sm/RegExp/replace-local-tolength-recompilation.js`

### B12. RegExp: constructor reads source/flags after new.target.prototype getter (1 test, MEDIUM)

Should read `[[OriginalSource]]`/`[[OriginalFlags]]` before `OrdinaryCreateFromConstructor`.

- `staging/sm/RegExp/constructor-ordering.js`

### B13. RegExp: capture groups not reset in quantified group (1 test, MEDIUM)

`/(?:^(a)|\1(a)|(ab)){2}/` — group 1 should reset between iterations.

- `staging/sm/RegExp/regress-613820-3.js`

### B14. RegExp: Unicode char class range + property escapes ignore lone surrogates (3 tests, MEDIUM)

Lone surrogates not treated as matchable code points in unicode mode.

- `staging/sm/RegExp/unicode-class-braced.js`
- `built-ins/RegExp/property-escapes/generated/General_Category_-_Private_Use.js`
- `built-ins/RegExp/property-escapes/generated/General_Category_-_Surrogate.js`

### B15. Iterator.from: double Symbol.iterator + missing getPrototypeOf (3 tests, MEDIUM)

Reads `Symbol.iterator` twice, doesn't check prototype chain per spec.

- `staging/sm/Iterator/from/proxy-not-wrapped.js`
- `staging/sm/Iterator/from/proxy-wrap-next.js`
- `staging/sm/Iterator/from/proxy-wrap-return.js`

### B16. Iterator.from: wrapper edge cases (3 tests, LOW)

return() re-read after first call, object check on next result, cross-realm brand.

- `staging/sm/Iterator/from/modify-return.js`
- `staging/sm/Iterator/from/wrap-next-not-object-throws.js`
- `staging/sm/Iterator/from/wrap-functions-on-other-global.js`

### B17. Iterator flatMap: missing IfAbruptCloseIterator (1 test, LOW)

Outer iterator not closed when inner value getter throws.

- `staging/sm/Iterator/prototype/flatMap/close-iterator-when-inner-value-throws.js`

### B18. Iterator helpers cross-realm prototype (1 test, MEDIUM)

Methods stored on instances instead of shared `%IteratorHelperPrototype%`.

- `staging/sm/Iterator/prototype/iterator-helpers-from-other-global.js`

### B19. TypedArray seal/freeze semantics (3 tests, LOW-MEDIUM)

Non-empty: should throw TypeError. Detached: should succeed.

- `staging/sm/TypedArray/seal-and-freeze.js`
- `staging/sm/TypedArray/test-integrity-level.js`
- `staging/sm/TypedArray/test-integrity-level-detached.js`

### B20. TypedArray constructor/from evaluation order (2 tests, MEDIUM)

AllocateTypedArray before ToIndex(byteOffset); construct before reading elements.

- `staging/sm/TypedArray/constructor-buffer-sequence.js`
- `staging/sm/TypedArray/from_errors.js`

### B21. Cross-realm TypedArray from/of (2 tests, MEDIUM)

Prototype chain not set to other realm's TypedArray prototype.

- `staging/sm/TypedArray/from_realms.js`
- `staging/sm/TypedArray/of.js`

### B22. Temporal non-ISO calendar IDs rejected (7 tests, MEDIUM)

PlainMonthDay/PlainYearMonth reject `islamic-civil`, `hebrew`, etc. Should accept and canonicalize.

- `intl402/Temporal/PlainMonthDay/from/canonicalize-calendar.js`
- `intl402/Temporal/PlainMonthDay/from/reference-date-noniso-calendar.js`
- `intl402/Temporal/PlainMonthDay/prototype/equals/canonicalize-calendar.js`
- `intl402/Temporal/PlainYearMonth/from/canonicalize-calendar.js`
- `intl402/Temporal/PlainYearMonth/prototype/equals/canonicalize-calendar.js`
- `intl402/Temporal/PlainYearMonth/prototype/since/canonicalize-calendar.js`
- `intl402/Temporal/PlainYearMonth/prototype/until/canonicalize-calendar.js`

### B23. Temporal offset string parsing (4 tests, EASY)

Rejects `-040000`, `+010000.0` — should accept no-colon offsets with optional fractional seconds.

- `staging/Temporal/Regex/old/instant.js`
- `staging/Temporal/Regex/old/plaindatetime.js`
- `staging/Temporal/Regex/old/plainmonthday.js`
- `staging/Temporal/Regex/old/plaintime.js`

### B24. Intl realm/toStringTag wiring (5 tests, EASY)

DisplayNames/Segmenter cross-realm: `[object Object]` instead of proper tag. Also Collator sort order and PluralRules categories.

- `intl402/Collator/usage-de.js`
- `intl402/DisplayNames/proto-from-ctor-realm.js`
- `intl402/PluralRules/prototype/resolvedOptions/plural-categories-order.js`
- `intl402/Segmenter/constructor/constructor/proto-from-ctor-realm.js`
- `intl402/Segmenter/proto-from-ctor-realm.js`

### B25. Temporal toLocaleString formatting (5 tests, MEDIUM)

Leading zeros on hours, wrong locale date formats.

- `staging/Intl402/Temporal/old/date-time-format.js`
- `staging/Intl402/Temporal/old/date-toLocaleString.js`
- `staging/Intl402/Temporal/old/datetime-toLocaleString.js`
- `staging/Intl402/Temporal/old/instant-toLocaleString.js`
- `staging/Intl402/Temporal/old/monthday-toLocaleString.js`

### B26. Set intersection/union/symmetricDifference (3 tests, MEDIUM)

Wrong deduplication/size with SetLike objects.

- `staging/sm/Set/intersection.js`
- `staging/sm/Set/symmetric-difference.js`
- `staging/sm/Set/union.js`

### B27. Generator edge cases (4 tests, MEDIUM)

yield* send value forwarding, return in finally, syntax edge cases, iterator close.

- `staging/sm/generators/iteration.js`
- `staging/sm/generators/return-finally.js`
- `staging/sm/generators/syntax.js`
- `staging/sm/generators/yield-iterator-close.js`

### B28. Class inner binding / super / eval (3 tests, MEDIUM)

Class name reassignment should throw TypeError. Super in eval in nested non-method function should be SyntaxError. Super property proxy receiver incorrect.

- `staging/sm/class/innerBinding.js`
- `staging/sm/class/superPropEvalInsideNested.js`
- `staging/sm/class/superPropProxies.js`

### B29. Async function/await parsing edge cases (4 tests, MEDIUM)

Unicode escape in `async` keyword, `await` in arrow/async function parameters.

- `staging/sm/async-functions/async-contains-unicode-escape.js`
- `staging/sm/async-functions/async-property-name-error.js`
- `staging/sm/async-functions/await-in-arrow-parameters.js`
- `staging/sm/async-functions/await-in-parameters-of-async-func.js`

### B30. using/await using in switch cases (3 tests, MEDIUM)

Parser rejects `using`/`await using` in switch case blocks where they should be valid.

- `staging/explicit-resource-management/await-using-in-switch-case-block.js`
- `staging/explicit-resource-management/call-dispose-methods.js`
- `staging/explicit-resource-management/await-using-in-async-function-call-without-await.js`

---

## C. Low Impact / One-offs

### C1. Temporal removed methods still present (1 test, TRIVIAL)

Delete deprecated methods (`getISOFields`, `getCalendar`, `getTimeZone`, `fromEpochSeconds`, etc.).

- `staging/Temporal/removed-methods.js`

### C2. Decorators / auto-accessor (1 test, HARD)

Full decorator runtime semantics needed for `accessor` keyword.

- `staging/decorators/public-auto-accessor.js`

### ~~C3. Float16Array (1 test, HIGH)~~ DONE (+53 passes)

Implemented Float16Array as the 12th TypedArrayKind. Reused existing `dv_f16_to_f64`/`dv_f64_to_f16_bits` IEEE 754 binary16 helpers from DataView. Added Float16 variant to TypedArrayKind enum, Realm prototype fields, all constructor/prototype lookup match blocks, Atomics rejection, and globalThis property list. Total scenario count increased from 101,269 to 101,328 (new Float16Array test scenarios discovered).

- `staging/sm/TypedArray/toString.js` — **NOW PASSING**

### C4. Source phase imports (1 test, HARD)

`import.source` not fully implemented.

- `staging/source-phase-imports/import-source-source-text-module.js`

### C5. RegExp duplicate named groups (1 test, MEDIUM)

Capture slot layout wrong across alternation branches with backreferences.

- `staging/built-ins/RegExp/named-groups/duplicate-named-groups.js`

### C6. Variable-length TypedArray operations (3 tests, MEDIUM)

preventExtensions/seal on resizable buffer TypedArrays should throw/return false when out-of-bounds.

- `staging/built-ins/Object/preventExtensions/preventExtensions-variable-length-typed-arrays.js`
- `staging/built-ins/Object/seal/seal-variable-length-typed-arrays.js`
- `staging/built-ins/Reflect/preventExtensions/preventExtensions-variable-length-typed-arrays.js`

### C7. Strict/syntax validation gaps (4 tests, MEDIUM)

Future reserved words, parenthesized destructuring patterns, optional chain eval, Function() parameter boundary exploits.

- `staging/sm/misc/future-reserved-words.js`
- `staging/sm/expressions/destructuring-pattern-parenthesized.js`
- `staging/sm/expressions/optional-chain.js`
- `staging/sm/Function/invalid-parameter-list.js`

### C8. Prototype writability / Reflect.set (2 tests, MEDIUM)

Property descriptor transitions and Reflect.set with distinct receiver.

- `staging/sm/object/proto-property-change-writability-set.js`
- `staging/sm/Reflect/set.js`

### C9. Proxy invariant checking (2 tests, MEDIUM)

getOwnPropertyDescriptor enumerable invariant; global-as-proxy property resolution.

- `staging/sm/regress/regress-1383630.js`
- `staging/sm/Proxy/global-receiver.js`

### C10. Miscellaneous one-offs (~17 tests, VARIED)

- **Stack overflow**: `staging/sm/extensions/recursion.js` — Rust stack overflow instead of JS RangeError
- **Timeouts**: `staging/sm/regress/regress-1507322-deep-weakmap.js`, `staging/sm/String/replace-math.js`
- **BigInt OOM**: `staging/sm/BigInt/large-bit-length.js` — should throw RangeError not abort
- **Destructuring**: `staging/sm/regress/regress-469625-02.js` — holes don't inherit from `Array.prototype`
- **eval var**: `staging/sm/regress/regress-694306.js` — `eval("var {if}")` leaks binding
- **eval var getter**: `staging/sm/global/bug-320887.js` — `eval("var x")` calls global getter
- **Destructuring syntax**: `staging/sm/destructuring/bug1396261.js` — `{a = 0}.x` should be SyntaxError
- **Reflect.apply**: `staging/sm/Reflect/apply.js` — wrong error type for non-callable
- **String internal**: `staging/sm/String/internalUsage.js` — Symbol.split override affects Intl
- **String case**: `staging/sm/String/string-upper-lower-mapping.js` — U+A7CF case mapping wrong
- **Error.stack**: `staging/sm/Math/acosh-approx.js`, `staging/sm/Math/atanh-approx.js` — `Error().stack` is undefined
- **JSON.rawJSON**: `staging/sm/JSON/parse-with-source.js` — wrong enumerable keys
- **Atomics**: `staging/sm/Atomics/detached-buffers.js` — no TypeError on detach during valueOf
- **Date parsing**: `staging/sm/Date/non-iso.js` — space-separated date-time not parsed
- **Array.from**: `staging/sm/Array/from-iterator-close.js` — iterator not closed on error
- **Array species**: `staging/sm/Array/species.js` — extra proxy log entry for filter
- **Array toLocaleString**: `staging/sm/Array/toLocaleString.js` — wrong `this` type in strict
- **Array unscopables**: `staging/sm/Array/unscopables.js` — `groupBy` in unscopables
- **Function.apply**: `staging/sm/Function/15.3.4.3-01.js` — valueOf not called during ToUint32
- **Function name**: `staging/sm/Function/function-name-for.js` — no name for for-in anonymous fn
- **Function.toString**: `staging/sm/Function/function-toString-builtin-name.js` — Symbol.split not shown
- `staging/sm/fields/await-identifier-module-3.js`

### C11. Tests that pass individually — flaky under load (~8 tests, N/A)

These pass when run in isolation but fail under the full suite (resource contention):

- `staging/sm/expressions/destructuring-array-default-call.js` (strict only)
- `staging/sm/expressions/destructuring-array-default-class.js` (strict only)
- `staging/sm/expressions/destructuring-array-default-function-nested.js` (strict only)
- `staging/sm/expressions/destructuring-array-default-function.js` (strict only)
- `staging/sm/expressions/destructuring-array-default-simple.js` (strict only)
- `staging/sm/expressions/destructuring-array-default-yield.js` (strict only)
- `staging/sm/JSON/parse-mega-huge-array.js`
- `staging/sm/regress/regress-610026.js`
