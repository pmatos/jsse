# Phase 6: Evaluation — Expressions & Statements

**Spec Reference:** §13 (Expressions), §14 (Statements & Declarations) — Runtime Semantics

## Goal
Implement runtime evaluation for all expression and statement types.

## Tasks

### 6.1 Literal Evaluation
- [x] `null` → Null value
- [x] `true` / `false` → Boolean value
- [x] NumericLiteral → Number / BigInt value
- [x] StringLiteral → String value
- [x] RegularExpressionLiteral → RegExp object creation
- [x] TemplateLiteral evaluation (string coercion, tagged templates)
- [x] ArrayLiteral → Array creation (elision, spread)
- [x] ObjectLiteral → Object creation (property definitions, computed, spread, get/set, `__proto__`)

### 6.2 Identifier & Reference Evaluation
- [x] IdentifierReference → Reference Record via ResolveBinding
- [x] `this` → ResolveThisBinding
- [x] Property access (dot, bracket) → Reference Record
- [x] `super` property access
- [x] Optional chaining `?.`

### 6.3 Unary & Update Operators
- [x] Prefix/postfix `++`/`--` (ToNumeric, increment, PutValue)
- [x] `delete` (Reference deletion semantics)
- [x] `void` (evaluate and return undefined)
- [x] `typeof` (unresolvable reference → "undefined")
- [x] Unary `+` (ToNumber), `-` (negate)
- [x] `~` (ToInt32 then complement)
- [x] `!` (ToBoolean then negate)

### 6.4 Binary Operators — Arithmetic
- [x] `+` (addition / string concatenation via ToPrimitive)
- [x] `-` (subtraction)
- [x] `*` (multiplication)
- [x] `/` (division)
- [x] `%` (remainder)
- [x] `**` (exponentiation)

### 6.5 Binary Operators — Bitwise & Shift
- [x] `<<`, `>>`, `>>>` (shift operators — ToInt32/ToUint32)
- [x] `&`, `^`, `|` (bitwise ops — ToInt32)

### 6.6 Binary Operators — Relational & Equality
- [x] `<`, `>`, `<=`, `>=` (IsLessThan abstract relational comparison)
- [x] `==`, `!=` (IsLooselyEqual — coercion rules)
- [x] `===`, `!==` (IsStrictlyEqual)
- [x] `instanceof` (OrdinaryHasInstance, @@hasInstance)
- [x] `in` (HasProperty, private `#field in obj`)

### 6.7 Logical & Short-Circuit Operators
- [x] `&&` (short-circuit AND)
- [x] `||` (short-circuit OR)
- [x] `??` (nullish coalescing — only null/undefined)

### 6.8 Conditional Operator
- [x] `? :` (ternary — ToBoolean condition)

### 6.9 Assignment
- [x] Simple assignment `=` (PutValue)
- [x] Compound assignment (`+=`, `-=`, etc.)
- [x] Logical assignment (`&&=`, `||=`, `??=`)
- [x] Destructuring assignment (array/object patterns)
  - [x] Array destructuring: iterator protocol, rest element, defaults
  - [x] Object destructuring: property access, rest properties, defaults

### 6.10 Comma Operator
- [x] Evaluate left, discard result, evaluate right

### 6.11 Block Statement & Scoping
- [x] Block: new declarative environment
- [x] Block scoped declarations (`let`, `const`) — TDZ semantics
- [x] Var hoisting within blocks

### 6.12 Variable Declarations
- [x] `var` — hoist to function/global scope
- [x] `let` — block scope, TDZ
- [x] `const` — block scope, TDZ, immutable binding
- [ ] `using` — disposable resource management
- [ ] `await using` — async disposable
- [x] Destructuring in declarations

### 6.13 Control Flow — Conditional
- [x] `if` / `else`
- [x] `switch` / `case` / `default` (strict equality matching)

### 6.14 Control Flow — Loops
- [x] `while`
- [x] `do`-`while`
- [x] `for` (init/test/update with scoping)
- [x] `for`-`in` (EnumerateObjectProperties)
- [x] `for`-`of` (GetIterator, IteratorStep) — partial, needs Iterator built-in
- [ ] `for await`-`of` (async iteration)
- [x] Loop body scoping: per-iteration environment for `let`/`const`
- [x] `break` / `continue` (with and without labels)
- [x] Labelled statements

### 6.15 Control Flow — Exception Handling
- [x] `throw` expression evaluation
- [x] `try` / `catch` — catch binding, new environment
- [x] `try` / `finally`
- [x] `try` / `catch` / `finally`
- [x] Catch parameter destructuring
- [x] Optional catch binding

### 6.16 Other Statements
- [x] `return` statement
- [x] `with` statement (new object environment — sloppy mode only)
- [x] `debugger` statement (no-op or host hook)
- [x] Expression statement (evaluate, discard result)
- [x] Empty statement

### 6.17 Function Calls
- [x] Function call evaluation
- [x] Argument list evaluation (spread in calls)
- [x] `new` expression evaluation
- [x] `super()` call in constructors
- [x] `super.property` access
- [x] Direct/indirect `eval()`
- [ ] Tail call optimization (§15.10)

## test262 Tests
- `test262/test/language/expressions/` — 11,093 tests
  - `addition/` — 48, `subtraction/` — 38, `multiplication/` — 40, `division/` — 45
  - `assignment/` — 485, `compound-assignment/` — 454
  - `logical-and/` — 18, `logical-or/` — 18, `coalesce/` — 24
  - `conditional/` — 22
  - `class/` — 4,059 (overlaps with Phase 7)
  - `object/` — 1,170, `array/` — 52
  - `template-literal/` — 57, `tagged-template/` — 27
  - `typeof/` — 16, `delete/` — 69, `void/` — 9
  - `call/` — 92, `new/` — 59
  - ... and all others
- `test262/test/language/statements/` — 9,337 tests
  - `variable/` — 178, `let/` — 145, `const/` — 136
  - `if/` — 69, `switch/` — 111
  - `while/` — 38, `do-while/` — 36, `for/` — 385
  - `for-in/` — 115, `for-of/` — 751, `for-await-of/` — 1,234
  - `try/` — 201, `throw/` — 14
  - `block/` — 21, `empty/` — 2, `expression/` — 3
  - `break/` — 20, `continue/` — 24, `return/` — 16
  - `with/` — 181, `labeled/` — 24, `debugger/` — 2
  - `class/` — 4,367 (overlaps with Phase 7)
  - `function/` — 451 (overlaps with Phase 7)
- `test262/test/language/eval-code/` — 347 tests
- `test262/test/language/directive-prologue/` — 62 tests
- `test262/test/language/statementList/` — 80 tests
