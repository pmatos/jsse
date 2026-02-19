# JSSE — Master Implementation Plan

A from-scratch JavaScript engine in Rust, fully spec-compliant with ECMA-262.

**Total test262 scenarios:** 92,496 (48,257 files, dual strict/non-strict per spec)
**Current pass rate:** 83,513 / 92,496 (90.29%)

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

Scenario counts (dual strict/non-strict per spec INTERPRETING.md).

| Built-in | Pass Rate | Scenarios |
|----------|-----------|-----------|
| Temporal | 100% | 8,964/8,964 |
| Reflect | 100% | 306/306 |
| block-scope | 100% | 287/287 |
| Map | 100% | 403/405 |
| Number | 99% | 662/670 |
| FinalizationRegistry | 98% | 92/94 |
| Set | 97% | 742/764 |
| Math | 97% | 632/654 |
| WeakRef | 97% | 56/58 |
| Iterator | 96% | 980/1,020 |
| Promise | 96% | 1,220/1,272 |
| String | 95% | 2,312/2,427 |
| Date | 95% | 1,124/1,188 |
| for-in | 95% | 188/198 |
| Object | 94% | 6,407/6,802 |
| Function | 94% | 839/893 |
| Array | 91% | 5,567/6,111 |
| DataView | 85% | 956/1,122 |
| TypedArray | 83% | 2,380/2,860 |
| RegExp | 93% | 3,481/3,756 |
| Proxy | 79% | 478/607 |
| TypedArrayConstructors | 77% | 1,116/1,442 |
| Symbol | 77% | 142/184 |
| annexB | 64% | 884/1,377 |

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

47. ~~**Conformance batch 13: delete-private early error, numeric separators, function .length**~~ — ✅ Done (+286 new passes, 72.39% → 72.98%). Three orthogonal fixes: (1) Delete private name early error: `delete obj.#x` and `delete (obj.#x)` now produce SyntaxError per spec Static Semantics early error rules for UnaryExpression. Parser-only change in expressions.rs (+192 passes). (2) Numeric separator validation: reject invalid `_` placements in numeric literals — double underscores, trailing underscore, leading after prefix, adjacent to `.` or `e`/`E`. Lexer-only change (+25 passes). (3) Function `.length` stops counting at first default/rest parameter per §9.2.6 SetFunctionLength, and async non-generator functions no longer get `.prototype` property since they are not constructable (+69 passes).

48. ~~**Conformance batch 14: ThrowTypeError intrinsic, Uint8Array base64/hex, Math.f16round/sumPrecise**~~ — ✅ Done (+133 new passes, 72.98% → 73.26%). Three orthogonal features: (1) %ThrowTypeError% intrinsic (§10.2.4): shared frozen function that throws TypeError, wired into strict arguments callee/caller accessors and Function.prototype.caller/arguments poison pills. Sloppy non-arrow/non-generator/non-async functions get own `caller`/`arguments` = null to shadow prototype accessor (Annex B). Method definitions strip these properties. ThrowTypeError: 6→13/14 (93%), Function: 397→432/509 (85%). (2) Uint8Array base64/hex methods (ES2025): fromBase64, fromHex, toBase64, toHex, setFromBase64, setFromHex with alphabet options, lastChunkHandling modes, whitespace stripping, proper error reporting. Uint8Array: 8→66/68 (97%). (3) Math.f16round (IEEE 754 binary16 conversion with manual bit manipulation) and Math.sumPrecise (BigInt exact arithmetic summation with iterator protocol). Math: 300→316/327 (97%).

49. ~~**Conformance batch 15: DataView Float16, class this TDZ, Object.__proto__ + Array fixes**~~ — ✅ Done (+338 new passes, 73.26% → 73.96%). Three orthogonal features: (1) DataView getFloat16/setFloat16 (ES2025) and DataView setter coercion ordering fix — setters now perform ToNumber/ToBigInt on value BEFORE buffer detachment and bounds checks per spec. DataView: 293→454/561 (80%). (2) Derived constructor `this` TDZ enforcement — `this` is uninitialized until `super()` is called in derived class constructors, with proper new.target forwarding via `construct_with_new_target()`. Class `.prototype` property now writable:false per spec. (3) Object.prototype.__proto__ accessor (Annex B §B.2.2.1), Array.prototype[@@unscopables], and Symbol.isConcatSpreadable support in Array.prototype.concat. Object: 3,176→3,181/3,411 (93%), Array: 2,496→2,553/3,079 (82%), Promise: 548→571/639 (89%), Reflect: 124→132/153 (86%).

50. ~~**Conformance batch 16: Annex B block-level functions, RegExp fixes**~~ — ✅ Done (+363 new passes, 73.96% → 74.71%). Two orthogonal features: (1) Annex B.3.3/B.3.4 block-level function declarations in sloppy mode: var-scope hoisting with copy-on-evaluation semantics, parameter/lexical conflict detection, generator/async exclusion, switch case function hoisting, IsLabelledFunction checks for iteration bodies and if-with-else. Parser: function declarations in if-bodies and labeled statements (sloppy mode only). annexB: 399→674/1,086 (62%). (2) RegExp fixes: pattern validation, type checks, `regexp_exec_abstract()` usage for spec-compliant exec dispatch. RegExp: 1,216→1,289/1,879 (69%).

51. ~~**Conformance batch 17: RegExp, Array, Date improvements**~~ — ✅ Done (+359 new passes, 74.71% → 75.46%). Three orthogonal features implemented in parallel: (1) RegExp: removed own flag/source/flags data properties from RegExp instances (now prototype getters only), internal slots via `__original_source__`/`__original_flags__`, spec_set() helper, Symbol.match/search/split/matchAll rewrites, RegExp constructor IsRegExp check, escape_regexp_pattern(), pattern validation. RegExp: 1,289→1,389/1,879 (74%). (2) Array: coercion fixes (to_number_value/to_string_value), spec-compliant Set/Delete/HasProperty with throw variants, CreateDataPropertyOrThrow with extensibility checks, ArraySpeciesCreate with Proxy support, integer limit checks (2^53-1), toString fallback to %Object.prototype.toString%. Array: 2,553→2,711/3,079 (88%). (3) Date: removed primitive_value from Date.prototype, setter error propagation with to_number_value(), Symbol.toPrimitive rewrite (check Object not Date), toJSON rewrite per spec, constructor ToPrimitive hint="default". Date: 493→561/594 (94%). Integration fixes: String.prototype.replaceAll/matchAll now use Get(searchValue, "flags") and IsRegExp per spec, TypedArray.prototype.join separator coercion, array_elements sync for indexed property mutation.

52. ~~**Conformance batch 18: Number/Symbol coercion, constructor fix, RegExp patterns**~~ — ✅ Done (+71 new passes, 75.46% → 75.60%). Three orthogonal fixes implemented in parallel: (1) Number/Symbol: Number prototype methods (toString, toFixed, toExponential, toPrecision) now use `to_number_value()` for argument coercion with proper ToPrimitive/Symbol TypeError. Argument coercion order fixed (coerce BEFORE NaN/Infinity check). `toFixed` returns ToString(x) for |x|>=10^21. `string_to_number` rejects case-insensitive "infinity". Symbol()/Symbol.for() use `to_string_value()` for description coercion. Number() constructor propagates ToPrimitive errors. Number: 331/335 (98%). (2) Constructor own property: removed spurious `constructor` own property set on every `new` object instance — `constructor` is now properly inherited from prototype chain only. Object: 3,181→3,184/3,411 (93%). (3) RegExp: quantifier-at-start validation (SyntaxError for `*`/`+`/`?`/`{}` at pattern start or after `(`/`|`), forward backreference handling (\N matches empty string when group not yet captured), octal escape handling for non-Unicode mode. RegExp: 1,389→1,432/1,879 (76%).

53. ~~**Real-world app integration: Acorn, Marked, Prettier**~~ — ✅ Done (+79 net new passes, 76.25% → 76.42%).

55. ~~**Conformance batch 22: String, Iterator helpers, Proxy/native fn prototype**~~ — Done (+108 new passes, 76.64% → 76.87%). Three orthogonal fixes: (1) String built-in fixes: getter-aware Symbol method lookups (match/replace/search/split), IsRegExp check, split ToUint32 limit, replaceAll upfront ToString, String.prototype as String wrapper object, matchAll fixes. String: 1,141→1,157/1,215 (95%). (2) Iterator helper fixes: re-entrancy detection via `running` flag, return method error propagation, take/drop argument validation with iterator close, concat.length=0, Iterator.from rewrite with WrapForValidIteratorPrototype, constructor/toStringTag as accessor properties. Iterator: 438→490/510 (96%). (3) Native function [[Prototype]] fix: `create_function()` uses `self.function_prototype` field directly, retroactive walk extended to cover internal prototype fields (iterator/collection/generator protos), generator/async function prototypes unconditionally fixed, Error.prototype.toString throws TypeError for non-object `this`. Function: 433→446/509 (88%), Proxy: 231→245/311 (79%), Object: 3,184→3,216/3,411 (94%), Set: 365→372/383 (97%).

57. ~~**Dynamic import from scripts fix**~~ — ✅ Done (+203 new passes, 76.89% → 77.31%). Two fixes: (1) jsse: `run_with_path()` now sets `current_module_path` for scripts (not just modules), enabling `import()` to resolve relative specifiers from script files. (2) Test runner: temp files for non-module tests now written in the same directory as the original test file (not `/tmp/`), so FIXTURE module files resolve correctly.

56. ~~**Conformance batch 23: Async generator yield* delegation, class method constructability**~~ — ✅ Done (+11 new passes, 76.87% → 76.89%). Two fixes: (1) Async generator yield* delegation: continuation path now awaits iterator result before extracting done/value (matching initial setup path), return/throw forwarding to inner iterator during yield* delegation, IteratorValue error propagation into generator try/catch via catch state jump. (2) Class method constructability: added `is_method` flag to `JsFunction::User`, class methods marked non-constructable (no `.prototype`, TypeError on `new`), generator/async-generator methods retain `.prototype` for generator prototype chain, `is_constructor` check updated in eval_new/create_function/promise.rs.

54. ~~**Conformance batch 21: Reflect, CanBeHeldWeakly, Proxy has+with**~~ — ✅ Done (+109 new passes, 76.42% → 76.64%). Three orthogonal features implemented in parallel: (1) Reflect built-in fixes: Reflect[Symbol.toStringTag], setPrototypeOf returns false (not throws) per spec §9.1.2, ownKeys property ordering (integer indices → strings → symbols per §9.1.12), Reflect.has.length=2, ToPropertyKey error propagation, CreateListFromArrayLike validation in apply/construct, Reflect.set accessor with receiver. Reflect: 132→153/153 (100%). (2) CanBeHeldWeakly + WeakRef/FinalizationRegistry: implemented CanBeHeldWeakly (§7.2.7) — objects and unregistered symbols can be held weakly, registered symbols and primitives cannot. Applied to WeakRef constructor, WeakMap.set, WeakSet.add, FinalizationRegistry register/unregister. Symbol tokens in FinalizationRegistry. OrdinaryCreateFromConstructor for NewTarget prototype. WeakRef: 24→28/29 (97%), FinalizationRegistry: 36→46/47 (98%). (3) Proxy `has` trap in `with` statement: Object Environment Record HasBinding now uses proxy-aware [[HasProperty]] for with-scope lookups. GetBindingValue uses get_object_property for proxy get trap. Proxy/has: 17→24/26 (92%), with: 121→124/181 (69%). Real-world JavaScript apps (Acorn parser, Marked markdown, Prettier formatter) exposed engine bugs fixed in feat/realapp branch. Key fixes: (1) Parser: `async`/`of` as contextual identifiers, `true`/`false`/`null` as dot member property names and object property keys, optional chaining continuation (`.prop`, `[expr]`, `()` after `?.`). (2) Array: GC rooting for intermediate arrays in concat/slice/map/filter/splice. (3) GC: module environment and promise data tracing, call stack environment rooting in exec_statements, private field definition tracing. (4) Eval: private field access on class instances, computed member access improvements. Post-merge fixes: array index bounds check (indices ≥ 2^32-1 are not valid array indices, density check for sparse arrays), exponentiation spec compliance (±1 ** ±∞ = NaN per §6.1.6.1.4), reserved word shorthand rejection (`({false})` is SyntaxError), RegExp `\d`/`\D`/`\w`/`\W` translated to ASCII-only character classes per spec. Note: 37 RegExp property-escape tests that were passing vacuously (broken buildString returned empty arrays) now correctly fail due to regex crate Unicode data version mismatch. Array: 2,711→2,734/3,079 (89%), RegExp: 1,432→1,398/1,879 (74%, -34 from vacuous property-escape fixes).

58. ~~**Conformance batch 24: TypedArray species, BigInt coercion, set() rewrite**~~ — ✅ Done (+189 new passes, 86.56% → 86.95%). TypedArray method completion: (1) TypedArraySpeciesCreate (§23.2.4.1): helper method using SpeciesConstructor + ContentType validation. Updated slice/map/filter to use species create, subarray to use SpeciesConstructor with buffer args. TypedArrayCreateSameType helper for toReversed/toSorted/with. (2) BigInt coercion: typed_array_coerce_value() delegates to to_number_value/to_bigint_value based on kind. Fixed fill(), set(), from(), of(), and constructor iterable/array-like/TypedArray-copy paths. ContentType compatibility check in constructor TypedArray-copy path. (3) set() rewrite: offset coercion via to_number_value() BEFORE detach check, array-like source uses per-element typed_array_coerce_value(), TypedArray-arg same-buffer overlap handling via Rc::ptr_eq + data cloning. TypedArray: 962→1,119/1,438 (77.8%), TypedArrayConstructors: 512→543/736 (73.8%).

59. ~~**Conformance batch 25: AllPrivateNamesValid, assignment order, logical assignment write-back**~~ — ✅ Done (+276 new passes, 86.95% → 87.52%). Three orthogonal fixes: (1) AllPrivateNamesValid parser early error (§15.7.1): `this.#x` in a class body where `#x` is never declared is now SyntaxError at parse time. Added `private_name_scopes` to parser with push/pop/declare/use helpers, nested class scope validation, `#constructor` prohibition, `super.#x` prohibition, eval private name context propagation via Environment chain (+99 passes). (2) Assignment evaluation order for member expressions: `eval_assign()` restructured so member expressions evaluate base → key → RHS per spec instead of RHS-first. Null/undefined base now throws TypeError. Compound assignment does ToPropertyKey+GetValue before RHS (+28 passes). (3) Logical assignment (`&&=`/`||=`/`??=`) member expression write-back: new `eval_logical_assign()` handles Identifier, dot/computed member, and private member targets with full property write logic including Proxy/setter/prototype chain support (+27 passes). Additional passes from interaction effects (+122).

---

## Cross-Cutting Concerns

These are tracked across all phases:

- [x] **Strict mode** — enforce throughout parser and runtime
- [x] **Unicode** — full Unicode support in lexer, identifiers, strings
- [x] **Unicode RegExp property escapes** — Generated Unicode 17.0.0 tables, expand `\p{...}`/`\P{...}` inline (+292 passes, 88.34% → 88.52%). RegExp property-escapes: 722→1,012/1,074 (94.23%).
- [x] **RegExp modifiers + dotAll** — Modifier syntax validation (`(?i:...)`, `(?s-m:...)`), dot line terminator fix (`.` no longer matches `\r`/`\u2028`/`\u2029`), modifier dotAll handling (+154 passes). RegExp: 3,240/3,892 (83.2%).
- [ ] **Unicode RegExp v flag** — `v` flag (unicodeSets) support
- [ ] **Error reporting** — quality error messages with source locations
- [ ] **Spec compliance annotations** — link code to spec section IDs
- [x] **Garbage collection** — mark-and-sweep GC with ephemeron support for WeakMap/WeakSet
- [ ] **Performance** — profile and optimize hot paths after correctness
- [x] **Annex B (partial)** — String HTML methods, substr, escape/unescape, Date getYear/setYear/toGMTString (+117 passes), block-level function declarations B.3.3/B.3.4 (+275 passes)

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
| `built-ins/Temporal` | 8,964 | 8,964 (100%) |
| `built-ins/Object` | 6,802 | 6,407 (94.2%) |
| `built-ins/Array` | 6,111 | 5,567 (91.1%) |
| `built-ins/RegExp` | 3,756 | 3,481 (92.7%) |
| `built-ins/TypedArray` | 2,860 | 2,380 (83.2%) |
| `built-ins/TypedArrayConstructors` | 1,442 | 1,116 (77.4%) |
| `built-ins/String` | 2,427 | 2,312 (95.3%) |
| `built-ins/` (rest) | ~16,000+ | All other built-ins |
| `annexB` | 1,377 | 884 (64.2%) |
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
| M8 | Classes, iterators, generators, async/await | ~65,000 | ✅ ~70,000 |
| M9 | RegExp, Proxy, Reflect, Promise, modules | ~85,000 | 🟡 83,513 |
| M10 | Full spec compliance | ~92,658 | ⬜ |
