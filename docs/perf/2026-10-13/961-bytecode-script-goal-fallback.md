# #961 — per-node tree-walker fallback and valued completions in the Script goal

Design: `docs/adr/2026-10-13-1210-bytecode-script-goal-fallback.md`.

`before` is `main` at `bdbf4f15`; `after` is this branch. Release builds,
`boa` is boa 0.22 (`~/.cargo/bin/boa`). Repros are
`docs/perf/2026-10-13/repros/bench_script_*.js`; the JetStream scripts are the
runner's concatenated driver for one iteration of each benchmark (all 45
runnable drivers were scanned with the `perf-counters` build).

The host was heavily loaded while measuring (load average 38-42 on 24
cores), so wall times are medians of 5 interleaved runs pinned to one core
(`taskset -c 3`), `before`, `after`, `after --no-bytecode` and `boa` in turn;
treat differences under ~10% as noise. The deterministic evidence is the
counters build.

## Acceptance: no `BAIL` for `<script body>`

| | before | after |
|---|---:|---:|
| JetStream drivers whose `<script body>` bails | 45 / 45 | 0 / 45 |
| bail reason | `lexical declaration` (function declaration, class, global `let`/`const`) | none |

`compile_bail` fell by exactly one on every driver. Eleven drivers also hit
`construct needs tree-walked frame` on their script with the first version of
this change: the async harness is `(async () => { ... await ... })()`, and the
shared poison traversal enters arrow bodies. The Script goal now ignores
`await`/`yield`/`new.target`/`super`/private `in` (see the ADR).

What the script hands to the tree-walker is now visible: `FALLBACK` rows
count a compiled script's hand-offs (`statement:ClassDeclaration`,
`lexical declaration`, ...), and the tree-walker work they spend is attributed
to the `<script body>` row.

## Tree-walker work (perf-counters build, never timed; 1 iteration)

AST work = tree-walker `exec_statement`/`eval_expr` entries. Drivers that
move by less than 100 units are omitted; none moves up beyond the
benchmarks' own run-to-run variation (proxy-vue draws random data).

| benchmark | AST work before | after |
|---|---:|---:|
| proxy-mobx | 257,436 | 244,991 |
| mandreel | 841,953 | 834,227 |
| earley-boyer | 154,928 | 148,268 |
| crypto | 14,995 | 10,172 |
| Babylon | 43,787 | 41,283 |
| gbemu | 2,713 | 2 |
| delta-blue | 466 | 1 |
| richards | 274 | 1 |
| cdjs | 227 | 1 |
| splay | 167 | 1 |
| UniPoker | 332 | 104 |
| Box2D | 4,447 | 3,978 |

The benchmarks' hot loops live in functions, which #902-#960 already
compile, so the JetStream drivers gain little wall time: the remaining top-level
tree-walker work was a few thousand units in the worst case.

## Wall time (s, pinned, median of 5; wall / user+sys)

| benchmark | before | after | after `--no-bytecode` | boa 0.22 | after/before |
|---|---:|---:|---:|---:|---:|
| `bench_script_loop_decl` (function + class, then a 3M-iteration global loop) | 2.25 | 1.00 | 2.25 | 2.02 | 0.44 |
| `bench_script_let_loop` (`for (let ...)` with a block `let`, global `const`) | 0.88 | 0.28 | 1.11 | 0.18 | 0.32 |
| `bench_script_handoff_loop` (labelled block, try/finally, switch in the loop) | 1.19 | 1.01 | 1.23 | 1.21 | 0.85 |
| `bench_loop` | 0.22 | 0.13 | 0.37 | 0.51 | 0.59 |
| `bench_fib` | 0.41 | 0.41 | 0.62 | 0.39 | 1.00 |
| `bench_opmix` | 3.59 | 3.06 | 15.36 | 3.31 | 0.85 |
| JetStream richards | 0.90 | 0.88 | 1.95 | 0.77 | 0.98 |
| JetStream delta-blue | 0.62 | 0.63 | 1.23 | 0.62 | 1.02 |
| JetStream raytrace | 1.66 | 1.61 | 2.50 | 1.64 | 0.97 |
| JetStream splay | 2.14 | 2.12 | 3.15 | 1.26 | 0.99 |
| JetStream crypto | 0.28 | 0.28 | 1.18 | 0.48 | 1.00 |
| JetStream navier-stokes | 0.21 | 0.20 | 1.04 | 0.34 | 0.95 |
| JetStream Box2D | 0.73 | 0.70 | 2.04 | 0.59 | 0.96 |
| JetStream UniPoker | 0.42 | 0.37 | 0.55 | 0.42 | 0.88 |
| JetStream Babylon | 0.47 | 0.38 | 0.53 | 0.38 | 0.81 |
| JetStream gbemu | 2.19 | 2.10 | 5.62 | 2.45 | 0.96 |
| JetStream earley-boyer | 0.71 | 0.69 | 1.18 | 0.70 | 0.97 |
| JetStream cdjs | 2.82 | 2.59 | 4.03 | 3.69 | 0.92 |
| JetStream hash-map | 1.59 | 1.56 | 2.75 | 1.20 | 0.98 |

`bench_script_let_loop` is still behind boa: the loop is native but each
iteration enters a block environment.

## A latent bug the Script goal exposed

Compiling every script puts all of test262 on the VM, which found a bug
already present for function bodies: `member_get`/`member_get_computed`
converted a primitive base with `ToObject` and passed the wrapper as the
[[Get]] receiver, so a strict getter inherited by a primitive saw a boxed
`this` (`language/function-code/10.4.3-1-104`, `-106`). They now go through
`access_property_on_value`, which keeps the primitive, after a direct
object-base fast path (the first attempt without it cost +20% on
richards/Box2D/gbemu through `to_js_property_key`). Test:
`test262-extra/bytecode-primitive-base-getter-receiver.js`.

The shifted safepoints also uncovered a rooting hole in
`Set.prototype.intersection` that fails on `main` under
`JSSE_GC_STRESS=1`: the entry passed to `has` was held only by a Rust local,
and a strict callee with no parameters does not hold its argument either, so a
`has` that removes the entry from `O` and collects lost it. Test:
`test262-extra/Set-prototype-intersection-has-removes-current-entry-gc-rooting.js`.

## Correctness gates

- Full `scripts/run-test262.py`: 99,911 / 99,911, 0 regressions.
- `test262-extra/` (713 files, 1,310 scenarios): 100% with and without
  `--no-bytecode`, on the plain release build and on `release-checked`, and
  under `JSSE_GC_STRESS=7` with and without `--no-bytecode`. A 10% sample of
  test262 on `release-checked` and a 2% sample under `JSSE_GC_STRESS=16` pass.
- `cargo test` (debug) and `cargo test --release --features perf-counters`;
  `scripts/lint.sh`; `scripts/run-custom-tests.py` 18 / 18.

## Not done here

- `switch`, `try`, `for-in`, `for-of` and labelled statements are still
  tree-walker units at Script top level, so a hot loop *of those* at Script
  top level keeps running them on the tree-walker (the surrounding loop is
  native). Lowering them natively needs the completion-value bookkeeping per
  construct.
- Global `let`/`const`/class declarations stay hand-offs by design.
- A Script with direct `eval`, `with` or `import()`/`import.meta` still
  bails whole.
