# jsse

An agent-coded JS engine in Rust. I didn't touch a single line of code here. Not one. This repo is a write-only data store. I didn't even create this repo by hand -- my agent did that.

**Goal: 100% test262 pass rate.**

## Test262 Progress

| Test Files | Scenarios | Passing | Failing | Pass Rate |
|------------|-----------|---------|---------|-----------|
| 52,735     | 101,234   | 101,081 | 153     | 99.85%    |

This includes all test262 directories: `language/`, `built-ins/`, `annexB/`, `intl402/`, and `staging/`.

Per the test262 specification ([INTERPRETING.md](https://github.com/tc39/test262/blob/main/INTERPRETING.md)), test files without `noStrict`, `onlyStrict`, `module`, or `raw` flags must be run **twice**: once in default (sloppy) mode and once with `"use strict";` prepended. Our test runner implements this dual-mode execution, expanding 52,735 test files into 101,234 scenarios.

On the full suite: **101,081 / 101,234 (99.85%)**.

*ES Modules now supported with dynamic `import()` and `import.meta`. Async tests run with Promise/async-await support.*

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
  - Variable declarations (`var`, `let`, `const` with TDZ, `using`, `await using`)
  - Explicit Resource Management (`using`/`await using` declarations, `DisposableStack`, `AsyncDisposableStack`, `SuppressedError`, `Symbol.dispose`, `Symbol.asyncDispose`)
  - Control flow (`if`, `while`, `do-while`, `for`, `for-in`, `for-of`, `for-await-of`, `switch`, `try/catch/finally`)
  - Functions (declarations, expressions, arrows, closures)
  - Classes (declarations, expressions, inheritance, `super`, static methods/properties, private fields, private methods, private accessors, `#x in obj` brand checks)
  - Operators (arithmetic, comparison, bitwise, logical, assignment, update, typeof, void)
  - Objects and arrays (literals, member access, computed properties)
  - Template literals (including tagged templates)
  - `String.raw`
  - `new` operator with prototype chain setup
  - `new.target` meta-property
  - `this` binding (method calls, constructors, arrow lexical scoping)
  - Property descriptors (data properties with writable/enumerable/configurable)
  - Prototype chain inheritance (Object.prototype on all objects)
  - Getter/setter support (object literals, classes, `Object.defineProperty`)
  - `Object.defineProperty`, `Object.getOwnPropertyDescriptor`, `Object.getOwnPropertyDescriptors`, `Object.defineProperties`, `Object.keys`, `Object.freeze`, `Object.getPrototypeOf`, `Object.setPrototypeOf`, `Object.create`, `Object.entries`, `Object.values`, `Object.assign`, `Object.is`, `Object.getOwnPropertyNames`, `Object.getOwnPropertySymbols`, `Object.groupBy`, `Object.preventExtensions`, `Object.isExtensible`, `Object.isFrozen`, `Object.isSealed`, `Object.seal`, `Object.hasOwn`, `Object.fromEntries`
  - `Function.prototype.call`, `Function.prototype.apply`, `Function.prototype.bind`
  - `Object.prototype.hasOwnProperty`, `Object.prototype.toString`, `Object.prototype.valueOf`, `Object.prototype.propertyIsEnumerable`, `Object.prototype.isPrototypeOf`, `Object.prototype.__defineGetter__`, `Object.prototype.__defineSetter__`, `Object.prototype.__lookupGetter__`, `Object.prototype.__lookupSetter__`
  - Number/Boolean primitive method calls (`toString`, `valueOf`, `toFixed`)
  - `instanceof` and `in` operators
  - ToPrimitive with valueOf/toString coercion for objects
  - Wrapper objects (`new Boolean`, `new Number`, `new String`) with primitive value
  - `eval()` support
  - `Function`, `GeneratorFunction`, `AsyncFunction`, `AsyncGeneratorFunction` constructors (dynamic function creation)
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
  - `JSON.stringify` (replacer function/array, space/indent, toJSON, circular detection, BigInt TypeError, wrapper unwrapping), `JSON.parse` (reviver), `JSON.rawJSON`, `JSON.isRawJSON`
  - `String.fromCharCode`
  - `Map` built-in (constructor, `get`, `set`, `has`, `delete`, `clear`, `size`, `entries`, `keys`, `values`, `forEach`, `@@iterator`, `Map.groupBy`)
  - `Set` built-in (constructor, `add`, `has`, `delete`, `clear`, `size`, `entries`, `keys`, `values`, `forEach`, `@@iterator`, ES2025 set methods: `union`, `intersection`, `difference`, `symmetricDifference`, `isSubsetOf`, `isSupersetOf`, `isDisjointFrom`)
  - Generator functions (`function*`, `yield`, `yield*`) with replay-based execution
  - Generator prototype (`next`, `return`, `throw`, `Symbol.iterator`, `Symbol.toStringTag`)
  - ES Modules (`import`, `export`, `import()`, `import.meta`, top-level await)
  - `ArrayBuffer` (constructor, `byteLength`, `slice`, `isView`)
  - TypedArrays (`Int8Array`, `Uint8Array`, `Uint8ClampedArray`, `Int16Array`, `Uint16Array`, `Int32Array`, `Uint32Array`, `Float32Array`, `Float64Array`, `BigInt64Array`, `BigUint64Array`) with indexed access, prototype methods (`at`, `set`, `subarray`, `slice`, `copyWithin`, `fill`, `indexOf`, `lastIndexOf`, `includes`, `find`, `findIndex`, `findLast`, `findLastIndex`, `forEach`, `map`, `filter`, `reduce`, `reduceRight`, `every`, `some`, `reverse`, `sort`, `join`, `toString`, `toReversed`, `toSorted`, `entries`, `keys`, `values`, `from`, `of`)
  - `DataView` (constructor, all get/set methods for Int8 through BigUint64, little/big endian)
  - `Temporal` (all types: `Instant`, `ZonedDateTime`, `PlainDateTime`, `PlainDate`, `PlainTime`, `PlainYearMonth`, `PlainMonthDay`, `Duration`, `Temporal.Now`; full arithmetic, comparison, rounding, formatting; IANA timezone support via ICU4X; 12 calendar systems; 100% test262 + intl402/Temporal compliance)
  - `Intl.DateTimeFormat` (partial — dateStyle/timeStyle, calendar-aware Temporal formatting)
  - `Intl.NumberFormat` (partial)
  - `ShadowRealm` (constructor, `evaluate`, `importValue`; full cross-realm value wrapping with WrappedFunction exotic objects)
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

Options: `-j <n>` for parallelism (default: nproc/2), `--timeout <s>` (default: 120).

The runner supports multiple engines via `--engine`:

```bash
uv run python scripts/run-test262.py --engine jsse      # default
uv run python scripts/run-test262.py --engine node --binary /path/to/node
uv run python scripts/run-test262.py --engine boa --binary /path/to/boa
```

Per-test timing data is written to `/tmp/timing-{engine}.json` after each run.

## Engine Comparison (test262)

Benchmark run on 2026-03-10 using `scripts/run-test262.py -j 64 --timeout 120` on a 128-core machine. All three engines ran the same test262 checkout (commit at `test262/`) with the same harness, timeout, and scenario expansion.

| Engine | Version | Scenarios | Run | Skip | Pass | Fail | Rate |
|--------|---------|-----------|-----|------|------|------|------|
| **JSSE** | latest | 101,234 | 101,234 | 0 | 101,044 | 190 | **99.81%** |
| **Boa** | v0.21 | 91,986 | 91,986 | 0 | 83,260 | 8,726 | **90.51%** |
| **Node** | v25.8.0 | 91,986 | 91,187 | 799 | 79,201 | 11,986 | **86.86%** |

### Failure breakdown

| Category | Node v25.8.0 | Boa v0.21 |
|----------|-------------|-----------|
| Temporal | 8,980 | 0 |
| Atomics/agent | 236 | 0 |
| Dynamic import | 162 | 594 |
| RegExp property escapes | — | 284 |
| Class elements/destructuring | — | 960+ |
| AssignmentTargetType | — | 608 |

### Caveats

- **Node**: 75% of its failures are Temporal (not shipped in Node 25). Module tests (799 scenarios) are skipped — the adapter can't run ES modules via Node. Some Atomics agent tests time out due to the basic `$262.agent` prelude. Node runs under a 4GB `RLIMIT_AS` (V8 requires ~2GB to start).
- **Boa**: Main gaps are parser-level (assignment target validation, class destructuring, regexp property escapes). Temporal and Atomics tests pass (Boa handles them internally).
- **JSSE**: Full pass across all categories including Temporal, Atomics, modules, and staging tests.

### Reproducing the comparison

**Prerequisites:**
- Rust nightly toolchain
- Python 3.10+ with [uv](https://github.com/astral-sh/uv)
- `test262/` submodule initialized (`git submodule update --init`)

**1. Build JSSE:**
```bash
cargo build --release
```

**2. Download Node.js v25.8.0:**
```bash
curl -sL "https://nodejs.org/dist/v25.8.0/node-v25.8.0-linux-x64.tar.xz" \
  | tar -xJ -C /tmp/
# Binary: /tmp/node-v25.8.0-linux-x64/bin/node
```

**3. Download Boa v0.21:**
```bash
curl -sL "https://github.com/boa-dev/boa/releases/download/v0.21/boa-x86_64-unknown-linux-gnu" \
  -o /tmp/boa-v0.21
chmod +x /tmp/boa-v0.21
```

**4. Run each engine** (sequentially to avoid resource contention):
```bash
# JSSE (~15-20 min at -j 64)
uv run python scripts/run-test262.py -j 64

# Node v25.8.0 (~10 min at -j 64)
uv run python scripts/run-test262.py --engine node \
  --binary /tmp/node-v25.8.0-linux-x64/bin/node -j 64

# Boa v0.21 (~5 min at -j 64)
uv run python scripts/run-test262.py --engine boa \
  --binary /tmp/boa-v0.21 -j 64
```

Results are printed to stdout. Per-test timing JSON is written to `/tmp/timing-{engine}.json`. Pass/fail lists are written to `test262-pass-{engine}.txt` and `/tmp/test262-fail-{engine}.txt`.
