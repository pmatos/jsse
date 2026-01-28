# Phase 4: Parser (AST)

**Spec Reference:** §13 (Expressions), §14 (Statements & Declarations), §15 (Functions & Classes), §16 (Scripts & Modules) — syntax only

## Goal
Build a complete recursive descent parser producing an AST from the token stream. Implement all Early Error rules.

## Tasks

### 4.1 AST Node Types
- [x] Design AST node hierarchy
- [ ] Source location tracking (line, column, byte offset) for all nodes
- [ ] Implement `Display` / debug printing for AST

### 4.2 Primary Expressions (§13.2)
- [x] `this`
- [x] IdentifierReference
- [x] Literal (null, boolean, numeric, string)
- [x] ArrayLiteral (including elision and spread)
- [x] ObjectLiteral (property definitions, computed properties, shorthand, spread)
- [x] FunctionExpression
- [x] ClassExpression
- [x] GeneratorExpression
- [ ] AsyncFunctionExpression
- [ ] AsyncGeneratorExpression
- [x] RegularExpressionLiteral
- [x] TemplateLiteral
- [x] CoverParenthesizedExpressionAndArrowParameterList (parenthesized / arrow params)

### 4.3 Left-Hand Side Expressions (§13.3)
- [x] MemberExpression (dot, bracket, tagged template, `super` property, `new` with args)
- [ ] `import.meta`
- [x] `new` expression
- [x] CallExpression (function call, `super()`)
- [ ] `import()` dynamic import
- [x] OptionalExpression / OptionalChaining (`?.`)

### 4.4 Update Expressions (§13.4)
- [x] Postfix `++` / `--`
- [x] Prefix `++` / `--`

### 4.5 Unary Expressions (§13.5)
- [x] `delete`
- [x] `void`
- [x] `typeof`
- [x] Unary `+` / `-`
- [x] Bitwise NOT `~`
- [x] Logical NOT `!`
- [ ] `await`

### 4.6 Binary Expressions (§13.6–13.12)
- [x] Exponentiation `**` (§13.6)
- [x] Multiplicative `*`, `/`, `%` (§13.7)
- [x] Additive `+`, `-` (§13.8)
- [x] Shift `<<`, `>>`, `>>>` (§13.9)
- [x] Relational `<`, `>`, `<=`, `>=`, `instanceof`, `in` (§13.10)
- [x] Equality `==`, `!=`, `===`, `!==` (§13.11)
- [x] Bitwise AND `&`, XOR `^`, OR `|` (§13.12)

### 4.7 Logical & Conditional Expressions (§13.13–13.14)
- [x] Logical AND `&&` / OR `||` (§13.13)
- [x] Nullish coalescing `??` (§13.13)
- [x] Conditional (ternary) `? :` (§13.14)

### 4.8 Assignment Expressions (§13.15)
- [x] Simple assignment `=`
- [x] Compound assignment (`+=`, `-=`, `*=`, `/=`, `%=`, `**=`, `<<=`, `>>=`, `>>>=`, `&=`, `^=`, `|=`, `&&=`, `||=`, `??=`)
- [x] Destructuring assignment (array / object patterns)
- [x] AssignmentTargetType validation

### 4.9 Comma & Sequence Expressions (§13.16)
- [x] Comma operator

### 4.10 Statements (§14)
- [x] Block statement `{ ... }` (§14.2)
- [x] Variable statement `var` (§14.3.2)
- [x] Empty statement `;` (§14.4)
- [x] Expression statement (§14.5)
- [x] `if` / `else` (§14.6)
- [x] Iteration statements (§14.7)
  - [x] `do`-`while`
  - [x] `while`
  - [x] `for`
  - [x] `for`-`in`
  - [x] `for`-`of`
  - [ ] `for await`-`of`
- [x] `continue` (§14.8)
- [x] `break` (§14.9)
- [x] `return` (§14.10)
- [x] `with` (§14.11) — sloppy mode only
- [x] `switch` (§14.12)
- [x] Labelled statements (§14.13)
- [x] `throw` (§14.14)
- [x] `try` / `catch` / `finally` (§14.15)
- [x] `debugger` (§14.16)

### 4.11 Declarations (§14.3)
- [x] `let` declarations (§14.3.1)
- [x] `const` declarations (§14.3.1)
- [x] Binding patterns (array/object destructuring) (§14.3.3)
- [ ] `using` / `await using` declarations

### 4.12 Function Definitions (§15.2–15.8)
- [x] FunctionDeclaration / FunctionExpression
- [x] Arrow function `=>`
- [x] GeneratorDeclaration / GeneratorExpression
- [ ] AsyncFunctionDeclaration / AsyncFunctionExpression
- [ ] AsyncGeneratorDeclaration / AsyncGeneratorExpression
- [x] Method definitions (get/set/generator/async)
- [x] Default parameters
- [x] Rest parameters `...`

### 4.13 Class Definitions (§15.7)
- [x] ClassDeclaration / ClassExpression
- [x] `extends` clause
- [x] ClassBody
- [x] Method definitions (static, instance, computed)
- [x] Field declarations (public)
- [ ] Field declarations (private `#`)
- [x] Static initialization blocks
- [ ] Private names (`#name`)

### 4.14 Scripts & Modules (§16)
- [x] Script goal parsing
- [ ] Module goal parsing
- [ ] `import` declarations (§16.2.2)
  - [ ] Named imports
  - [ ] Default import
  - [ ] Namespace import `* as`
  - [ ] Import attributes (`with { type: ... }`)
- [ ] `export` declarations (§16.2.3)
  - [ ] Named exports
  - [ ] Default export
  - [ ] Re-exports (`export ... from`)
  - [ ] `export * as`

### 4.15 Early Errors
- [x] Duplicate formal parameters (strict mode)
- [x] Use of reserved words
- [x] Invalid assignment targets
- [x] `with` in strict mode
- [x] `delete` of unqualified identifier in strict mode
- [x] Duplicate `__proto__` in object literals
- [ ] `yield` in generator parameters
- [ ] `await` in async function parameters
- [ ] Duplicate export names
- [x] `new.target` outside functions
- [x] `super` outside methods
- [x] `return` outside functions
- [x] `break`/`continue` label validation
- [x] Lexical declarations in single-statement positions
- [ ] Labelled function declarations (strict mode restrictions)

### 4.16 Operator Precedence
- [x] Correct precedence for all operators

## test262 Tests
- All `language/expressions/` — 11,093 tests (syntax parsing subset)
- All `language/statements/` — 9,337 tests (syntax parsing subset)
- `language/computed-property-names/` — 48 tests
- `language/destructuring/` — 19 tests
- `language/rest-parameters/` — 11 tests
