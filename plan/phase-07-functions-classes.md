# Phase 7: Functions & Classes

**Spec Reference:** §15 (Functions and Classes), portions of §10 (Function Objects)

## Goal
Implement all function types (normal, arrow, generator, async, async generator) and full class semantics including private fields.

## Tasks

### 7.1 Ordinary Functions (§15.2)
- [ ] FunctionDeclarationInstantiation
- [ ] Function hoisting
- [ ] Strict mode propagation
- [ ] `arguments` object (mapped and unmapped)
- [ ] Default parameter values
- [ ] Rest parameters `...args`
- [ ] Duplicate parameter names (sloppy mode)
- [ ] `"use strict"` directive in function body

### 7.2 Arrow Functions (§15.3)
- [ ] Lexical `this` binding
- [ ] No `arguments` object
- [ ] No `new.target`
- [ ] Cannot be used as constructors
- [ ] Concise body (expression) vs block body

### 7.3 Method Definitions (§15.4)
- [ ] Method shorthand `{ foo() {} }`
- [ ] Getter `get` / Setter `set`
- [ ] `super` method calls
- [ ] Computed method names `{ [expr]() {} }`

### 7.4 Generator Functions (§15.5, §27.5)
- [ ] `function*` declaration and expression
- [ ] `yield` expression
- [ ] `yield*` delegation
- [ ] Generator prototype chain
- [ ] GeneratorStart, GeneratorResume, GeneratorResumeAbrupt
- [ ] GeneratorYield
- [ ] Generator `next()` / `return()` / `throw()`
- [ ] Suspended execution context

### 7.5 Async Functions (§15.8, §27.7)
- [ ] `async function` declaration and expression
- [ ] `await` expression
- [ ] Implicit promise wrapping
- [ ] Async function start
- [ ] Await fulfilled/rejected reactions
- [ ] Async-from-sync iterator

### 7.6 Async Generator Functions (§15.6, §27.6)
- [ ] `async function*` declaration and expression
- [ ] `yield` and `await` in async generators
- [ ] AsyncGeneratorStart / Resume / Yield / Return
- [ ] AsyncGeneratorEnqueue / Drain
- [ ] `next()` / `return()` / `throw()` returning Promises

### 7.7 Class Definitions (§15.7)
- [ ] ClassDefinitionEvaluation
- [ ] Constructor function creation
- [ ] `extends` and prototype chain setup
- [ ] `super()` in derived class constructors
- [ ] Derived class `this` binding (TDZ until `super()` called)
- [ ] Instance methods and static methods
- [ ] Instance fields
- [ ] Static fields
- [ ] Private fields (`#field`)
  - [ ] Private instance fields
  - [ ] Private static fields
  - [ ] Private methods (instance and static)
  - [ ] Private getters/setters
  - [ ] `#field in obj` (private brand check)
- [ ] Static initialization blocks `static { ... }`
- [ ] Computed property names in class body
- [ ] `new.target` in constructors
- [ ] Class name binding (const in class body, not const outside)
- [ ] `toString()` of class

### 7.8 Tail Position Calls (§15.10)
- [ ] IsInTailPosition
- [ ] PrepareForTailCall (optional optimization)

## test262 Tests
- `test262/test/language/expressions/class/` — 4,059 tests
- `test262/test/language/statements/class/` — 4,367 tests
- `test262/test/language/expressions/function/` — 264 tests
- `test262/test/language/statements/function/` — 451 tests
- `test262/test/language/expressions/arrow-function/` — 343 tests
- `test262/test/language/expressions/generators/` — 290 tests
- `test262/test/language/statements/generators/` — 266 tests
- `test262/test/language/expressions/async-function/` — 93 tests
- `test262/test/language/statements/async-function/` — 74 tests
- `test262/test/language/expressions/async-generator/` — 623 tests
- `test262/test/language/statements/async-generator/` — 301 tests
- `test262/test/language/expressions/yield/` — 63 tests
- `test262/test/language/expressions/await/` — 22 tests
- `test262/test/language/expressions/super/` — 94 tests
- `test262/test/language/expressions/new.target/` — 14 tests
- `test262/test/built-ins/Function/` — 509 tests
- `test262/test/built-ins/GeneratorFunction/` — 23 tests
- `test262/test/built-ins/AsyncFunction/` — 18 tests
- `test262/test/built-ins/AsyncGeneratorFunction/` — 23 tests
- `test262/test/built-ins/GeneratorPrototype/` — 61 tests
- `test262/test/built-ins/AsyncGeneratorPrototype/` — 48 tests
