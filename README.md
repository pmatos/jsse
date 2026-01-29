# jsse

An agent-coded JS engine in Rust. I didn't touch a single line of code here. Not one. This repo is a write-only data store. I didn't even create this repo by hand -- my agent did that.

**Goal: 100% test262 pass rate.**

## Test262 Progress

| Total Tests | Run     | Skipped | Passing | Failing | Pass Rate |
|-------------|---------|---------|---------|---------|-----------|
| 48,257      | 42,076  | 6,181   | 13,284  | 28,792  | 31.57%    |

*Skipped: module and async tests (async parsing supported, runtime not yet implemented).*

## Structure

- `spec/` — ECMAScript specification (submodule from [tc39/ecma262](https://github.com/tc39/ecma262))
- `test262/` — Official test suite (submodule from [tc39/test262](https://github.com/tc39/test262))
- `tests/` — Additional custom tests
- `scripts/` — Test runner and tooling

## Supported Features

- CLI with file execution, `--eval`/`-e` inline evaluation, and REPL mode
- `--version` and `--help` flags
- Exit codes: 0 (success), 1 (runtime error), 2 (syntax error)
- Lexer: all ES2024 tokens, keywords, numeric/string/template literals, Unicode identifiers
- Parser: recursive descent, all statements, expressions, destructuring, arrow functions, classes, private fields, strict mode
- Interpreter: tree-walking execution with environment chain scoping
  - Variable declarations (`var`, `let`, `const` with TDZ)
  - Control flow (`if`, `while`, `do-while`, `for`, `for-in`, `for-of`, `switch`, `try/catch/finally`)
  - Functions (declarations, expressions, arrows, closures)
  - Classes (declarations, expressions, inheritance, `super`, static methods/properties, private fields)
  - Operators (arithmetic, comparison, bitwise, logical, assignment, update, typeof, void)
  - Objects and arrays (literals, member access, computed properties)
  - Template literals
  - `new` operator with prototype chain setup
  - `new.target` meta-property
  - `this` binding (method calls, constructors, arrow lexical scoping)
  - Property descriptors (data properties with writable/enumerable/configurable)
  - Prototype chain inheritance (Object.prototype on all objects)
  - Getter/setter support (object literals, classes, `Object.defineProperty`)
  - `Object.defineProperty`, `Object.getOwnPropertyDescriptor`, `Object.getOwnPropertyDescriptors`, `Object.defineProperties`, `Object.keys`, `Object.freeze`, `Object.getPrototypeOf`, `Object.setPrototypeOf`, `Object.create`, `Object.entries`, `Object.values`, `Object.assign`, `Object.is`, `Object.getOwnPropertyNames`, `Object.preventExtensions`, `Object.isExtensible`, `Object.isFrozen`, `Object.isSealed`, `Object.seal`, `Object.hasOwn`, `Object.fromEntries`
  - `Function.prototype.call`, `Function.prototype.apply`, `Function.prototype.bind`
  - `Object.prototype.hasOwnProperty`, `Object.prototype.toString`, `Object.prototype.valueOf`, `Object.prototype.propertyIsEnumerable`, `Object.prototype.isPrototypeOf`
  - Number/Boolean primitive method calls (`toString`, `valueOf`, `toFixed`)
  - `instanceof` and `in` operators
  - ToPrimitive with valueOf/toString coercion for objects
  - Wrapper objects (`new Boolean`, `new Number`, `new String`) with primitive value
  - `eval()` support
  - `Symbol` with well-known symbols (iterator, hasInstance, toPrimitive, etc.)
  - `delete` operator for object properties
  - Iterator protocol (`Symbol.iterator`, lazy `ArrayIterator`, `StringIterator`)
  - `Array.prototype.values()`, `.keys()`, `.entries()`, `[@@iterator]()` returning lazy iterators
  - `String.prototype[@@iterator]()` with Unicode code point iteration
  - Spread elements in arrays and function calls (iterator-protocol aware)
  - Rest parameters in function declarations
  - Destructuring (array and object patterns, iterator-protocol aware)
  - RegExp literals and `RegExp` constructor with `test`, `exec`, `toString`
  - Array prototype methods (`push`, `pop`, `shift`, `unshift`, `indexOf`, `lastIndexOf`, `includes`, `join`, `toString`, `concat`, `slice`, `splice`, `reverse`, `fill`, `forEach`, `map`, `filter`, `reduce`, `reduceRight`, `some`, `every`, `find`, `findIndex`, `findLast`, `findLastIndex`, `sort`, `flat`, `flatMap`, `at`)
  - `Array.isArray`, `Array.from`, `Array.of`
  - String prototype methods (`charAt`, `charCodeAt`, `indexOf`, `lastIndexOf`, `includes`, `startsWith`, `endsWith`, `slice`, `substring`, `toLowerCase`, `toUpperCase`, `trim`, `trimStart`, `trimEnd`, `repeat`, `padStart`, `padEnd`, `concat`, `split`, `replace`, `replaceAll`, `at`, `search`, `match`)
  - Proper Error objects (`TypeError`, `ReferenceError`, `SyntaxError`, `RangeError`) with prototype chains and `instanceof` support
  - `JSON.stringify`, `JSON.parse`
  - `String.fromCharCode`
  - `Map` built-in (constructor, `get`, `set`, `has`, `delete`, `clear`, `size`, `entries`, `keys`, `values`, `forEach`, `@@iterator`)
  - `Set` built-in (constructor, `add`, `has`, `delete`, `clear`, `size`, `entries`, `keys`, `values`, `forEach`, `@@iterator`, ES2025 set methods: `union`, `intersection`, `difference`, `symmetricDifference`, `isSubsetOf`, `isSupersetOf`, `isDisjointFrom`)
  - `globalThis`
  - Built-ins: `console.log`, `Error`, `Test262Error`, `$DONOTEVALUATE$`

## Building & Running

```bash
cargo build --release
./target/release/jsse <file.js>
./target/release/jsse -e "1 + 1"
./target/release/jsse              # starts REPL
```

## Running test262

```bash
cargo build --release
uv run python scripts/run-test262.py
```

Options: `-j <n>` for parallelism (default: nproc), `--timeout <s>` (default: 60).
