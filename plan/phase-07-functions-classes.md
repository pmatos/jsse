# Phase 7: Functions & Classes

**Spec Reference:** §15 (Functions and Classes), portions of §10 (Function Objects)

## Goal
Implement all function types (normal, arrow, generator, async, async generator) and full class semantics including private fields.

## Tasks

### 7.1 Ordinary Functions (§15.2)
- [x] FunctionDeclarationInstantiation
- [x] Function hoisting
- [x] Strict mode propagation
- [ ] `arguments` object (mapped and unmapped) — **BLOCKER: ~140 tests**
- [x] Default parameter values
- [x] Rest parameters `...args`
- [x] Duplicate parameter names (sloppy mode)
- [x] `"use strict"` directive in function body

### 7.2 Arrow Functions (§15.3)
- [x] Lexical `this` binding
- [x] No `arguments` object
- [x] No `new.target`
- [x] Cannot be used as constructors
- [x] Concise body (expression) vs block body

### 7.3 Method Definitions (§15.4)
- [x] Method shorthand `{ foo() {} }`
- [x] Getter `get` / Setter `set`
- [x] `super` method calls
- [x] Computed method names `{ [expr]() {} }`

### 7.4 Generator Functions (§15.5, §27.5) — ✅ ~85% working
- [x] `function*` declaration and expression (parsing)
- [x] `yield` expression (runtime)
- [x] `yield*` delegation
- [x] Generator prototype chain
- [x] GeneratorStart, GeneratorResume (state machine with persistent environment)
- [x] GeneratorYield
- [x] Generator `next()` / `return()` / `throw()`
- [x] Suspended execution context (environment persists between yields)
- [x] Local variable persistence between yield calls
- [ ] GeneratorResumeAbrupt (throw/return with try/finally)
- Generator statements: 225/266 (85%), expressions: 233/290 (80%), GeneratorPrototype: 38/61 (62%)

### 7.5 Async Functions (§15.8, §27.7) — ✅ Basic support
- [x] `async function` declaration and expression (parsing)
- [x] `await` expression (runtime)
- [x] Implicit promise wrapping
- [x] Async function start
- [x] Await fulfilled/rejected reactions (synchronous drain)
- [ ] Async-from-sync iterator

### 7.6 Async Generator Functions (§15.6, §27.6) — ✅ Done
- [x] `async function*` declaration and expression (parsing)
- [x] `yield` and `await` in async generators
- [x] AsyncGenerator iterator state, replay-based execution
- [x] `next()` / `return()` / `throw()` returning Promises
- [x] %AsyncIteratorPrototype% and %AsyncGeneratorPrototype% setup
- [x] %AsyncGeneratorFunction.prototype% with proper prototype chain
- [x] `yield*` delegation with async iterator protocol
- [x] Rejected promises for type errors in next/return/throw
- [x] Nested yield expression evaluation fix

### 7.7 Class Definitions (§15.7)
- [x] ClassDefinitionEvaluation
- [x] Constructor function creation
- [x] `extends` and prototype chain setup
- [x] `super()` in derived class constructors
- [x] Derived class `this` binding (TDZ until `super()` called)
- [x] Instance methods and static methods
- [x] Instance fields
- [x] Static fields
- [x] Private fields (`#field`) — parsing done, runtime partial
  - [x] Private instance fields
  - [x] Private static fields
  - [ ] Private methods (instance and static)
  - [ ] Private getters/setters
  - [x] `#field in obj` (private brand check)
- [x] Static initialization blocks `static { ... }`
- [x] Computed property names in class body
- [x] `new.target` in constructors
- [x] Class name binding (const in class body, not const outside)
- [ ] `toString()` of class

### 7.8 Function Built-in Compliance (§20.2) — ✅ 94% (839/893)
- [x] Function() constructor strict mode inheritance (sloppy closure env)
- [x] Function() constructor ToString coercion (to_string_value)
- [x] Function.prototype.toString Proxy handling (callable vs non-callable)
- [x] Function.prototype.apply CreateListFromArrayLike (getter-aware, arity fix)
- [x] Class constructor callable check (TypeError without `new`)
- [x] Derived constructor return value check order (TypeError before ReferenceError)
- [x] Bound function target tracking (bound_target_function, bound_args, newTarget resolution)
- [x] OrdinaryHasInstance bound function chain + getter-aware prototype
- [x] bind length Infinity/edge cases (HasOwnProperty, Number type check, f64)
- [x] bind name getter-aware error propagation

### 7.9 Tail Position Calls (§15.10)
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
