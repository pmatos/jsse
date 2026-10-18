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
  distinct and number at most `LINEAR_MAX` (16) and the map is empty; any other case falls back to `push_unique` inside the method,
  and duplicate-named lists keep `insert_atom`.
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
| proxy-vue | 1,158,093,732 | 1,093,301,766 | -5.59% |
| fib | 5,111,975,794 | 4,893,880,318 | -4.27% |
| js-tokens | 4,750,691,632 | 4,589,644,832 | -3.39% |
| ML | 50,625,951,832 | 49,132,160,469 | -2.95% |
| hash-map | 13,006,488,967 | 12,698,008,166 | -2.37% |
| array | 739,213,811 | 722,111,695 | -2.31% |
| bigint-noble-secp256k1 | 4,855,433,918 | 4,752,742,565 | -2.11% |
| delta-blue | 6,335,442,949 | 6,205,132,152 | -2.06% |
| Air | 1,769,885,060 | 1,739,140,147 | -1.74% |
| bigint-noble-bls12-381 | 56,442,049,044 | 55,483,775,757 | -1.70% |
| bigint-noble-ed25519 | 5,611,449,112 | 5,519,843,561 | -1.63% |
| raytrace | 11,665,323,204 | 11,476,087,947 | -1.62% |
| bigint-bigdenary | 4,211,359,832 | 4,149,567,254 | -1.47% |
| Babylon | 2,351,712,789 | 2,318,639,851 | -1.41% |
| richards | 8,529,003,110 | 8,410,073,096 | -1.39% |
| splay | 7,678,916,211 | 7,571,858,060 | -1.39% |
| closure_create | 2,500,206,298 | 2,466,165,691 | -1.36% |
| proxy-mobx | 567,993,843 | 560,878,609 | -1.25% |
| earley-boyer | 4,982,529,290 | 4,929,063,013 | -1.07% |
| ai-astar | 10,068,077,269 | 9,961,328,719 | -1.06% |
| Basic | 3,506,488,018 | 3,475,014,591 | -0.90% |
| lazy-collections | 3,749,897,386 | 3,719,289,002 | -0.82% |
| closures | 5,788,101,978 | 5,743,867,141 | -0.76% |
| crypto | 3,852,703,324 | 3,827,327,058 | -0.66% |
| opmix | 38,860,067,626 | 38,616,819,561 | -0.63% |
| Box2D | 5,410,288,473 | 5,383,984,948 | -0.49% |
| promise_new | 5,780,929,569 | 5,771,013,505 | -0.17% |
| sync-fs | 5,987,249,344 | 5,978,523,836 | -0.15% |
| doxbee-promise | 3,968,580,965 | 3,962,940,770 | -0.14% |
| doxbee-async | 2,367,990,269 | 2,364,881,230 | -0.13% |
| stanford-crypto-sha256 | 3,541,628,416 | 3,537,524,747 | -0.12% |
| gaussian-blur | 19,873,658,390 | 19,859,250,973 | -0.07% |
| async-fs | 7,503,343,305 | 7,500,200,538 | -0.04% |
| regex | 116,854,367 | 116,807,350 | -0.04% |
| navier-stokes | 3,152,952,632 | 3,151,999,553 | -0.03% |
| json | 312,843,255 | 312,790,941 | -0.02% |
| object | 1,466,120,952 | 1,466,102,852 | -0.00% |
| string | 335,984,831 | 335,983,030 | -0.00% |
| loop | 3,087,546,581 | 3,087,545,565 | -0.00% |
| json-parse-inspector | 9,167,207,809 | 9,167,329,152 | +0.00% |

Geometric mean over 40 workloads: -1.19%. The eight call-heavy controls (fib, richards, delta-blue, Box2D, lazy-collections, bigint-noble-secp256k1, opmix, gaussian-blur) all fall; no workload rises by more than 0.01%.

Reproducer (the leaf-call script, Ir per iteration = (N=250,000 minus N=50,000) / 200,000):

| shape | base | new | delta |
|---|---:|---:|---:|
| call-free loop (control) | 441 | 441 | 0 |
| sloppy, 1 argument | 2,079 | 1,997 | -82 |
| strict, 1 argument | 2,058 | 1,976 | -82 |
| sloppy, 4 arguments | 2,492 | 2,297 | -195 |

One call (minus the control) costs 1,638 Ir at base and 1,556 now. Per extra parameter 138 -> 100 Ir; sloppy minus strict stays 21.

Native stack: a script recursing to the catchable limit (4,000 levels, then a RangeError) peaks at 32.2 MB RSS with this change and
32.7 MB at base, so merging the layers does not enlarge the per-call native footprint.

Wall (hyperfine, 9 runs, pinned to one CPU, loadavg 1.5 to 3, mean of two alternating invocations):

| benchmark | base | new | delta |
|---|---:|---:|---:|
| fib | 1.030 s | 0.929 s | -9.8% |
| richards | 1.988 s | 1.939 s | -2.5% |
| delta-blue | 1.417 s | 1.375 s | -3.0% |
| hash-map | 3.183 s | 3.134 s | -1.5% |

## Correctness gates

`test262-extra/call-compiled-function-parameter-binding-shapes.js` calls functions with 0, 1, 2, 16, 17 and 20 parameters
(sixteen also beside a `var`, a closure and a function declaration), duplicate names, a parameter named `arguments`, parameters
beside an `arguments` object, missing and extra arguments, captured and reassigned parameters, and a `var` or function
declaration redeclaring a parameter, ten times each so the later calls take the compiled path. `binding_map.rs` unit tests cover
`fill_distinct` against `push_unique` and the index build on the 17th name.

Full test262 default and `--no-bytecode`: 99,911/99,911 each; `test262-extra/` normal, `--no-bytecode` and both under
`JSSE_GC_STRESS=7`, on the plain release and the `release-checked` binaries; `cargo test --lib`; `tests/recursion-limit-*.js` and
`scripts/run-custom-tests.py` on the plain release build; `./scripts/lint.sh`.
