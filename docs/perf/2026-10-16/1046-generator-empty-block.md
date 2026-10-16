# #1046: lower declaration-free blocks in generators in place

A `Statement::Block` with no `let`/`const`/`using`/class/function declaration, lowered while no `with` chain is
active, no longer opens a state-machine scope: no `entry_state`, no `ScopeAction::OpenBlock`, no `inner_after`
join state, no run-time `Environment`. The predicate is `hoisting::block_declares_lexical`, shared with the
bytecode compiler (`compile_scoped_block`).

Measured on buildbox3 (shipped profile, fat LTO, `line-tables-only` + frame pointers, nightly), base = `main`
at `2a86956e`, new = the same tree plus the change. Instruction counts are deterministic.

| metric | base | new | delta |
|---|---:|---:|---:|
| lazy-collections total Ir (callgrind) | 10,837,361,513 | 9,225,213,063 | -14.9% |
| lazy-collections wall (hyperfine, 7 runs, load ~2) | 2.732 s | 2.297 s | -15.9% |
| `body_non_function_execs` (perf-counters) | 5,073,196 | 3,516,870 | -30.7% |
| `BODY range` state steps, lazy-collections | 3,446,237 | 2,304,388 | -33.1% |
| `ast_work_units`, `vm_ops`, GC collections | unchanged | unchanged | 0 |
| repro N=200k, `BODY range` steps | 1,200,004 | 800,004 | 6 -> 4 per yield |
| repro Ir per yield, braces | 10,231 | 8,601 | -15.9% |
| repro Ir per yield, no braces (control) | 8,601 | 8,601 | braces == control |

## Kept on the scoped path

A block with any lexical or function declaration (labelled functions included), any block lowered under a
`with` chain, and `using`/`await using` blocks. Guard cases for these live in
`test262-extra/generator-declaration-free-block-*.js` and
`test262-extra/async-declaration-free-block-across-await.js`.

## Not done

The try, `finally` and `switch` case `OpenBlock` sites and the tree-walker `Statement::Block` arm still allocate
an environment for a declaration-free body (see the issue's follow-ups).
