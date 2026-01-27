# JSSE — Master Implementation Plan

A from-scratch JavaScript engine in Rust, fully spec-compliant with ECMA-262.

**Total test262 tests:** ~53,114
**Current pass rate:** 0 / 53,114 (0%)

---

## Phased Implementation Roadmap

The engine is broken into 10 phases, ordered by dependency. Each phase has a detailed sub-plan in `plan/`.

| Phase | Name | Spec Sections | Detail | Est. Tests |
|-------|------|---------------|--------|------------|
| 1 | [Project Scaffolding & Infrastructure](plan/phase-01-infrastructure.md) | — | Rust project, CLI, test harness, CI | — |
| 2 | [Types & Values](plan/phase-02-types.md) | §6 | Language types, spec types, type conversions | ~113 |
| 3 | [Lexer](plan/phase-03-lexer.md) | §12 | Lexical grammar, tokens, Unicode | ~800+ |
| 4 | [Parser (AST)](plan/phase-04-parser.md) | §13–16 | Expressions, statements, functions, modules | ~2,000+ |
| 5 | [Runtime Core](plan/phase-05-runtime.md) | §6–10 | Environments, execution contexts, objects, abstract ops | ~3,500+ |
| 6 | [Evaluation — Expressions & Statements](plan/phase-06-evaluation.md) | §13–14 | Runtime semantics for all language constructs | ~20,000+ |
| 7 | [Functions & Classes](plan/phase-07-functions-classes.md) | §15 | Functions, arrow, async, generators, classes | ~10,000+ |
| 8 | [Modules & Scripts](plan/phase-08-modules.md) | §16 | Script/module evaluation, import/export | ~900+ |
| 9 | [Built-in Objects](plan/phase-09-builtins.md) | §19–28 | All standard built-ins | ~20,000+ |
| 10 | [Advanced Features](plan/phase-10-advanced.md) | §17,25–27,B | Error handling, memory model, Proxy, Reflect, Annex B | ~3,000+ |

---

## Cross-Cutting Concerns

These are tracked across all phases:

- [ ] **Strict mode** — enforce throughout parser and runtime
- [ ] **Unicode** — full Unicode support in lexer, identifiers, strings, RegExp
- [ ] **Error reporting** — quality error messages with source locations
- [ ] **Spec compliance annotations** — link code to spec section IDs
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

| Milestone | Description | Target Tests |
|-----------|-------------|-------------|
| M0 | CLI runs, exits with code | 0 |
| M1 | Numeric/string/bool literals evaluate | ~50 |
| M2 | Variable declarations + basic expressions | ~500 |
| M3 | Control flow (if/while/for) | ~1,500 |
| M4 | Functions (basic call/return) | ~3,000 |
| M5 | Objects + prototypes | ~6,000 |
| M6 | All expressions + statements | ~15,000 |
| M7 | Built-in objects (Object, Array, String, Number, Math, JSON) | ~25,000 |
| M8 | Classes, iterators, generators, async/await | ~35,000 |
| M9 | RegExp, Proxy, Reflect, Promise, modules | ~45,000 |
| M10 | Full spec compliance | ~50,000+ |
