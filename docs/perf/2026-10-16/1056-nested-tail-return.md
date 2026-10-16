# #1056: lower nested tail-call returns natively

`return a && f(x)`, `return c ? f(x) : g(x)`, `return a, f(x)` and `return a ?? f(x)` were handed to the tree-walker
(`Unsupported("nested tail call")`) whenever `tail_call_allowed()` held, and a body that was only such a return ran
wholly on the tree-walker on every call (`BAIL fallback-only body`). The compiler cannot see strictness (the VM checks
`frame_env.strict` when it meets `ReturnCall`), so sloppy code paid for a strict-mode-only concern too.

`compile_tail_return` now lowers the expression with one return per tail leaf: a `Call` leaf is `ReturnCall` +
`Return`, `?:` branches via `compile_cond_jump`, `&&`/`||`/`??` via the existing `*Keep` jump to a `Return` of the
kept value, a comma/sequence compiles its prefix with `compile_effect` and recurses into the last operand, and any
other expression is `compile_expr` + `Return`. A leaf `compile_call` rejects (spread argument, `super`/optional-chain
callee) or a tagged-template leaf still returns `Unsupported`, so the statement rolls back to `ExecAst` and keeps its
tree-walker tail call. Only entered when `tail_call_allowed() && contains_tail_call(expr)`. No VM, GC or `ExecAst`
change, and no new operand roots.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`. base = `origin/main` at `00bb965d` (binary sha256 `3e0cee1b146a...`),
new = base plus this change (`f402664851b8...`). JetStream drivers at `--iterations 1`.

| benchmark | base Ir | new Ir | delta | criterion |
|---|---:|---:|---:|---|
| proxy-vue (median of 5) | 1,631,417,379 | 1,522,999,254 | -6.65% | >= 2.5% |
| proxy-mobx (median of 20) | 852,896,414 | 822,003,922 | -3.62% | >= 3.5% |
| richards | 14,391,083,819 | 14,391,110,937 | +0.00% | within 0.5% |
| Box2D | 8,628,393,744 | 8,626,669,914 | -0.02% | within 0.5% |
| fib (micro) | 8,394,878,255 | 8,394,877,434 | -0.00% | within 0.5% |
| bigint-noble-ed25519 | 8,135,500,114 | 8,145,133,375 | +0.12% | within 0.5% |
| Babylon | 3,587,894,105 | 3,583,234,778 | -0.13% | |
| bigint-noble-bls12-381 | 76,099,209,791 | 76,054,778,989 | -0.06% | |

proxy-mobx's total Ir is not run-to-run stable (randomised interner: base 840M-876M, new 814M-848M over 20 runs
each), so only medians mean anything. The 3.5% criterion is met by the median (-3.62%) and the LTO-off build (-3.89%);
the mean delta is -3.47% and the best-case pair -3.1%, so read it as roughly -3.5%.

The LTO-off perf-counters builds (`release-nolto`, the issue's other recipe), medians of 8 cachegrind runs: proxy-mobx
914.9M -> 879.3M (-3.89%), proxy-vue 1,724.7M -> 1,626.7M (-5.68%).

Counters (same perf-counters build):

| benchmark | `ast_work_units` base -> new | `body_dispatch_ast` | `BAIL fallback-only body` | `FALLBACK nested tail call` |
|---|---:|---:|---:|---:|
| proxy-mobx | 244,925 -> 15,514 (-93.7%) | 20,441 -> 737 | 18 -> 2 | 9 -> 0 |
| proxy-vue | 713,596 -> 26,092 (-96.3%) | 15,671 -> 735 | 7 -> 2 | 7 -> 0 |

Reproducer (the issue's `nested-tail-return.js`, strict, 4 functions x 20,000 iterations; `FLAT` is the same logic with
`if`/`return`), cachegrind total Ir with the shipped binaries:

| | nested | flat |
|---|---:|---:|
| base | 659,200,356 | 597,653,078 |
| new | 594,796,352 | 598,489,170 |

Nested went from +10.3% over flat to -0.6% (criterion: within 5%). Counters: `ast_work_units` 520,026 -> 26,
`FALLBACK nested tail call` and `BAIL fallback-only body` absent, `vm_ops` 1,770,043 identical to the flat run.

## Correctness gates

`test262-extra/nested-tail-call-positions.js` (strict: deep recursion through every nested shape, spread and
tagged-template leaves, short-circuit values, evaluation order, `this`, `try`/`finally`, `new`/derived constructors)
and `nested-tail-call-positions-sloppy.js` (same results outside strict mode, for-of iterator close) pass on the
baseline binary and on the new one, in both modes. `bytecode/tests.rs` asserts that each nested shape lowers to
`Op::ReturnCall` leaves with no `ExecAst`, and that spread/tagged-template leaves keep the tree-walker.

Found while writing the for-of coverage: a strict `return f()` inside a for-of body skips IteratorClose in the
tree-walker (#1084); the strict test therefore leaves for-of to the sloppy file.
