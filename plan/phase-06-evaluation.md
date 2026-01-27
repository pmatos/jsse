# Phase 6: Evaluation — Expressions & Statements

**Spec Reference:** §13 (Expressions), §14 (Statements & Declarations) — Runtime Semantics

## Goal
Implement runtime evaluation for all expression and statement types.

## Tasks

### 6.1 Literal Evaluation
- [ ] `null` → Null value
- [ ] `true` / `false` → Boolean value
- [ ] NumericLiteral → Number / BigInt value
- [ ] StringLiteral → String value
- [ ] RegularExpressionLiteral → RegExp object creation
- [ ] TemplateLiteral evaluation (string coercion, tagged templates)
- [ ] ArrayLiteral → Array creation (elision, spread)
- [ ] ObjectLiteral → Object creation (property definitions, computed, spread, get/set, `__proto__`)

### 6.2 Identifier & Reference Evaluation
- [ ] IdentifierReference → Reference Record via ResolveBinding
- [ ] `this` → ResolveThisBinding
- [ ] Property access (dot, bracket) → Reference Record
- [ ] `super` property access
- [ ] Optional chaining `?.`

### 6.3 Unary & Update Operators
- [ ] Prefix/postfix `++`/`--` (ToNumeric, increment, PutValue)
- [ ] `delete` (Reference deletion semantics)
- [ ] `void` (evaluate and return undefined)
- [ ] `typeof` (unresolvable reference → "undefined")
- [ ] Unary `+` (ToNumber), `-` (negate)
- [ ] `~` (ToInt32 then complement)
- [ ] `!` (ToBoolean then negate)

### 6.4 Binary Operators — Arithmetic
- [ ] `+` (addition / string concatenation via ToPrimitive)
- [ ] `-` (subtraction)
- [ ] `*` (multiplication)
- [ ] `/` (division)
- [ ] `%` (remainder)
- [ ] `**` (exponentiation)

### 6.5 Binary Operators — Bitwise & Shift
- [ ] `<<`, `>>`, `>>>` (shift operators — ToInt32/ToUint32)
- [ ] `&`, `^`, `|` (bitwise ops — ToInt32)

### 6.6 Binary Operators — Relational & Equality
- [ ] `<`, `>`, `<=`, `>=` (IsLessThan abstract relational comparison)
- [ ] `==`, `!=` (IsLooselyEqual — coercion rules)
- [ ] `===`, `!==` (IsStrictlyEqual)
- [ ] `instanceof` (OrdinaryHasInstance, @@hasInstance)
- [ ] `in` (HasProperty, private `#field in obj`)

### 6.7 Logical & Short-Circuit Operators
- [ ] `&&` (short-circuit AND)
- [ ] `||` (short-circuit OR)
- [ ] `??` (nullish coalescing — only null/undefined)

### 6.8 Conditional Operator
- [ ] `? :` (ternary — ToBoolean condition)

### 6.9 Assignment
- [ ] Simple assignment `=` (PutValue)
- [ ] Compound assignment (`+=`, `-=`, etc.)
- [ ] Logical assignment (`&&=`, `||=`, `??=`)
- [ ] Destructuring assignment (array/object patterns)
  - [ ] Array destructuring: iterator protocol, rest element, defaults
  - [ ] Object destructuring: property access, rest properties, defaults

### 6.10 Comma Operator
- [ ] Evaluate left, discard result, evaluate right

### 6.11 Block Statement & Scoping
- [ ] Block: new declarative environment
- [ ] Block scoped declarations (`let`, `const`) — TDZ semantics
- [ ] Var hoisting within blocks

### 6.12 Variable Declarations
- [ ] `var` — hoist to function/global scope
- [ ] `let` — block scope, TDZ
- [ ] `const` — block scope, TDZ, immutable binding
- [ ] `using` — disposable resource management
- [ ] `await using` — async disposable
- [ ] Destructuring in declarations

### 6.13 Control Flow — Conditional
- [ ] `if` / `else`
- [ ] `switch` / `case` / `default` (strict equality matching)

### 6.14 Control Flow — Loops
- [ ] `while`
- [ ] `do`-`while`
- [ ] `for` (init/test/update with scoping)
- [ ] `for`-`in` (EnumerateObjectProperties)
- [ ] `for`-`of` (GetIterator, IteratorStep)
- [ ] `for await`-`of` (async iteration)
- [ ] Loop body scoping: per-iteration environment for `let`/`const`
- [ ] `break` / `continue` (with and without labels)
- [ ] Labelled statements

### 6.15 Control Flow — Exception Handling
- [ ] `throw` expression evaluation
- [ ] `try` / `catch` — catch binding, new environment
- [ ] `try` / `finally`
- [ ] `try` / `catch` / `finally`
- [ ] Catch parameter destructuring
- [ ] Optional catch binding

### 6.16 Other Statements
- [ ] `return` statement
- [ ] `with` statement (new object environment — sloppy mode only)
- [ ] `debugger` statement (no-op or host hook)
- [ ] Expression statement (evaluate, discard result)
- [ ] Empty statement

### 6.17 Function Calls
- [ ] Function call evaluation
- [ ] Argument list evaluation (spread in calls)
- [ ] `new` expression evaluation
- [ ] `super()` call in constructors
- [ ] `super.property` access
- [ ] Direct/indirect `eval()`
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
