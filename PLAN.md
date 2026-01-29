# JSSE — Master Implementation Plan

A from-scratch JavaScript engine in Rust, fully spec-compliant with ECMA-262.

**Total test262 tests:** ~48,257 (excluding Temporal/intl402)
**Current pass rate:** 12,878 / 42,076 run (30.61%)
*Skipped: 6,181 module and async tests*

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
| 7 | [Functions & Classes](plan/phase-07-functions-classes.md) | §15 | 🟡 ~50% | Functions, classes work; generators/async don't |
| 8 | [Modules & Scripts](plan/phase-08-modules.md) | §16 | ⬜ 0% | Script/module evaluation, import/export |
| 9 | [Built-in Objects](plan/phase-09-builtins.md) | §19–28 | 🟡 ~40% | Object, Array, String, Math, JSON work |
| 10 | [Advanced Features](plan/phase-10-advanced.md) | §17,25–27,B | ⬜ 0% | Error handling, memory model, Proxy, Reflect, Annex B |

---

## Current Built-in Status

| Built-in | Pass Rate | Tests |
|----------|-----------|-------|
| Object | 35% | 1,199/3,411 |
| Array | 25% | 736/2,989 |
| String | 24% | 294/1,215 |
| Function | 19% | 95/509 |
| Iterator | 27% | 138/510 |
| Promise | 0% | 0/281 |
| Map | 50% | 103/204 |
| Set | 68% | 261/383 |
| Date | 0% | 0/594 |

---

## Current Blockers (Highest Impact)

These features block significant numbers of tests:

1. ~~**`arguments` object**~~ — ✅ Done (82/203, 40.39%). Mapped arguments + Symbol.iterator implemented.
2. ~~**Garbage collection**~~ — ✅ Done. Mark-and-sweep GC with free-list reuse (148 MB → 11 MB on 100k object alloc).
3. **Generator `yield` evaluation** — Parsing works, runtime doesn't. Blocks ~245 remaining Iterator tests and async.
4. **Iterator protocol** — Breaks `for...of`, spread on non-arrays, many built-in methods.
4. **Promise** — Blocks all async/await runtime.
5. ~~**Map/Set**~~ — ✅ Done (Map: 103/204, Set: 261/383). Remaining failures: native fn `.length` properties, Proxy/Reflect/Symbol.species deps.
6. **Date** — Not implemented. 594 tests, fundamental.

---

## Recommended Next Tasks (Priority Order)

1. ~~**Complete `arguments` object (mapped arguments)**~~ — ✅ Done
2. ~~**Garbage collection**~~ — ✅ Done
3. ~~**Complete Iterator built-in**~~ — ✅ Done (138/510, 27%). Constructor, helpers (toArray/forEach/reduce/some/every/find/map/filter/take/drop/flatMap), Iterator.from, Iterator.concat. Remaining failures need generators.
3. ~~**Implement Map and Set**~~ — ✅ Done (364 new passes)
4. **Implement Date** — 594 tests, fundamental built-in
5. **Generator `yield` evaluation** — Enables iterator protocol, async later
6. **Private fields runtime** — Parsing done, evaluation needed

---

## Cross-Cutting Concerns

These are tracked across all phases:

- [x] **Strict mode** — enforce throughout parser and runtime
- [x] **Unicode** — full Unicode support in lexer, identifiers, strings
- [ ] **Unicode RegExp** — Unicode property escapes, `v` flag
- [ ] **Error reporting** — quality error messages with source locations
- [ ] **Spec compliance annotations** — link code to spec section IDs
- [ ] **Garbage collection** — replace append-only object store with GC-managed heap
- [ ] **Performance** — profile and optimize hot paths after correctness
- [ ] **Annex B** — web legacy compat (1,086 tests in `test262/test/annexB/`)

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
| `built-ins/TypedArray` | 1,438 | |
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
| M7 | Built-in objects (Object, Array, String, Number, Math, JSON) | ~25,000 | 🟡 In progress |
| M8 | Classes, iterators, generators, async/await | ~35,000 | ⬜ Partial |
| M9 | RegExp, Proxy, Reflect, Promise, modules | ~45,000 | ⬜ |
| M10 | Full spec compliance | ~48,000+ | ⬜ |
