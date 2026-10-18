# #1122: trim the native activation of a compiled call

#1122 proposes running a compiled-to-compiled call inside `run_chunk_inner`'s dispatch loop instead of recursing natively.
Before taking on that rewrite, the call path was re-profiled on `origin/main` (`731c3749`, fib, callgrind with line tables, Ir
per call over 2,692,537 calls) to see what the native activation costs on its own.

| piece (self Ir per fib call) | Ir |
|---|---:|
| `call_compiled_function` (frame/environment setup, `run_activation`) | 237 |
| `call_function_ic_validated`, `call_function_inner_impl`, `compiled_call_target` | 100, 56, 40 |
| `pop_operands` | 100 |
| `run_compiled_body`, `run_chunk_with_var_prologue` | 99, 63 |
| `recycle_environment` (+ binding drop glue) | 97 + 29 |
| `bind_simple_parameters` + `push_unique` | 75 + 29 |
| `recycle_vm_buffer`, `acquire_function_environment`, `adopt_pooled` | 43, 32, 33 |

The layering itself is cheap: merging `call_function_ic_validated`, `call_function_inner_impl`, `run_compiled_body` and
`run_chunk_with_var_prologue` into their callers saved 20 Ir per call (-1.05% on fib) on its own. What a call costs is a long
tail of real work (environment, parameters, call frame, IC switch, operand unrooting) that an in-loop frame push would still do;
boa avoids part of it only because it shares one operand stack and has no environment object. The in-loop push would save the
dispatch-entry scaffolding, roughly 100 of the 1,640 Ir per call, at the price of a larger `run_chunk_inner` frame and of
exception, `finally`, tail-call, generator and depth-guard cases crossing frames. It was not attempted: the pieces outside the
dispatch loop gave a larger gain, below.

## Change

- **Fewer native layers.** `call_function_ic_validated` calls `with_call_depth` and `call_function_inner_impl` directly;
  `call_function_inner_impl`, `run_compiled_body`, `run_chunk` and `run_chunk_with_var_prologue` are `#[inline(always)]`
  (`run_chunk_inner` itself is unchanged). The compiled-call target check reads one `ObjectKind::Ordinary` test instead of four
  `proxy()`/`wrapped()` probes; a function whose kind is not `Ordinary` keeps the general path.
- **Parameter binding in one pass.** `BindingMap::fill_distinct` writes the parameters of a fresh environment with one reserve
  and one `Vec::extend`, with the name filter precomputed per literal (`ParamAtoms::filter`). It applies when the names are
  distinct and number at most `LINEAR_MAX` (16); longer or duplicate-named lists keep `push_unique`/`insert_atom`.
- **`ordinary_call_bind_this`** is inline for the strict, object and nullish cases; wrapping a primitive stays out of line.
- **`refs`.** A chunk with no reference slots no longer runs the drop glue of its empty `Vec<IdentifierRef>`.

No `unsafe` is added.

Measured and not shipped: lazy `private_ids` initialisation inside `run_chunk_inner` (opmix +4.0%, the loop control +24 Ir per
iteration; any edit inside the dispatch function moves code generation for every opcode), and one borrow instead of two in
`recycle_environment` (about -5 Ir per call, but loop +1.6% and string +1.8%; the function is inlined into the same loop). Both
are layout effects, not work.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`; base = `origin/main` `731c3749`, new = base plus this change. JetStream
drivers at `--iterations 1`. Each row is one deterministic run, except navier-stokes, whose Ir moves by up to +-2.5% between
concurrent runs of the same binary; its row is the run-alone pair (base 3,145.8M and 3,153.0M, new 3,145.0M and 3,150.2M), and
Box2D's base differs by 0.24% between two runs.

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| proxy-vue | 1,158,093,732 | 1,077,302,194 | -6.98% |
| fib | 5,111,975,794 | 4,883,111,780 | -4.48% |
| js-tokens | 4,750,691,632 | 4,587,248,456 | -3.44% |
| ML | 50,625,951,832 | 49,054,330,612 | -3.10% |
| hash-map | 13,006,488,967 | 12,682,345,641 | -2.49% |
| array | 739,213,811 | 721,266,571 | -2.43% |
| bigint-noble-secp256k1 | 4,855,433,918 | 4,749,638,926 | -2.18% |
| delta-blue | 6,335,442,949 | 6,198,535,215 | -2.16% |
| bigint-noble-bls12-381 | 56,442,049,044 | 55,449,832,440 | -1.76% |
| Air | 1,769,885,060 | 1,739,294,869 | -1.73% |
| raytrace | 11,665,323,204 | 11,466,286,943 | -1.71% |
| bigint-noble-ed25519 | 5,611,449,112 | 5,519,075,596 | -1.65% |
| Babylon | 2,351,712,789 | 2,313,550,336 | -1.62% |
| bigint-bigdenary | 4,211,359,832 | 4,145,962,247 | -1.55% |
| closure_create | 2,500,206,298 | 2,462,970,120 | -1.49% |
| richards | 8,529,003,110 | 8,403,212,475 | -1.47% |
| earley-boyer | 4,982,529,290 | 4,909,746,667 | -1.46% |
| splay | 7,678,916,211 | 7,567,041,759 | -1.46% |
| proxy-mobx | 567,993,843 | 561,128,327 | -1.21% |
| closures | 5,788,101,978 | 5,724,905,023 | -1.09% |
| Basic | 3,506,488,018 | 3,472,508,280 | -0.97% |
| ai-astar | 10,068,077,269 | 9,970,846,128 | -0.97% |
| lazy-collections | 3,749,897,386 | 3,715,967,606 | -0.90% |
| opmix | 38,860,067,626 | 38,604,807,523 | -0.66% |
| Box2D | 5,410,288,473 | 5,382,172,025 | -0.52% |
| crypto | 3,852,703,324 | 3,839,069,809 | -0.35% |
| navier-stokes | 3,152,952,632 | 3,144,991,343 | -0.25% |
| sync-fs | 5,987,249,344 | 5,976,214,793 | -0.18% |
| doxbee-async | 2,367,990,269 | 2,363,828,013 | -0.18% |
| promise_new | 5,780,929,569 | 5,770,988,756 | -0.17% |
| stanford-crypto-sha256 | 3,541,628,416 | 3,536,452,363 | -0.15% |
| async-fs | 7,503,343,305 | 7,497,371,822 | -0.08% |
| gaussian-blur | 19,873,658,390 | 19,858,172,468 | -0.08% |
| json | 312,843,255 | 312,783,901 | -0.02% |
| json-parse-inspector | 9,167,207,809 | 9,165,877,539 | -0.01% |
| string | 335,984,831 | 335,982,498 | -0.00% |
| loop | 3,087,546,581 | 3,087,545,333 | -0.00% |
| object | 1,466,120,952 | 1,466,135,398 | +0.00% |
| doxbee-promise | 3,968,580,965 | 3,970,880,780 | +0.06% |
| regex | 116,854,367 | 116,969,693 | +0.10% |

Geometric mean over 40 workloads: -1.28%. The eight call-heavy controls (fib, richards, delta-blue, Box2D, lazy-collections, bigint-noble-secp256k1, opmix, gaussian-blur) all fall; the largest rise is regex at +0.10% (115k Ir).

Reproducer (the leaf-call script, Ir per iteration = (N=250,000 minus N=50,000) / 200,000):

| shape | base | new | delta |
|---|---:|---:|---:|
| call-free loop (control) | 441 | 440 | -1 |
| sloppy, 1 argument | 2,079 | 1,993 | -86 |
| strict, 1 argument | 2,058 | 1,973 | -85 |
| sloppy, 4 arguments | 2,492 | 2,290 | -202 |

One call (minus the control) costs 1,638 Ir at base and 1,553 now. Per extra parameter 138 -> 99 Ir; sloppy minus strict stays 20.

Wall (hyperfine, 9 runs, pinned to one CPU, loadavg about 1.5, mean of two alternating invocations):

| benchmark | base | new | delta |
|---|---:|---:|---:|
| fib | 1.036 s | 0.930 s | -10.2% |
| richards | 1.996 s | 1.940 s | -2.8% |
| delta-blue | 1.417 s | 1.379 s | -2.7% |
| hash-map | 3.187 s | 3.145 s | -1.3% |

## Correctness gates

`test262-extra/call-compiled-function-parameter-binding-shapes.js` calls functions with 0, 1, 2, 16, 17 and 20 parameters,
duplicate names, a parameter named `arguments`, missing and extra arguments, captured and reassigned parameters, and a `var` or
function declaration redeclaring a parameter, ten times each so the later calls take the compiled path.

Full test262 default and `--no-bytecode`: 99,911/99,911 each; `test262-extra/` normal, `--no-bytecode` and both under
`JSSE_GC_STRESS=7`, on the plain release and the `release-checked` binaries; `cargo test --lib`; `tests/recursion-limit-*.js` and
`scripts/run-custom-tests.py` on the plain release build; `./scripts/lint.sh`.
