# JSSE Architecture Overview

A from-scratch JavaScript engine in Rust. No JS parser or engine libraries — every component is implemented from first principles.

## Pipeline

```
Source Code (.js / -e string / REPL)
        |
        v
    +-------+
    | Lexer |   src/lexer.rs  (~1,200 lines)
    +-------+
        |  Token stream
        v
    +--------+
    | Parser |  src/parser.rs  (~1,800 lines)
    +--------+
        |  AST (src/ast.rs, ~380 lines)
        v
    +-------------+
    | Interpreter |  src/interpreter.rs  (~6,200 lines)
    +-------------+
        |  JsValue results (src/types.rs, ~550 lines)
        v
    stdout / exit code
```

## Components

### Lexer (`src/lexer.rs`)

Converts source text into tokens. Handles all ES2024 token types:

- Identifiers and keywords (with Unicode support via `unicode-ident`)
- Numeric literals (decimal, hex, octal, binary, BigInt)
- String literals (single/double quotes, escape sequences)
- Template literals with substitution tracking
- RegExp literals
- All punctuators and operators
- Line terminator tracking (for ASI in parser)

### Parser (`src/parser.rs`)

Recursive descent parser producing an AST. Features:

- All statement types (variable declarations, control flow, try/catch, classes, functions)
- All expression types (binary, unary, assignment, member access, calls, templates)
- Destructuring patterns (array and object, with rest/defaults)
- Arrow functions, async, generators (syntax level)
- Automatic Semicolon Insertion (ASI)
- Single-token pushback for lookahead

### AST (`src/ast.rs`)

Pure data structures — no logic. Key types:

- `Program` → `Vec<Statement>`
- `Statement`: Block, Variable, If, While, DoWhile, For, ForIn, ForOf, Switch, Try, Return, Throw, Class, Function, Labeled, With
- `Expression`: Literal, Identifier, Binary, Unary, Assign, Call, New, Member, Arrow, Template, Spread, etc.
- `Pattern`: Identifier, Array, Object, Assign, Rest (for destructuring)
- Operator enums: BinaryOp, UnaryOp, LogicalOp, AssignOp, UpdateOp

### Types (`src/types.rs`)

JavaScript value system:

- `JsValue` enum: Undefined, Null, Boolean, Number, String, Symbol, BigInt, Object
- `JsString`: stores UTF-16 code units for spec-correct string indexing
- `JsSymbol`: unique ID + optional description, well-known symbols
- `JsBigInt`: wraps `num-bigint::BigInt`
- `JsObject`: ID reference into interpreter's object store
- Number/BigInt operation modules for arithmetic, bitwise, comparison

### Interpreter (`src/interpreter.rs`)

Tree-walking interpreter. The largest component. Key subsystems:

**Object Model**
- `JsObjectData`: properties (HashMap + insertion-order Vec), prototype chain, callable slot, array elements, class name, primitive value wrapper
- `PropertyDescriptor`: value/writable/enumerable/configurable + get/set for accessors
- Prototype chain lookup for property access
- `Object.defineProperty` with full descriptor validation

**Environment & Scoping**
- `Environment`: linked list of scopes (global → function → block)
- `Binding`: value + kind (Var/Let/Const) + initialized flag
- TDZ enforcement for let/const
- Var hoisting to function/global scope

**Execution**
- `Completion` enum: Normal, Return, Throw, Break, Continue
- Statement execution dispatches by statement type
- Expression evaluation with proper operator semantics
- ToPrimitive coercion (valueOf/toString) for operators
- Abstract equality and relational comparison per spec

**Built-in Objects**
- Object: create, defineProperty, keys, values, entries, freeze, seal, assign, is, hasOwn, fromEntries, getPrototypeOf, setPrototypeOf
- Function: call, apply, bind
- Array: push, pop, shift, unshift, map, filter, reduce, find, sort, flat, flatMap, splice, slice, concat, indexOf, includes, every, some, at, copyWithin, entries, keys, values
- String: charAt, indexOf, slice, substring, split, replace, trim, padStart, padEnd, repeat, startsWith, endsWith, match, search, codePointAt
- Number: isFinite, isNaN, isInteger, isSafeInteger, toFixed, toExponential, toPrecision
- Math: abs, floor, ceil, round, sqrt, pow, min, max, random, log, sin, cos, tan, atan2, hypot, etc.
- JSON: stringify, parse
- RegExp: test, exec (via Rust `regex` crate)
- Error types: Error, TypeError, ReferenceError, SyntaxError, RangeError
- Symbol: constructor + well-known symbols (iterator, hasInstance, toPrimitive, toStringTag)

### CLI (`src/main.rs`)

Entry point with three modes:
1. File execution: `jsse <file.js>`
2. Inline eval: `jsse -e "expression"`
3. REPL: `jsse` (no args)

Uses `clap` for argument parsing. Exit codes: 0 success, 1 runtime error, 2 syntax error.

## Testing

- **test262**: Primary validation. Run via `uv run python scripts/run-test262.py`. Parallel execution, YAML frontmatter parsing, negative test handling.
- **Custom tests**: `tests/` directory for cases not covered by test262.
- **Lint**: `scripts/lint.sh` for clippy + rustfmt.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing |
| `num-bigint` | BigInt arithmetic |
| `unicode-ident` | Unicode identifier validation |
| `regex` | RegExp implementation |
