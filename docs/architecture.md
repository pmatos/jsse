# JSSE Architecture Overview

JSSE is a from-scratch JavaScript engine in Rust. The execution model is a direct AST-walking interpreter: source text is tokenized, parsed into an AST, and evaluated without a bytecode or JIT layer.

## Pipeline

```text
Source (.js file / -e string / REPL)
        |
        v
    src/lexer.rs
        |
        v
    src/parser/
        |
        v
    src/ast.rs
        |
        v
    src/interpreter/
        |
        v
    stdout / exit code
```

## Main Components

### Lexer

`src/lexer.rs` converts source text into tokens, including:

- identifiers and keywords with Unicode support
- numeric, string, template, and RegExp literals
- punctuators and operators
- line terminator tracking for ASI-sensitive parsing

### Parser

The parser lives under `src/parser/` and is split by responsibility:

- `src/parser/mod.rs`: parser entrypoints and shared helpers
- `src/parser/expressions.rs`: expression parsing
- `src/parser/statements.rs`: statement parsing
- `src/parser/declarations.rs`: declarations, destructuring, and class/function parsing
- `src/parser/modules.rs`: module-specific syntax

The parser is recursive descent and produces the AST defined in `src/ast.rs`.

### AST and Runtime Values

`src/ast.rs` holds syntax tree data structures only.

Core JavaScript value types live in `src/types.rs`, including:

- `JsValue`
- `JsString`
- `JsSymbol`
- `JsBigInt`
- numeric and bigint operation helpers

### Interpreter

The interpreter lives under `src/interpreter/` and is split across runtime subsystems:

- `src/interpreter/mod.rs`: interpreter state, object store, module loading, runtime entrypoints
- `src/interpreter/exec.rs`: statement execution
- `src/interpreter/eval.rs`: expression evaluation
- `src/interpreter/types.rs`: runtime object, environment, and completion types
- `src/interpreter/helpers.rs`: coercion, equality, JSON, date, and other shared helpers
- `src/interpreter/gc.rs`: mark-and-sweep garbage collection
- `src/interpreter/builtins/`: built-in constructors, prototypes, and related runtime support

Key runtime responsibilities:

- object model and property descriptors
- environment chains and lexical scoping
- completion handling (`return`, `throw`, `break`, `continue`)
- built-ins and host/test262 support
- module loading and dynamic import
- typed arrays, buffers, and Atomics

## Built-ins Layout

`src/interpreter/builtins/mod.rs` wires globals and shared helpers together. Larger built-in families live in dedicated modules such as:

- arrays, strings, numbers, collections, iterators, promises, dates, regexp, typed arrays, atomics
- feature-specific support such as generators, disposable resources, and Temporal/Intl support where present in the tree

When adding or debugging a built-in, start in `builtins/mod.rs` to find the setup path, then move into the specialized module for the implementation details.

## Entry Points and Tooling

- `src/main.rs`: CLI entrypoint for file execution, `-e`, and REPL
- `scripts/run-test262.py`: primary conformance runner
- `scripts/run-custom-tests.py`: custom repo tests

## Validation Flow

The main validation target is test262. A typical workflow is:

1. build with `cargo build --release`
2. run targeted or full `test262` via `uv run python scripts/run-test262.py`
3. review the full-run results and update any published documentation you intend to keep current

Custom tests in `tests/` and the CI workflow complement test262, but test262 remains the primary correctness signal for language and built-in behavior.
