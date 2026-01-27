# jsse

An agent-coded JS engine in Rust. I didn't touch a single line of code here. Not one. This repo is a write-only data store. I didn't even create this repo by hand -- my agent did that.

**Goal: 100% test262 pass rate.**

## Test262 Progress

| Total Tests | Passing | Failing | Pass Rate |
|-------------|---------|---------|-----------|
| 48,257      | 4,566   | 43,691  | 9.46%     |

*Passing tests are negative tests (expected failures) since the engine is not yet implemented.*

## Structure

- `spec/` — ECMAScript specification (submodule from [tc39/ecma262](https://github.com/tc39/ecma262))
- `test262/` — Official test suite (submodule from [tc39/test262](https://github.com/tc39/test262))
- `tests/` — Additional custom tests
- `scripts/` — Test runner and tooling

## Supported Features

- CLI with file execution, `--eval`/`-e` inline evaluation, and REPL mode
- `--version` and `--help` flags
- Exit codes: 0 (success), 1 (runtime error), 2 (syntax error)

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
