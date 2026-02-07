# JSSE — Master Implementation Plan

A from-scratch JavaScript engine in Rust, fully spec-compliant with ECMA-262.

**Total test262 tests:** ~48,257 (excluding Temporal/intl402)
**Current pass rate:** 34,934 / 48,257 run (72.39%)

---

## Phased Implementation Roadmap

The engine is broken into 10 phases, ordered by dependency. Each phase has a detailed sub-plan in `plan/`.

| Phase | Name | Spec Sections | Status | Detail |
|-------|------|---------------|--------|--------|
| 1 | [Project Scaffolding & Infrastructure](plan/phase-01-infrastructure.md) | — | ✅ Complete | Rust project, CLI, test harness, CI |
| 2 | [Types & Values](plan/phase-02-types.md) | §6 | ✅ ~95% | Language types, spec types, type conversions |
| 3 | [Lexer](plan/phase-03-lexer.md) | §12 | ✅ Complete | Lexical grammar, tokens, Unicode |
| 4 | [Parser (AST)](plan/phase-04-parser.md) | §13–16 | ✅ Complete | Expressions, statements, functions, modules |
| 5 | [Runtime Core](plan/phase-05-runtime.md) | §6–10 | 🟡 ~30% | Environments, execution contexts, objects |
| 6 | [Evaluation — Expressions & Statements](plan/phase-06-evaluation.md) | §13–14 | 🟡 ~60% | Most operators/statements work |
| 7 | [Functions & Classes](plan/phase-07-functions-classes.md) | §15 | 🟡 ~70% | Functions, classes, generators, async/await work |
| 8 | [Modules & Scripts](plan/phase-08-modules.md) | §16 | ✅ ~90% | import/export, dynamic import(), import.meta, TLA, cyclic deps |
| 9 | [Built-in Objects](plan/phase-09-builtins.md) | §19–28 | 🟡 ~40% | Object, Array, String, Math, JSON (105/165), URI encode/decode work |
| 10 | [Advanced Features](plan/phase-10-advanced.md) | §17,25–27,B | 🟡 ~20% | Error handling, memory model, Proxy, Reflect, Annex B |

---

## Current Built-in Status

| Built-in | Pass Rate | Tests |
|----------|-----------|-------|
| Object | 93% | 3,176/3,411 |
| Array | 81% | 2,496/3,079 |
| String | 92% | 1,120/1,215 |
| Function | 78% | 397/509 |
| Iterator | 85% | 436/510 |
| Promise | 86% | 548/639 |
| Map | 77% | 158/204 |
| Set | 95% | 365/383 |
| Date | 76% | 451/594 |
| Reflect | 81% | 124/153 |
| Proxy | 58% | 181/311 |
| Symbol | 71% | 67/94 |
| RegExp | 65% | 1,214/1,879 |
| Math | 92% | 300/327 |
| WeakRef | 76% | 22/29 |
| FinalizationRegistry | 72% | 34/47 |

---

## Current Blockers (Highest Impact)

These features block significant numbers of tests:

1. ~~**`arguments` object**~~ — ✅ Done (82/203, 40.39%). Mapped arguments + Symbol.iterator implemented.
2. ~~**Garbage collection**~~ — ✅ Done. Mark-and-sweep GC with free-list reuse (148 MB → 11 MB on 100k object alloc).
3. ~~**Generator `yield` evaluation**~~ — ✅ Done (965 new passes, 33.79% overall). Replay-based yield with next/return/throw. Remaining: yield* delegation, throw resumption, GeneratorFunction constructor.
4. **Iterator protocol** — Breaks `for...of`, spread on non-arrays, many built-in methods.
4. ~~**Promise**~~ — ✅ Done (190/639, 30%). Constructor, then/catch/finally, resolve/reject/all/allSettled/race/any. Async/await supported.
5. ~~**Map/Set**~~ — ✅ Done (Map: 103/204, Set: 261/383). Remaining failures: native fn `.length` properties, Proxy/Reflect/Symbol.species deps.
6. ~~**Date**~~ — ✅ Done (305/594, 51%). Constructor, static methods (now/parse/UTC), getters, setters, string formatting, Symbol.toPrimitive. Remaining failures: native fn `.length`/`.name`/prop-desc, Proxy/Reflect.construct, edge-case string parsing.

---

## Recommended Next Tasks (Priority Order)

1. ~~**Complete `arguments` object (mapped arguments)**~~ — ✅ Done
2. ~~**Garbage collection**~~ — ✅ Done
3. ~~**Complete Iterator built-in**~~ — ✅ Done (138/510, 27%). Constructor, helpers (toArray/forEach/reduce/some/every/find/map/filter/take/drop/flatMap), Iterator.from, Iterator.concat. Remaining failures need generators.
3. ~~**Implement Map and Set**~~ — ✅ Done (364 new passes)
4. ~~**Implement Date**~~ — ✅ Done (305/594, 51%, 406 new passes overall)
5. ~~**Generator `yield` evaluation**~~ — ✅ Done (965 new passes)
6. ~~**Proxy and Reflect**~~ — ✅ Done (Reflect: 54/153, Proxy: 120/311, 140 net new passes). All 13 traps wired, Proxy.revocable implemented. Remaining: invariant enforcement, Symbol property keys.
7. ~~**Native function `.length` and Constructor `.prototype` exposure**~~ — ✅ Done (375 new passes, 50 regressions, net +325). All 210+ native functions now report correct arity via `.length`. Array.prototype and String.prototype accessible via constructors.
8. ~~**Private class elements runtime**~~ — ✅ Done (87 new passes, 1 regression). Private methods, private getters/setters, static private methods/accessors, `#x in obj` brand checks.
9. ~~**WeakMap and WeakSet**~~ — ✅ Done (WeakMap: 72/141, WeakSet: 50/85, 129 new passes overall). Constructor with iterable, get/set/has/delete methods. Weak GC semantics implemented (ephemeron fixpoint, post-sweep cleanup).
10. ~~**Symbol built-in**~~ — ✅ Done (26/94, 28%, 43 new passes overall). Symbol.prototype (toString, valueOf, description, @@toPrimitive, @@toStringTag), Symbol.for/keyFor registry, new Symbol() TypeError, symbol equality, primitive property access.
11. ~~**RegExp well-known Symbol methods + lastIndex + String dispatch**~~ — ✅ Done (165 new passes, 38.02% → 38.42%). RegExp.prototype exec/test with lastIndex/global/sticky/captures. @@match, @@search, @@replace, @@split, @@matchAll on RegExp.prototype. String.prototype match/replace/replaceAll/search/split/matchAll dispatch through Symbol methods. RegExpStringIterator for matchAll. Follow-up: flags getter, ToString coercion fix, hasIndices flag (+37 passes, 54.68% → 54.76%). RegExp: 941/1,879 (50.08%).
12. ~~**Class public instance fields, method descriptors, static blocks**~~ — ✅ Done (236 new passes, 39.21% → 39.78%). Public instance fields stored on constructor and initialized at construction time. Class method descriptors set to enumerable:false per spec. Static blocks executed with `this` bound to constructor.
13. ~~**Array built-in spec compliance**~~ — ✅ Done (926 new passes, 42.63% → 44.58%). All Array.prototype methods rewritten with ToObject(this), LengthOfArrayLike, IsCallable validation, thisArg support, and property-based access for array-like objects. Array: 736/2,989 → 2,050/3,079 (67%).
14. ~~**`for await...of` and async iteration**~~ — ✅ Done (1,014 new passes, 44.82% → 46.89%). Parse `for await (... of ...)`, Symbol.asyncIterator, async-from-sync iterator wrapper, await in for-of loop body. for-await-of: 567/1,234 (46%). Test harness fix: auto-include doneprintHandle.js for async tests.
15. ~~**Async generators (`async function*`)**~~ — ✅ Done (2,332 new passes, 46.89% → 51.80%). AsyncGenerator iterator state, %AsyncIteratorPrototype% with [Symbol.asyncIterator], %AsyncGeneratorPrototype% with next/return/throw returning promises, await yielded values, AsyncGeneratorFunction.prototype chain, yield* with async iterators, rejected promises for type errors, nested yield expression fix. async-generator statements: 188/301 (62%), expressions: 399/623 (64%), for-await-of: 1,064/1,234 (86%), AsyncGeneratorPrototype: 29/48 (60%).
16. ~~**`with` statement**~~ — ✅ Done (19 new passes, 51.80% → 51.84%). Object environment records with `with_object` field on Environment. Property lookup/assignment through with-scope object. `@@unscopables` support (eagerly resolved). Known limitations: Proxy `has` trap not invoked in with-scope, lazy @@unscopables getter evaluation not supported. with statements: 41/181 (23%).
17. ~~**not-a-constructor enforcement**~~ — ✅ Done (290 new passes, 51.84% → 52.45%). Added `is_constructor` flag to `JsFunction::Native`, only constructors get `.prototype` property. `eval_new()` throws TypeError for non-constructors. Built-in constructors (Object, Array, Error variants, String, Number, Boolean, Function, RegExp, Date, Map, Set, WeakMap, WeakSet, Promise, Proxy, ArrayBuffer, DataView, TypedArrays) marked as constructors; all other native functions default to non-constructor.
18. ~~**String.prototype wiring fix**~~ — ✅ Done (445 new passes, 5 regressions, 52.45% → 53.38%). Reordered String constructor registration before `setup_string_prototype()` so prototype methods are wired correctly. String: 462/1,215 (38%) → 863/1,215 (71%).
19. ~~**Iterator protocol completion**~~ — ✅ Done (70 new passes, 53.38% → 53.53%). Symbol.dispose on %IteratorPrototype%, Iterator.zip and Iterator.zipKeyed with shortest/longest/strict modes, argument validation closes iterator on failure, take/drop edge case fixes, generator return/throw borrow fix. Iterator: 138/510 (27%) → 290/510 (57%).

20. ~~**Error & NativeError built-in fixes**~~ — ✅ Done (58 new passes, 54.83% → 54.95%). Per-type NativeError prototypes inheriting from Error.prototype with `name`, `message`, `constructor`. Error.prototype properties set non-enumerable. Constructor arities fixed to 1. `cause` option support. Message ToString coercion. Error.isError() static method. Error: 32/53 (60%), NativeErrors: 62/92 (67%).
21. ~~**`instanceof` and `Function.prototype[@@hasInstance]`**~~ — ✅ Done (7 new passes, 54.95%). Spec-compliant `instanceof` (§13.10.2): checks `Symbol.hasInstance` before prototype chain walk, throws TypeError for non-objects. `OrdinaryHasInstance` extracted as reusable helper. `Function.prototype[@@hasInstance]` added (non-writable, non-enumerable, non-configurable). instanceof: 25/43 → 28/43 (65%), Function.prototype[Symbol.hasInstance]: 1/11 → 5/11 (45%).
22. ~~**Function name inference (SetFunctionName)**~~ — ✅ Done (628 new passes, 54.95% → 56.25%). Anonymous functions get `.name` set from binding context: variable declarations, assignments, object literal properties, destructuring defaults, get/set accessors (prefixed with "get "/"set ").
23. ~~**Generator method syntax in object literals**~~ — ✅ Done (160 new passes, 56.25% → 56.54%). Parse `{ *method() { yield ... } }` in object literals. Unblocks gen-meth-* destructuring tests and generator method-definition tests.
25. ~~**Object destructuring RequireObjectCoercible + ToObject + getter invocation**~~ — ✅ Done (141 new passes, 60.83% → 61.06%). Object destructuring now calls `to_object()` (throws TypeError for null/undefined, wraps primitives). Property access during destructuring uses `get_object_property()` to invoke getters and Proxy traps.
26. ~~**Update expressions for member expressions + ToNumeric**~~ — ✅ Done (50 new passes). `obj.x++`, `obj[i]++`, `--obj.prop` now work. Update expressions use `to_primitive(number)` for valueOf coercion on objects.
27. ~~**Math[@@toStringTag] + prop-desc fixes**~~ — ✅ Done (25 new passes). Math methods now non-enumerable. Math: 275/327 (84%) → 300/327 (92%).
28. ~~**WeakRef + FinalizationRegistry**~~ — ✅ Done (56 new passes). WeakRef constructor + deref(). FinalizationRegistry constructor + register/unregister. WeakRef: 22/29 (76%), FinalizationRegistry: 34/47 (72%).
24. ~~**AggregateError + Promise.try/withResolvers + Proxy invariants**~~ — ✅ Done (75 new passes, 58.30% → 58.46%). AggregateError constructor with proper prototype chain. Promise.try and Promise.withResolvers static methods. Proxy invariant enforcement for get/set/has/deleteProperty/defineProperty/getOwnPropertyDescriptor/ownKeys/getPrototypeOf/setPrototypeOf/isExtensible/preventExtensions traps. Proxy trap delegation added to Reflect methods. AggregateError: 14/25 (56%), Proxy: 163/310 (53%), Reflect: 124/153 (81%).
29. ~~**Symbol property key uniqueness**~~ — ✅ Done (51 new passes, 62.22% → 62.28%). Symbols with same description were treated as identical property keys. Added id-based property key format for user-created symbols. Updated all built-in functions (Object.hasOwn, defineProperty, getOwnPropertyDescriptor, Reflect methods, etc.) to use `to_property_key_string` for symbol-aware key conversion.
30. ~~**Function parameter error propagation**~~ — ✅ Done (371 new passes, 61.44% → 62.22%). Destructuring errors in function parameter binding were silently swallowed by `let _ =`. Now propagated as throws for sync functions and promise rejections for async functions.
31. ~~**@@toPrimitive support + unary operator ToPrimitive**~~ — ✅ Done (45 new passes, 62.28% → 62.38%). `to_primitive` now checks `Symbol.toPrimitive` before falling back to valueOf/toString per §7.1.1. Unary +/- operators now call `to_number_coerce` for objects instead of raw `to_number`. 1 Date regression (year-zero parsing).
32. ~~**Prototype constructor properties**~~ — ✅ Done (119 new passes, 62.38% → 62.62%). Added `constructor` property to Array.prototype, Number.prototype, Boolean.prototype, RegExp.prototype pointing to their respective constructors. 1 Array.from regression (thisArg constructor).
33. ~~**Generator state machine refactor**~~ — ✅ Done (694 new passes, 62.70% → 64.16%). Replaced replay-from-start generator execution with persistent environment. Parameters bound once at creation, local variables persist between yields. Added `GeneratorExecutionState` enum (SuspendedStart, SuspendedYield, Executing, Completed). Generator statements: 225/266 (85%), expressions: 233/290 (80%), GeneratorPrototype: 38/61 (62%).
34. ~~**ES Modules**~~ — ✅ Done (684 new passes, 64.16% → 64.52%). Full ES module support: import/export declarations, dynamic import(), import.meta, top-level await, module namespace objects with live bindings, circular dependency detection, duplicate export detection, re-export live binding resolution. module-code: 518/737 (70%), import: 120/162 (74%), dynamic-import: ~60%.
35. ~~**TypedArray.prototype.with()**~~ — ✅ Done (13 new passes, 64.52% → 64.54%). ES2023 immutable update method that creates a copy with a single element replaced. Proper coercion order (index then value), BigInt TypedArray support, valueOf error propagation. TypedArray: 786/1,438 (55%).
36. ~~**Array.prototype.toLocaleString + Object.prototype.toLocaleString + TypedArray.prototype.toLocaleString**~~ — ✅ Done (48 new passes, 64.54% → 64.64%). Implemented per ECMA-262 §23.1.3.32: ToObject(this), LengthOfArrayLike, comma separator, skip undefined/null elements, Invoke toLocaleString on others with no arguments. Added Object.prototype.toLocaleString as base (calls this.toString). Added TypedArray.prototype.toLocaleString with ValidateTypedArray checks. Array/toLocaleString: 8/12 (67%), Object/toLocaleString: 11/12 (92%), TypedArray/toLocaleString: passing.

42. ~~**Conformance batch 5: Property descriptors, strict mode, String exotic**~~ — ✅ Done (133 new passes, 67.89% → 68.16%). String exotic objects (§10.4.3): wrapper .length and indexed character access with correct descriptors. Built-in method enumerability: bulk insert_value→insert_builtin for all prototype/static methods. Global NaN/Infinity/undefined made non-writable/non-configurable. `in` operator: symbol key support and TypeError for non-object RHS. Strict mode inheritance for nested functions/arrows. Sloppy this wrapping (ToObject for primitive this). from_property_descriptor always includes get/set for accessors. define_own_property checks array_elements for existing properties. Object: 3,076→3,121 (92%), Array: 2,306→2,342 (76%), String: 1,015→1,024 (84%).

37. ~~**Symbol.species accessor**~~ — ✅ Done (27 new passes, 64.64% → 64.70%). Added `[Symbol.species]` getter to Array, ArrayBuffer, Map, Set, Promise, RegExp constructors. Simple getter returning `this`. All 29 direct tests now pass. See `plan/symbol-species.md`.

38. ~~**ArrayBuffer.prototype getters**~~ — ✅ Done (3 new passes, 64.70% → 64.71%). Added `detached`, `resizable`, `maxByteLength` accessor properties to ArrayBuffer.prototype. For non-resizable buffers: detached=false, resizable=false, maxByteLength=byteLength. Most direct tests require `arraybuffer-transfer` or `resizable-arraybuffer` features not yet implemented. See `plan/arraybuffer-getters.md`.

39. ~~**GeneratorFunction constructor**~~ — ✅ Done (27 new passes, 64.71% → 64.78%). Implemented GeneratorFunction constructor (§27.3) with proper prototype chain: GeneratorFunction.prototype inherits from Function.prototype, links to Generator.prototype. Fixed `create_function()` to assign `generator_function_prototype` to generator functions. Fixed `eval_new()` to reject generators and async functions as constructors. Fixed Generator.prototype.return/throw to validate generator state. GeneratorFunction: 7/23 → 18/23 (78%), GeneratorPrototype: 41/61 → 49/61 (80%).

40. ~~**AsyncFunction and AsyncGeneratorFunction constructors**~~ — ✅ Done (21 new passes, 64.78% → 64.82%). Implemented AsyncFunction (§27.7) and AsyncGeneratorFunction (§27.4) constructors. Added `async_function_prototype` field to Interpreter with Symbol.toStringTag. AsyncFunction.prototype inherits from Function.prototype. Fixed `create_function()` to detect async non-generator functions and assign correct prototype/class_name. Constructors are intrinsics (not exposed as globals) - accessed via `Object.getPrototypeOf(async function(){}).constructor`. AsyncFunction: 10/18 → 15/18 (83%), AsyncGeneratorFunction: 9/23 → 19/23 (83%).

41. ~~**Rewrite assignment destructuring**~~ — ✅ Done (138 new passes, 64.82% → 65.11%). Rewrote array and object assignment destructuring to use iterator protocol (`get_iterator`/`iterator_step`/`iterator_value`) instead of direct `array_elements` access. Added `put_value_to_target` for recursive dispatch to any assignment target (identifiers, member expressions, nested patterns). Added `set_member_property` helper for member expression targets (dot, computed, private, Proxy set traps, setters). Added `iterator_close_result` for proper IteratorClose error propagation. Object destructuring now uses `get_object_property` for getter/Proxy trap invocation, supports rest (`{...r} = obj`). assignment/dstr: 120/368 (33%) → 252/368 (69%).

43. ~~**Conformance batch 9: Map methods, RegExp @@replace, JSON parse/stringify, ToPrimitive**~~ — ✅ Done (+254 new passes, 69.48% → 70.01%). Map.prototype.getOrInsert/getOrInsertComputed (100% pass). RegExp[@@replace] rewritten to spec §22.2.5.8: RegExpExec, result coercion, GetSubstitution, AdvanceStringIndex (34% → 77%). JSON.parse reviver with Proxy support, ES2025 source text context, lone surrogate escaping, proxy-aware stringify (70% → 96%). ToPrimitive OrdinaryToPrimitive bug fix: error propagation and getter invocation via get_object_property (+77 bonus String passes). ToObject-before-ToPropertyKey ordering fix for computed member access. Map: 156→158/204 (77%), RegExp: 1,154→1,214/1,879 (65%), JSON: 115→159/165 (96%), String: 1,043→1,120/1,215 (92%).

44. ~~**Conformance batch 10: var scoping, Promise combinators, Proxy trap forwarding**~~ — ✅ Done (+137 new passes, 70.01% → 70.29%). Fixed `var` binding in block statements to declare in var scope (function/global) instead of current block scope — affects `bind_pattern` and `exec_variable_declaration` in exec.rs (+51 passes). Promise combinators (all/allSettled/race/any) now use spec-compliant `Invoke(nextPromise, "then", ...)` instead of internal `promise_then` (+47 passes). Proxy trap forwarding for proxy-of-proxy chains: all 12 non-get traps now recurse through proxy-aware helpers instead of raw `JsObjectData` methods (+7 passes). Also fixed `Object.preventExtensions` to throw TypeError when trap returns false, and class static blocks to use function scope environments. Function: 375→397/509 (78%), Promise: 501→548/639 (86%), Proxy: 173→181/311 (58%).

45. ~~**Conformance batch 11: iterator protocol, DataView/ArrayBuffer prototype, function name inference**~~ — ✅ Done (+783 new passes, 70.29% → 71.91%). Three orthogonal fixes: (1) Iterator protocol in `bind_pattern`: IteratorClose after array destructuring, `iterator_value()` now uses `get_object_property()` for getter invocation, elision error propagation, object rest pattern getter invocation (+~400 passes). (2) DataView/ArrayBuffer prototype chain: constructor `.prototype` now points to the real prototype object with methods installed, matching Map/Set pattern (+~160 passes, DataView: 52%, ArrayBuffer: 45%, TypedArray: 66%). (3) IsAnonymousFunctionDefinition check: added `is_anonymous_function_definition()` to guard `set_function_name()` calls — comma expressions, parenthesized expressions no longer incorrectly infer names (+~220 passes). Array: 2,395→2,496/3,079 (81%), Iterator: 303→316/510 (62%).

46. ~~**Conformance batch 12: Set methods, Iterator helpers, TypedArray internals**~~ — ✅ Done (+231 new passes, 71.91% → 72.39%). Three orthogonal fixes: (1) Set new methods spec compliance: GetSetRecord with getter-aware property access, spec-compliant iterator protocol, correct observable operation ordering, iterator close on early termination, live iteration for mutation visibility. Set: 261→365/383 (95%). (2) Iterator helper method fixes: getter-aware iterator protocol throughout, GetIteratorFlattenable for flatMap, IteratorCloseAll with reverse ordering, zip/zipKeyed complete rewrite with spec-compliant mode/padding/strict handling, null-prototype result objects for zipKeyed, proper argument validation order for take/drop. Iterator: 316→436/510 (85%). (3) TypedArray internal methods: CanonicalNumericIndexString (§7.1.4.1), IsValidIntegerIndex (§10.4.5.14), TypedArray [[Get]]/[[Set]]/[[Delete]]/[[HasProperty]]/[[DefineOwnProperty]] per spec, ToNumber/ToBigInt coercion before index check, buffer-arg constructor to_index() fixes. TypedArrayConstructors: 405→498/736 (67%).

---

## Cross-Cutting Concerns

These are tracked across all phases:

- [x] **Strict mode** — enforce throughout parser and runtime
- [x] **Unicode** — full Unicode support in lexer, identifiers, strings
- [ ] **Unicode RegExp** — Unicode property escapes, `v` flag
- [ ] **Error reporting** — quality error messages with source locations
- [ ] **Spec compliance annotations** — link code to spec section IDs
- [x] **Garbage collection** — mark-and-sweep GC with ephemeron support for WeakMap/WeakSet
- [ ] **Performance** — profile and optimize hot paths after correctness
- [x] **Annex B (partial)** — String HTML methods, substr, escape/unescape, Date getYear/setYear/toGMTString (+117 passes)

---

## Test262 Integration

- Test harness: Rust binary that runs test262 `.js` files against our engine
- Harness files: `test262/harness/*.js` must be pre-loaded (assert.js, sta.js, etc.)
- Metadata parsing: each test has YAML frontmatter with flags, features, negative expectations
- Progress tracking: after each implementation session, run suite and update this file

### Test262 Breakdown by Area

| Area | Tests | Notes |
|------|-------|-------|
| `language/expressions` | 11,093 | Largest language category |
| `language/statements` | 9,337 | Second largest |
| `language/module-code` | 737 | |
| `language/literals` | 534 | |
| `language/eval-code` | 347 | |
| `language/identifiers` | 268 | |
| `language/arguments-object` | 263 | |
| `language/function-code` | 217 | |
| `language/import` | 162 | |
| `language/block-scope` | 145 | |
| `language/types` | 113 | |
| `language/asi` | 102 | |
| `language/` (other) | ~400 | white-space, comments, keywords, etc. |
| `built-ins/Temporal` | 4,482 | Stage 3 — optional |
| `built-ins/Object` | 3,411 | |
| `built-ins/Array` | 3,079 | |
| `built-ins/RegExp` | 1,879 | 1,101 (58.6%) |
| `built-ins/TypedArray` | 1,438 | 786 |
| `built-ins/String` | 1,215 | |
| `built-ins/` (rest) | ~8,000+ | All other built-ins |
| `annexB` | 1,086 | Legacy web compat |
| `intl402` | varies | Internationalization — optional |

---

## Milestone Targets

| Milestone | Description | Target Tests | Status |
|-----------|-------------|--------------|--------|
| M0 | CLI runs, exits with code | 0 | ✅ |
| M1 | Numeric/string/bool literals evaluate | ~50 | ✅ |
| M2 | Variable declarations + basic expressions | ~500 | ✅ |
| M3 | Control flow (if/while/for) | ~1,500 | ✅ |
| M4 | Functions (basic call/return) | ~3,000 | ✅ |
| M5 | Objects + prototypes | ~6,000 | ✅ |
| M6 | All expressions + statements | ~15,000 | 🟡 ~12,000 |
| M7 | Built-in objects (Object, Array, String, Number, Math, JSON) | ~25,000 | 🟡 ~16,828 |
| M8 | Classes, iterators, generators, async/await | ~35,000 | ⬜ Partial |
| M9 | RegExp, Proxy, Reflect, Promise, modules | ~45,000 | ⬜ |
| M10 | Full spec compliance | ~48,000+ | ⬜ |
