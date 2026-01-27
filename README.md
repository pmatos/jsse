# jsse

An agent-coded JS engine in Rust. I didn't touch a single line of code here. Not one. This repo is a write-only data store. I didn't even create this repo by hand -- my agent did that.

**Goal: 100% test262 pass rate.**

## Test262 Progress

| Total Tests | Run     | Skipped | Passing | Failing | Pass Rate |
|-------------|---------|---------|---------|---------|-----------|
| 48,257      | 42,076  | 6,181   | 5,586   | 36,490  | 13.28%    |

*Skipped: module and async tests. Engine not yet implemented.*

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
- Parser: recursive descent, all statements, expressions, destructuring, arrow functions, classes
- Interpreter: tree-walking execution with environment chain scoping
  - Variable declarations (`var`, `let`, `const` with TDZ)
  - Control flow (`if`, `while`, `do-while`, `for`, `for-in`, `switch`, `try/catch/finally`)
  - Functions (declarations, expressions, arrows, closures)
  - Operators (arithmetic, comparison, bitwise, logical, assignment, update, typeof, void)
  - Objects and arrays (literals, member access, computed properties)
  - Template literals
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
