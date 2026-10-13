# #960 — native try/catch/finally, for-in and for-of in the VM

Design: `docs/adr/2026-10-13-1019-bytecode-try-and-iteration.md`.

`before` is `main` at `2cddf07d`; `after` is this branch. Release builds,
`boa` is boa 0.22 (`~/.cargo/bin/boa`). Repros are
`docs/perf/2026-10-13/repros/bench_try_*.js`, `bench_for_of_*.js` and
`bench_for_in.js`; the JetStream scripts are the runner's concatenated driver
for one iteration of each benchmark.

The host is shared and heterogeneous (P and E cores, other jobs), and the
same binary swings by 30% between runs depending on the core it lands on. So:

- **Instruction counts** are the primary evidence. They are user-space
  retired instructions of the whole process (`perf_event_open`, pinned with
  `taskset -c 0`), deterministic to the million.
- **Wall times** are medians of 5 interleaved runs pinned to one idle P core
  (CPU 0), `before`, `after`, `after --no-bytecode` and `boa` in turn. Treat
  differences under ~5% as noise: richards moved +3% to -7% between runs of
  the same two binaries.

## Tree-walker work (perf-counters build, never timed; 1 iteration)

AST work = tree-walker `exec_statement`/`eval_expr` entries.

| benchmark | AST work before | after | VM ops before | after | fallback execs before | after |
|---|---:|---:|---:|---:|---:|---:|
| gbemu | 5,695,038 | 2,713 | 142,186,660 | 147,829,023 | 3,993 | 0 |
| UniPoker | 940,029 | 332 | 13,099,337 | 14,543,406 | 25,569 | 0 |
| Babylon | 46,251 | 43,787 | 10,114,072 | 10,117,418 | 627 | 524 |
| pdfjs | 698,946 | 2,287 | 43,556,391 | 44,461,625 | 337 | 288 |
| typescript-octane | 1,324,912 | 385,690 | 290,250,341 | 291,239,671 | 5,360 | 1,992 |
| splay / raytrace / cdjs / richards / delta-blue / crypto / Box2D | unchanged | | | | | |

`FALLBACK statement:Try` / `statement:ForIn` / `statement:ForOf` read 0 on
every JetStream driver (all 45 were scanned), and so does the new
`completion leaves a cleanup region` reason. What is left in the fallback
rows is `nested tail call`, `expression:Delete`, `expression:Template` and
`lexical binding pattern` (Babylon, pdfjs, typescript-octane), which are not
this issue.

## Instructions (M, user space, whole process)

| benchmark | before | after | change |
|---|---:|---:|---:|
| `bench_try_catch` | 14,542 | 5,162 | -64.5% |
| `bench_try_finally` | 16,298 | 7,269 | -55.4% |
| `bench_for_of_array` | 24,223 | 19,505 | -19.5% |
| `bench_for_in` | 10,879 | 7,219 | -33.6% |
| `bench_for_of_map_set` | 13,978 | 11,050 | -20.9% |
| gbemu | 46,593 | 43,820 | -5.9% |
| UniPoker | 6,167 | 5,793 | -6.1% |
| Babylon | 4,947 | 4,938 | -0.2% |
| pdfjs | 14,873 | 14,542 | -2.2% |
| typescript-octane | 142,679 | 142,762 | +0.1% |
| richards | 16,360 | 16,481 | +0.7% |
| delta-blue | 11,076 | 11,125 | +0.4% |
| raytrace | 23,754 | 23,884 | +0.5% |
| splay | 14,033 | 14,071 | +0.3% |
| crypto | 6,689 | 6,491 | -3.0% |
| navier-stokes | 5,018 | 4,839 | -3.6% |
| Box2D | 11,262 | 11,263 | +0.0% |
| cdjs | 35,809 | 35,859 | +0.1% |
| earley-boyer | 8,335 | 8,366 | +0.4% |
| hash-map | 25,441 | 25,454 | +0.1% |
| `bench_fib` | 8,384 | 8,423 | +0.5% |

The +0.3..0.7% on property-write-heavy code is the strict-rejection
diagnostic path (below), not the dispatch loop.

## Wall time (s, pinned to CPU 0, median of 5)

| benchmark | before | after | after `--no-bytecode` | boa 0.22 | after/before | after/boa |
|---|---:|---:|---:|---:|---:|---:|
| `bench_try_catch` (rare throw, 3M iterations) | 1.09 | 0.23 | 1.54 | 0.34 | 0.21 | 0.68 |
| `bench_try_finally` (continue + return through a finalizer) | 0.91 | 0.39 | 1.05 | 0.67 | 0.43 | 0.58 |
| `bench_for_of_array` (`const x of`, `const [k, v] of`) | 1.57 | 1.22 | 1.49 | 3.00 | 0.78 | 0.41 |
| `bench_for_in` (`var k in`, `let j in`) | 0.56 | 0.44 | 0.66 | 1.45 | 0.79 | 0.30 |
| `bench_for_of_map_set` (Map entries, Set with `break`) | 1.25 | 0.91 | 1.13 | 1.99 | 0.73 | 0.45 |
| JetStream gbemu | 2.61 | 2.58 | - | 2.64 | 0.99 | 0.98 |
| JetStream UniPoker | 0.47 | 0.47 | - | 0.44 | 1.00 | 1.08 |
| JetStream Babylon | 0.63 | 0.51 | - | 0.41 | 0.81 | 1.26 |
| JetStream pdfjs | 1.41 | 1.39 | - | 1.42 | 0.99 | 0.98 |
| JetStream typescript-octane | 17.64 | 17.79 | - | 8.72 | 1.01 | 2.04 |

The one-iteration JetStream drivers are dominated by setup, so the
tree-walker work removed above (5.7M units in gbemu) shows as a few percent;
the counters are the deterministic evidence.

## A call-path codegen fix that rode along

With the VM changes in place, `Interpreter::call_function_inner_impl`
executed +186 instructions per call (+7% on `bench_fib`, +2.5% on richards)
although none of its source had changed: LLVM scalarised the
`CallTarget::User` struct, six `bool` fields, into a pack/unpack sequence it
had previously copied as a block. Bisecting showed the extra instructions
did not come from the dispatch loop or the new arms (a build with all new
arms replaced by `unreachable!()` had them too) but from code layout luck in
that one function. `CallTarget::User` now carries the booleans as one flags
byte (`CALL_ARROW`, `CALL_STRICT`, ...), which makes the call path's
instruction count independent of it (`empty_calls` 2,984M -> 2,963M).

## Correctness gates

- Full `scripts/run-test262.py`: 99,911 / 99,911, 0 regressions.
- `test262-extra/` (710 files, 1,304 scenarios): 100% with and without
  `--no-bytecode`, on the plain release build and on `release-checked`, and
  under `JSSE_GC_STRESS=7` with and without `--no-bytecode`. (At
  `JSSE_GC_STRESS=1` two older files fail identically on `main`: the Set
  intersection and `using` loop-head tests.)
- The `language/` and `annexB/language/` trees re-run with every test body
  wrapped in an immediately invoked function, so that the bodies compile:
  the same 220 scenarios fail before and after (wrapping changes global
  semantics), also on `release-checked`; `built-ins/` and `intl402/` the
  same 193; `language/statements/{try,for-in,for-of}` wrapped pass 100% at
  `JSSE_GC_STRESS=7`.
- `cargo test` (debug, all targets) and `cargo test --release --features
  perf-counters`; `scripts/lint.sh`; `scripts/run-custom-tests.py` 18 / 18.

## Not done here

- `for await`, `using` heads, the Annex B for-in initializer, expression
  targets (`for (f() in o)`) and labelled `try` stay on the tree-walker.
- A statement the compiler cannot lower that completes with a
  `return`/`break`/`continue` out of a `try`/`finally` or a for-of keeps the
  whole construct on the tree-walker (`completion leaves a cleanup region`).
  None of the JetStream drivers hits it.
- The Script goal still bails on `try`, for-in and for-of.
