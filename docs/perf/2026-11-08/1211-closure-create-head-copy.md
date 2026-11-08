# #1211: hot-loop closure creation

`for (let i...) { const x = ...; const f = (z) => x + y + z; }` copied the loop head's environment on every iteration
(`CopyIterEnv`, 100,001 executions in the repro). The arrow's body-block environment has the head environment as its
parent, so the reference-count shortcut in the VM never fired.

## Change

`ForStatement::may_capture_iteration_scope` now asks which names the loop's nested functions and classes mention
(`CaptureWalk::nested_mentions_of`) instead of whether any exist. The per-iteration copy is kept when one of them names a
head binding (shadowing ignored) or the loop holds a direct `eval` or `with`. The tree-walker, bytecode compiler and
generator transform share the predicate.

## Measured (release-nolto, callgrind whole process, valgrind 3.x, repro of 100k iterations)

| binary | Ir | wall (pinned, 5 runs, median) |
|---|---:|---:|
| jsse base `220c3610` | 795,480,273 | 84 ms |
| jsse with change | 643,635,462 | 58 ms |
| boa HEAD `5a6281f7` | 636,423,061 | 51 ms |

Startup is 6.25 M Ir for jsse and 18.9 M for boa. Controls (Ir base -> new): `bench_closure_create` 2,488.6 M -> 2,489.7 M,
`bench_closures` 5,917 M -> 5,887 M, `bench_loop`, `bench_array`, `bench_object`, `bench_fib` within 0.05%.

For-in and for-of still use the coarse test.
