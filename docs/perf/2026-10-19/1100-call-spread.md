# #1100: native `CallSpread`/`ConstructSpread` for spread call arguments

Calls and `new` with spread arguments were tree-walker hand-offs for the whole expression (callee/`this` evaluation, an
`eval_spread_args` `Vec`, `call_function`). They are now VM ops: a spread pushes the iterable's elements on the operand
stack as arguments (each rooted like any operand) and a Number slot above them carries the running argument count
(`ArgSpreadFirst`/`ArgSpread`/`ArgPush`, then `CallSpread`/`ConstructSpread`). A call whose only argument is a spread
(`f(...args)`, `o.m(...args)`, `new C(...args)`) skips the count slot (`CallSpreadSole`/`ConstructSpreadSole`). The callee
borrows its arguments straight from the stack, so no argument list is built. `super(...)`, optional calls, direct `eval` and a
strict `return f(...)` (its `TailCall` completion) stay on the tree-walker. Sources: `compiler/calls.rs`, `vm/spread.rs`.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

Base = `origin/main` at `7461a9a6` (binary sha256 `c061ba2f484904...`), new = base plus this change. cachegrind
`--cache-sim=no`, whole-process `I refs`. Ir per iteration = median over five runs of (20,000 minus 2,000 iterations) / 18,000
(per-run noise is about +-60 Ir: the interner is randomised).

The issue's probes (`g(...args)` over a prebuilt array, `g` returns 1; `spreadctl` is `g(a0, a1, a2)`):

| probe | base | new | base over control | new over control |
|---|---:|---:|---:|---:|
| `spreadctl` `g(a0, a1, a2)` | 2,372 | 2,364 | | |
| `spread0` `g(...[])` array of 0 | 3,345 | 2,801 | +973 | **+437** (criterion 500) |
| `spread3` 3 elements | 3,689 | 3,042 | +1,317 | +678 |
| `spread12` 12 elements | 4,026 | 3,466 | +1,654 | **+1,102** (criterion 1,500) |
| `spreadnew` `new C(i, ...args)` | 6,477 | 5,759 | | |
| `spreadpush` `a.length = 0; a.push(...args)` | 7,873 | 7,076 | | |

`ast_fallback_execs` is 0 for each of these (`tests/perf_counters_call_spread.rs`). The control moved from 3,811 (issue text)
to 2,372 after #1134/#1122; the bounds hold against the control as it is today.

With the spread operand an array literal built in the loop (`g(...[])`, `g(...[1,2,3])`, `g(...[12 elements])`), the literal's own
allocation (about 840 Ir for `[]`, 1,380 for 12 elements) is part of the probe, not of the spread (single run each):

| probe | base | new |
|---|---:|---:|
| `g(...[])` | 4,123 | 3,636 |
| `g(...[1,2,3])` | 5,168 | 4,276 |
| `g(...[12 elements])` | 6,845 | 4,852 |
| `g(i, ...args, i)` | 4,901 | 3,401 |

Controls that never spread, same method: `g(a0,a1,a2)` 2,372 -> 2,364; `g()` 2,103 -> 2,098; 12 arguments 3,137 -> 3,121;
`new C(i, a1, a2)` 4,825 -> 4,811.

## No-regression gates

Whole-process Ir, every control (one run each; `js-tokens` and `proxy-vue` move by +-1.5% between runs of one binary, a
five-run check of both showed the new binary at or below base):

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| lazy-collections | 3,712,375,399 | 3,669,773,814 | -1.15% |
| bigint-noble-secp256k1 | 4,788,228,047 | 4,740,276,219 | -1.00% |
| gaussian-blur | 19,901,426,064 | 19,775,331,021 | -0.63% |
| richards | 8,444,242,645 | 8,401,718,731 | -0.50% |
| delta-blue | 6,210,872,685 | 6,182,209,144 | -0.46% |
| raytrace | 11,486,444,563 | 11,433,842,500 | -0.46% |
| fib | 4,896,561,780 | 4,876,365,146 | -0.41% |
| Babylon | 2,320,291,870 | 2,311,995,972 | -0.36% |
| splay | 7,573,452,376 | 7,546,380,275 | -0.36% |
| Box2D | 5,342,420,252 | 5,324,266,219 | -0.34% |
| ai-astar | 9,972,114,099 | 9,941,906,919 | -0.30% |
| loop | 3,087,529,341 | 3,079,531,134 | -0.26% |
| proxy-vue | 1,079,063,871 | 1,076,591,787 | -0.23% |
| hash-map | 12,829,550,637 | 12,800,635,454 | -0.23% |
| opmix | 38,800,940,094 | 38,716,357,148 | -0.22% |
| string | 335,964,001 | 335,265,026 | -0.21% |
| closures | 5,688,838,597 | 5,677,668,752 | -0.20% |
| array | 721,802,195 | 720,603,793 | -0.17% |
| js-tokens | 4,702,653,908 | 4,696,480,506 | -0.13% |
| json | 312,676,773 | 312,473,774 | -0.06% |
| object | 1,466,070,985 | 1,465,135,662 | -0.06% |
| regex | 116,661,683 | 116,628,091 | -0.03% |

Geometric mean -0.35%; no benchmark above base. None of the controls executes a spread call op.

Peak RSS and wall, pinned core, load average 1.8-2.9 on the 61-core host, median of 11 interleaved runs:

| benchmark | base | new | wall | base RSS | new RSS | RSS |
|---|---:|---:|---:|---:|---:|---:|
| fib | 0.930 s | 0.910 s | -2.2% | 16.9 MB | 17.0 MB | +0.3% |
| richards | 1.920 s | 1.900 s | -1.0% | 19.0 MB | 19.1 MB | +0.2% |
| delta-blue | 1.390 s | 1.360 s | -2.2% | 23.1 MB | 23.1 MB | +0.3% |
| splay | 2.050 s | 2.020 s | -1.5% | 584.7 MB | 584.2 MB | -0.1% |
| Box2D | 1.220 s | 1.210 s | -0.8% | 39.4 MB | 39.2 MB | -0.6% |
| lazy-collections | 0.860 s | 0.880 s | +2.3% | 27.5 MB | 27.5 MB | +0.0% |
| 1M x `g(a0,a1,a2)` (control) | 0.460 s | 0.450 s | -2.2% | 16.9 MB | 16.9 MB | -0.3% |
| 1M x `g(...args)`, 3 elements | 0.730 s | 0.560 s | -23.3% | 17.0 MB | 16.9 MB | -0.3% |
| 1M x `g(...args)`, 12 elements | 0.790 s | 0.630 s | -20.3% | 17.1 MB | 16.9 MB | -1.3% |
| 1M x `new C(i, ...args)` | 1.390 s | 1.150 s | -17.3% | 20.4 MB | 20.3 MB | -0.4% |
| 1M x `a.push(...args)` | 1.540 s | 1.350 s | -12.3% | 17.2 MB | 17.2 MB | -0.3% |

`lazy-collections` is +2.3% wall (21 interleaved runs: 0.86 s base, 0.89 s new) at -1.15% Ir. It executes no spread op and no
changed function (callgrind: the Ir difference is `run_chunk_inner::<true>`, -38M, from instruction selection). This is the
code-layout effect the project already documents (equal-Ir builds moved `richards` by -0.8..+4.4% in #1028): across five
builds that differ only in unrelated code, one control moves by +-2..3% each time and it is never the same one (below).

## What the layout lottery looked like

- A first version passed the code position by `&mut pc` into the out-of-line handler. That address-taking kept `pc` out of a
  register in `run_chunk_inner` and cost fib +2.5%, opmix +3.5%, gaussian-blur +1.5%, Box2D +1.0% Ir with no spread in sight;
  the shipped arm decodes `ArgSpreadFirst`'s operand itself and passes it by value, and `spread::run` is `#[inline(never)]`.
- Hinting the two property lookups of `try_intrinsic_iteration` (`@@iterator` on `%Array.prototype%`, `next` on
  `%ArrayIteratorPrototype%`: they cost about 230 of the 540 Ir `iterate_to_vec` spends on a short Array) lowered `spread0` to +375
  over control, but every form of it (a `thread_local` cell, an atomic, out of line, a separate module) made Box2D +2.5% wall at
  an Ir within 0.2% of the build without it, while Box2D never calls the function. Dropped; the single-spread ops reach the
  bounds without touching a shared helper. The hint is the remaining lever for `iterate_to_vec` callers.
- Three builds differing from the shipped one in an attribute or the order of an arm: wall vs base fib -1.1/-3.2/+2.2%,
  richards +1.6/+0.0/+2.1%, delta-blue +1.4/-2.2/+0.7%, splay 0.0/-1.5/+0.5%, Box2D +0.8/0.0/+1.6%, lazy-collections
  0.0/+2.3/+2.3%. The shipped build is -3.2/-0.5/-2.9/-1.9/-0.8/+2.3% in that run.

## Residual

- A strict `return f(...args)` still hands off (`ReturnCall` keeps its `TailCall` completion on the tree-walker); a sloppy one
  is native.
- `super(...args)` and `o?.m(...args)` / `f?.(...args)` still hand off (a derived constructor forwarding `...args` is the
  common one).
- `iterate_to_vec`'s Array check (two property lookups, `is_plain_dense_array`, a `Vec` copy) is about 540 Ir per spread and
  is shared by every caller.

## Correctness gates

`test262-extra/call-spread-argument-evaluation.js` (checked against Node): callee, `this`, plain arguments and spreads in
order with the full iterator protocol (`@@iterator`, `next`, `done`, `value` reads), arguments built before the call, the
non-callable and non-constructor `TypeError` after the arguments, a throwing `next` not closing, non-iterables, strings, Sets,
Maps, generators, `arguments`, holes and inherited indices, getters, snapshot semantics, patched `@@iterator`/`next`, removed
and re-added built-ins, receivers (member, computed, `with`, `(0, o.m)`, arrows), `new` (class, native, bound, `new.target`),
sizes 0 to 5,000, adjacent and trailing spreads, handlers around the call, `yield` inside spread operands and arguments with
`next`/`return`/`throw`, `super(...)`, optional calls, direct and indirect `eval`, a strict spread tail call.
`test262-extra/call-spread-gc-rooting.js`: callee, receiver and every argument stay reachable across collections in later
spreads, argument expressions, iterator steps and the callee, and operands are released after throws; it also passes under
`JSSE_GC_STRESS=1,2,7`.

Full test262 default and `--no-bytecode`: 99,911/99,911 each; `test262-extra/` 1,514/1,514 normal, `--no-bytecode`, and both under
`JSSE_GC_STRESS=7`, all with the `release-checked` binary; `cargo test --profile release-nolto` (1,167 lib tests and the integration
tests, including `tests/perf_counters_call_spread.rs` under `--features perf-counters`); `scripts/run-custom-tests.py` 19/19;
`./scripts/lint.sh` clean.
