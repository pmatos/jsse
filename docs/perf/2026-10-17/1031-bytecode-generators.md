# #1031: sync generator bodies as suspending bytecode chunks

A sync generator whose body the compiler can host no longer runs as a state machine over tree-walked payloads: its
body is one chunk, `Op::Yield` suspends it, and `next`/`return`/`throw` re-enter it (ADR-2026-10-17-0640).

## Where the Ir went (base = `origin/main` `3d126b87`, lazy-collections, callgrind)

Total 8.97 G Ir. The state-machine driver and its payload were the benchmark:

- 3,516,870 state steps for 597,001 resumes (the machine had already lost the empty-block states to #1046), each
  paying `generator_next_state_machine_impl` + `sync_generator_scope_stack` + `reconcile_scope_stack` +
  `align_generator_for_of_stack` (about 600 Ir per step);
- `ast_work_units` 8.6 M against 5.0 M VM ops: terminator expressions and state payloads on the tree-walker, with
  string-keyed name lookup (`resolve_identifier_hinted::<String>` 14.1%, `walk_scope_chain::<String>` 13.0%,
  `find_in::<str>` 9.8% inclusive);
- GC (1%) and iterator-result objects (`create_iter_result_object` 13.8 K calls against 594 K resumes) were not the gap.

After the change the resume path itself is `bytecode_generator_resume` at about 70 Ir self per resume; `ast_work_units`
is 125 K and `vm_ops` 18.0 M. What is left in the profile is the generic VM: by-name `LoadName` of captured and
non-simple-parameter locals (`walk_scope_chain::<Atom>` 16% inclusive; #1022, #1024) and the VM dispatch itself.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

base = `origin/main` `3d126b87` (binary sha256 `b13347988481...` with line tables and frame pointers for Ir, `c27988e30ef7...`
shipped for wall); new = this change (`07e9a2ef6927...` / `dc7f44f6c3be...`).

cachegrind `--cache-sim=no`, whole process, `--iterations 1` JetStream drivers:

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| lazy-collections | 8,970,659,957 | 5,239,502,127 | -41.6% |
| js-tokens | 9,514,155,438 | 5,419,075,259 | -43.0% |
| sync-fs | 10,609,730,338 | 7,765,783,746 | -26.8% |
| async-fs (control, async generators/functions unchanged) | 8,401,424,332 | 8,386,668,071 | -0.2% |
| Basic | 4,109,075,914 | 4,000,087,884 | -2.7% |
| Babylon | 3,390,949,599 | 3,375,786,868 | -0.4% |
| richards (no generators) | 13,864,734,038 | 13,845,690,190 | -0.1% |
| delta-blue (no generators) | 9,054,454,045 | 9,069,268,902 | +0.2% |
| ML (no generators in the timed path) | 73,137,288,961 | 72,429,450,711 | -1.0% |

`run_chunk_inner` is generic over `GENERATOR` so the non-generator instantiation carries none of the resume code; the
first variant that kept one function cost +0.5% Ir on richards/delta-blue/ML.

Wall (`hyperfine -N -w 1 -r 9`, load 2-3, boa 0.22 release binary alongside):

| benchmark | base | new | boa 0.22 |
|---|---:|---:|---:|
| lazy-collections | 2.243 s | 1.204 s (-46.3%) | 1.173 s |
| js-tokens | 2.861 s | 1.561 s (-45.4%) | 8.037 s |
| sync-fs | 2.340 s | 1.823 s (-22.1%) | 1.923 s |
| async-fs | 2.212 s | 2.196 s | 2.600 s |

Standalone fixtures from the issue (net Ir per `next()`/yield, N=30k/90k, consumer inside a function; boa 0.22 profiled tag):

| fixture | base | new | boa |
|---|---:|---:|---:|
| trivial `while(true) yield i++` | 5,938 | 4,286 | 5,532 |
| + 5 arithmetic statements per yield | 12,995 | 6,463 | 6,470 |
| `range` (`for (let i...) { yield i; }`, for-of consumer) | 6,879 | 3,145 | 5,188 |
| manual `next(i).value` accumulator (`const v = yield total`) | 13,276 | 6,688 | 5,467 |
| yield inside try/finally inside try/finally | 11,827 | 5,020 | 6,273 |

## Not changed

- Generators with `yield*`, `using`, destructuring defaults or templates containing `yield`, and every generator under
  `--no-bytecode`, keep the state machine (lazy-collections has three such bodies, 459 steps).
- Async generators and async functions: still state machines with tree-walked payloads (async-fs setup, the per-await
  bookkeeping of the issue's section 4).
- The state machine evaluates operands that precede a `yield` when it resumes, not before suspending
  (`note(1) + (yield note(2))` logs 2,1, #1104); the hosted path is spec-correct (see
  `test262-extra/generator-bytecode-yield-in-expressions.js`).
