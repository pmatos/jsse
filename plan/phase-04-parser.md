# Phase 4: Parser (AST)

**Spec Reference:** §13 (Expressions), §14 (Statements & Declarations), §15 (Functions & Classes), §16 (Scripts & Modules) — syntax only

## Goal
Build a complete recursive descent parser producing an AST from the token stream. Implement all Early Error rules.

## Tasks

### 4.1 AST Node Types
- [ ] Design AST node hierarchy
- [ ] Source location tracking (line, column, byte offset) for all nodes
- [ ] Implement `Display` / debug printing for AST

### 4.2 Primary Expressions (§13.2)
- [ ] `this`
- [ ] IdentifierReference
- [ ] Literal (null, boolean, numeric, string)
- [ ] ArrayLiteral (including elision and spread)
- [ ] ObjectLiteral (property definitions, computed properties, shorthand, spread)
- [ ] FunctionExpression
- [ ] ClassExpression
- [ ] GeneratorExpression
- [ ] AsyncFunctionExpression
- [ ] AsyncGeneratorExpression
- [ ] RegularExpressionLiteral
- [ ] TemplateLiteral
- [ ] CoverParenthesizedExpressionAndArrowParameterList (parenthesized / arrow params)

### 4.3 Left-Hand Side Expressions (§13.3)
- [ ] MemberExpression (dot, bracket, tagged template, `super` property, `import.meta`, `new` with args)
- [ ] `new` expression
- [ ] CallExpression (function call, `super()`, `import()`)
- [ ] OptionalExpression / OptionalChaining (`?.`)

### 4.4 Update Expressions (§13.4)
- [ ] Postfix `++` / `--`
- [ ] Prefix `++` / `--`

### 4.5 Unary Expressions (§13.5)
- [ ] `delete`
- [ ] `void`
- [ ] `typeof`
- [ ] Unary `+` / `-`
- [ ] Bitwise NOT `~`
- [ ] Logical NOT `!`
- [ ] `await`

### 4.6 Binary Expressions (§13.6–13.12)
- [ ] Exponentiation `**` (§13.6)
- [ ] Multiplicative `*`, `/`, `%` (§13.7)
- [ ] Additive `+`, `-` (§13.8)
- [ ] Shift `<<`, `>>`, `>>>` (§13.9)
- [ ] Relational `<`, `>`, `<=`, `>=`, `instanceof`, `in` (§13.10)
- [ ] Equality `==`, `!=`, `===`, `!==` (§13.11)
- [ ] Bitwise AND `&`, XOR `^`, OR `|` (§13.12)

### 4.7 Logical & Conditional Expressions (§13.13–13.14)
- [ ] Logical AND `&&` / OR `||` (§13.13)
- [ ] Nullish coalescing `??` (§13.13)
- [ ] Conditional (ternary) `? :` (§13.14)

### 4.8 Assignment Expressions (§13.15)
- [ ] Simple assignment `=`
- [ ] Compound assignment (`+=`, `-=`, `*=`, `/=`, `%=`, `**=`, `<<=`, `>>=`, `>>>=`, `&=`, `^=`, `|=`, `&&=`, `||=`, `??=`)
- [ ] Destructuring assignment (array / object patterns)
- [ ] AssignmentTargetType validation

### 4.9 Comma & Sequence Expressions (§13.16)
- [ ] Comma operator

### 4.10 Statements (§14)
- [ ] Block statement `{ ... }` (§14.2)
- [ ] Variable statement `var` (§14.3.2)
- [ ] Empty statement `;` (§14.4)
- [ ] Expression statement (§14.5)
- [ ] `if` / `else` (§14.6)
- [ ] Iteration statements (§14.7)
  - [ ] `do`-`while`
  - [ ] `while`
  - [ ] `for`
  - [ ] `for`-`in`
  - [ ] `for`-`of`
  - [ ] `for await`-`of`
- [ ] `continue` (§14.8)
- [ ] `break` (§14.9)
- [ ] `return` (§14.10)
- [ ] `with` (§14.11) — sloppy mode only
- [ ] `switch` (§14.12)
- [ ] Labelled statements (§14.13)
- [ ] `throw` (§14.14)
- [ ] `try` / `catch` / `finally` (§14.15)
- [ ] `debugger` (§14.16)

### 4.11 Declarations (§14.3)
- [ ] `let` declarations (§14.3.1)
- [ ] `const` declarations (§14.3.1)
- [ ] Binding patterns (array/object destructuring) (§14.3.3)
- [ ] `using` / `await using` declarations

### 4.12 Function Definitions (§15.2–15.8)
- [ ] FunctionDeclaration / FunctionExpression
- [ ] Arrow function `=>`
- [ ] GeneratorDeclaration / GeneratorExpression
- [ ] AsyncFunctionDeclaration / AsyncFunctionExpression
- [ ] AsyncGeneratorDeclaration / AsyncGeneratorExpression
- [ ] Method definitions (get/set/generator/async)
- [ ] Default parameters
- [ ] Rest parameters `...`

### 4.13 Class Definitions (§15.7)
- [ ] ClassDeclaration / ClassExpression
- [ ] `extends` clause
- [ ] ClassBody
- [ ] Method definitions (static, instance, computed)
- [ ] Field declarations (public, private `#`)
- [ ] Static initialization blocks
- [ ] Private names (`#name`)

### 4.14 Scripts & Modules (§16)
- [ ] Script goal parsing
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
- [ ] Duplicate formal parameters (strict mode)
- [ ] Use of reserved words
- [ ] Invalid assignment targets
- [ ] `with` in strict mode
- [ ] `delete` of unqualified identifier in strict mode
- [ ] Duplicate `__proto__` in object literals
- [ ] `yield` in generator parameters
- [ ] `await` in async function parameters
- [ ] Duplicate export names
- [ ] `new.target` outside functions
- [ ] `super` outside methods
- [ ] `return` outside functions
- [ ] `break`/`continue` label validation
- [ ] Lexical declarations in single-statement positions
- [ ] Labelled function declarations (strict mode restrictions)

### 4.16 Operator Precedence
- [ ] Correct precedence for all operators (comma < assignment < conditional < nullish < or < and < bitwise-or < bitwise-xor < bitwise-and < equality < relational < shift < additive < multiplicative < exponentiation < unary < update < LHS)

## test262 Tests
- All `language/expressions/` — 11,093 tests (syntax parsing subset)
- All `language/statements/` — 9,337 tests (syntax parsing subset)
- `language/computed-property-names/` — 48 tests
- `language/destructuring/` — 19 tests
- `language/rest-parameters/` — 11 tests
