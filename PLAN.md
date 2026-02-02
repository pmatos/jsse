# JSSE — Master Implementation Plan

A from-scratch JavaScript engine in Rust, fully spec-compliant with ECMA-262.

**Total test262 tests:** ~48,257 (excluding Temporal/intl402)
**Current pass rate:** 20,232 / 47,458 run (42.63%)
*Skipped: 799 module tests*

---

## Phased Implementation Roadmap

The engine is broken into 10 phases, ordered by dependency. Each phase has a detailed sub-plan in `plan/`.

| Phase | Name | Spec Sections | Status | Detail |
|-------|------|---------------|--------|--------|
| 1 | [Project Scaffolding & Infrastructure](plan/phase-01-infrastructure.md) | — | ✅ Complete | Rust project, CLI, test harness, CI |
| 2 | [Types & Values](plan/phase-02-types.md) | §6 | ✅ ~95% | Language types, spec types, type conversions |
| 3 | [Lexer](plan/phase-03-lexer.md) | §12 | ✅ Complete | Lexical grammar, tokens, Unicode |
| 4 | [Parser (AST)](plan/phase-04-parser.md) | §13–16 | 🟡 ~95% | Expressions, statements, functions (modules missing) |
| 5 | [Runtime Core](plan/phase-05-runtime.md) | §6–10 | 🟡 ~30% | Environments, execution contexts, objects |
| 6 | [Evaluation — Expressions & Statements](plan/phase-06-evaluation.md) | §13–14 | 🟡 ~60% | Most operators/statements work |
| 7 | [Functions & Classes](plan/phase-07-functions-classes.md) | §15 | 🟡 ~70% | Functions, classes, generators, async/await work |
| 8 | [Modules & Scripts](plan/phase-08-modules.md) | §16 | ⬜ 0% | Script/module evaluation, import/export |
| 9 | [Built-in Objects](plan/phase-09-builtins.md) | §19–28 | 🟡 ~40% | Object, Array, String, Math, JSON (105/165), URI encode/decode work |
| 10 | [Advanced Features](plan/phase-10-advanced.md) | §17,25–27,B | 🟡 ~20% | Error handling, memory model, Proxy, Reflect, Annex B |

---

## Current Built-in Status

| Built-in | Pass Rate | Tests |
|----------|-----------|-------|
| Object | 50% | 1,704/3,411 |
| Array | 67% | 2,050/3,079 |
| String | 71% | 863/1,215 |
| Function | 55% | 279/509 |
| Iterator | 57% | 290/510 |
| Promise | 30% | 190/639 |
| Map | 50% | 103/204 |
| Set | 68% | 261/383 |
| Date | 51% | 305/594 |
| Reflect | 35% | 54/153 |
| Proxy | 39% | 120/311 |
| Symbol | 28% | 26/94 |

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
| `built-ins/RegExp` | 1,879 | |
| `built-ins/TypedArray` | 1,438 | 669 |
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
