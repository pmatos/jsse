# `execute_ast` fuzz target: arbitrary-driven JS AST generator

Status: accepted (#1175, part of #1168); extends ADR 2026-10-23-1400.

## Context

`execute` mutates raw bytes, so most mutants die in the lexer or parser and the
interpreter, bytecode VM and built-ins see far fewer distinct programs than the
executions suggest. A structure-aware generator keeps every input a program.

## Decision

- Types, not text: `fuzz/src/js_ast.rs` is a compact AST deriving
  `arbitrary::Arbitrary` (about 20 statement and 45 expression forms:
  destructuring, classes with private members and static blocks, generators,
  async functions, `for await`, labels, `with`, optional chains, templates,
  spread, `eval` of pooled strings). It lives in the `fuzz` crate, not the
  engine: it needs no engine internals, and `src/lib.rs` is near its token cap.
  `fuzz/src/js_print.rs`, `js_print_class.rs` and `js_print_expr.rs` render it.
- Termination: every recursive enum lists a leaf variant first. `Unstructured`
  yields the lowest choice when its bytes run out, so exhaustion ends the
  recursion; the printer also caps depth (`MAX_DEPTH`) and source size.
  `worst_case_input_does_not_overflow_the_stack` pins this on an 8 MiB stack.
- Validity by construction, not by retrying: identifiers, properties, strings,
  numbers and regexes are indexes into fixed pools (`js_pools.rs`), so a byte
  never forms an invalid name. The printer tracks where `break`, `continue`,
  `return`, `yield`, `await`, `super`, `new.target`, `arguments`, labels and
  private names are legal and prints a harmless expression instead where they
  are not. `var`, parameter, catch and lexical (`n0`..`n5`) names come from
  disjoint pools and a lexical name is declared once per scope, so no
  redeclaration error is possible. Every operand is parenthesised and every
  statement-position expression is wrapped, which removes precedence, ASI and
  `{`/`function`/`class` statement-start hazards. Targets of assignment, update
  and `delete` are a restricted `Target`, so `f() = 1` is unrepresentable.
- Loop fuel: every loop body starts with `if ($g++ > 48) break;` on one shared
  counter. Without it `while ({}) {}` burns the whole 20,000-step budget twice
  (about 1 s per input with debug assertions) and throughput fell to about 2
  executions per second; with it, 30 to 40. Total iterations per program are
  bounded, which is enough to execute loop bodies and closures over
  per-iteration bindings.
- The pools hold none of the identifiers `execute` skips or does not compare
  (`agent`, `Date`, `$262`, `gc`, ...), checked by
  `pools_contain_no_runner_marker_identifiers`; `"use strict"` is kept out of
  the string pool so a pooled string never becomes a directive.
- The target prints the program and calls `jsse::fuzz_execute_source`, the
  body that `fuzz_execute_bytes` now also calls, so the Execution Divergence
  check is the same code, not a copy. Scripts only, as in `execute`.
- ClusterFuzzLite: `build.sh` builds `execute_ast` beside the others with
  `max_len = 4096` (`execute_ast.options`) and a six-file seed corpus in
  `fuzz/corpus/execute_ast/` (random `Arbitrary` buffers whose programs cover
  classes, generators, async, loops, labels, `try` and `switch`). The workflows
  are unchanged; `fuzz-seconds` is split between four targets now.

## Measurements

Local, `cargo +nightly fuzz build -O --debug-assertions` (no ASan), `-seed=1`,
four runs sharing a loaded 24-core host, so absolute rates are noisy.
`cov:` is libFuzzer's edge count over the whole binary; all four targets link
the same engine, so it is comparable.

Parse rate (`printed_programs_parse_in_the_engine`, 2,000 pseudo-random
buffers of 16 to 2,048 bytes through `Parser`): **99.9%** (1998/2000; 19,976 of
20,000 before loop fuel). Failures were an engine bug (below) and a
`super()` call in a derived constructor's parameter default, which the printer
now suppresses. Mean printed source: 157 bytes.

| Budget | `execute` (54 seeds) | `execute_ast` (6 seeds) | `execute` (empty) | `execute_ast` (empty) |
| --- | --- | --- | --- | --- |
| 10,000 executions, `cov` | 26,957 | **29,450** | 9,847 | **15,249** |
| 300 s wall, `cov` | **27,992** | 27,043 | 11,788 | **15,270** |
| 300 s wall, executions | 46,740 | 9,865 | 66,072 | 11,549 |

Per execution, the generator wins: +9% edges with seeds and +55% without. Per
wall-clock second the seeded `execute` is 3% ahead after five minutes, because
a valid program costs two full interpreter setups (about 30 ms each with debug
assertions) while a raw mutant mostly fails fast in the parser, so `execute`
runs about five times as many inputs. Its seeds are also 54 full test262 files
with the harness inlined; the generator needs none. `execute_ast` was still
gaining edges when the run ended (corpus 1,826 units). Edge counts were used
instead of `llvm-cov` line coverage, which needs a separate instrumented build
of each target; the numbers above are not a line-coverage claim.

## Consequences

- The `ClusterFuzzLite PR` check now splits its 120 s three ways and any latent
  engine bug the generator reaches can fail an unrelated PR. A 5-minute local
  run at CI flags found no panic or divergence before this landed.
- Found while measuring (filed, not fixed here): #1192, a nested class inside
  a private method cannot reference the enclosing private name
  (`class A { #q(){ class B { m(){ this.#q } } } }`). Those programs are a
  `SyntaxError` in both modes, so they agree and are harmless to the check.
- Rejected: putting the generator in the library crate (exposes nothing it
  needs, bloats `src/lib.rs`); a custom libFuzzer mutator over source (more
  machinery than `Arbitrary` for the same effect); `node` as the oracle (wrong
  oracle, see ADR 2026-10-23-1400); retrying until a program parses (wastes
  executions and biases toward trivial programs).
- Follow-ups: module-goal programs, weighting or dictionaries for the pools,
  async-ordering programs with timers, an `Arbitrary` seed expander over
  test262, cheaper realm setup so valid programs cost less than 30 ms.
