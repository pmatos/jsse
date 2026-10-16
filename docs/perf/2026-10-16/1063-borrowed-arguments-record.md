# #1063: borrow the sloppy-call arguments record instead of copying it

A sloppy, non-arrow call whose body never mentions `arguments` pushed a `CallFrameArguments::Deferred` record for the
legacy `Function.prototype.arguments` accessor. Building it cloned every argument into a `SmallVec<[JsValue; 6]>`, took
a second `Rc<Environment>` clone, and popping the frame ran both drop loops. The data is the `args: &[JsValue]` slice
`call_function_inner_impl` already holds, and that slice outlives the frame (pushed after `bind_simple_parameters`,
popped right after `dispatch_body`, same activation, nothing else truncates `call_stack_frames`).

## Change

`DeferredCallArguments` now stores a `NonNull<[JsValue]>` of the borrowed slice and a `NonNull<RefCell<Environment>>`
of the function environment. Its constructor is `unsafe` with the invariant above documented on the type; the single
call site justifies it. `CallFrameArguments::Deferred` lost its `func_env` field.

- GC: the root walk reads the borrowed slice, so the arguments stay rooted exactly as before. The function
  environment is no longer rooted by the frame: a mapped record only exists for simple parameter lists, where the
  environment pushed on `call_stack_envs` is that same environment, and an unmapped record never reads it.
- `materialize_call_frame_arguments` copies the slice and re-creates an `Rc` from the pointer on demand
  (`increment_strong_count` + `from_raw`), then replaces the record with `Materialized`, as before.
- The existing unit test that asserted the inline `SmallVec` shape now asserts the borrowed length.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`. base = `origin/main` at `9ae23c6c` (binary sha256 `0d2f77ffde33...`),
new = base plus this change (`ffebced3007c...`). JetStream drivers at `--iterations 1`.

| benchmark | base Ir | new Ir | delta | criterion |
|---|---:|---:|---:|---|
| fib (micro) | 7,964,072,653 | 7,530,570,662 | -5.44% | >= 1.5% |
| delta-blue | 9,416,771,249 | 9,110,011,305 | -3.26% | >= 1.0% |
| richards | 14,132,660,716 | 13,862,673,533 | -1.91% | >= 0.7% |
| hash-map | 23,160,153,344 | 22,546,218,333 | -2.65% | |
| Box2D | 8,564,253,785 | 8,486,579,172 | -0.91% | |
| Babylon | 3,529,759,331 | 3,501,340,557 | -0.80% | |
| bigint-noble-ed25519 | 8,003,229,616 | 7,935,502,028 | -0.85% | strict-heavy, 224k of 802k calls build a record |

Reproducer (the issue's leaf-call script, Ir per iteration = (N=250,000 minus N=50,000) / 200,000):

| shape | base | new | delta |
|---|---:|---:|---:|
| sloppy, 1 argument | 3,319 | 3,159 | -4.8% |
| sloppy, 4 arguments | 3,979 | 3,710 | -6.8% |
| sloppy, 10 arguments (control) | 5,457 | 4,815 | -11.8% |
| `"use strict";` file, 1 argument (control) | 3,161 | 3,149 | -0.4% |
| sloppy callee that reads `arguments` (#1025 shape, control) | 9,687 | 9,654 | -0.3% |
| call-free loop (control) | 853 | 853 | 0 |

The sloppy-minus-strict gap at one argument goes 158 -> 10 Ir (criterion <= 105). The issue's absolute baselines
(3,522 and 4,308) predate #1062/#1088; against those, 3,159 and 3,710 are well inside the <= 3,465 and <= 4,150 targets.
The saving scales with the argument count (about 66 Ir per extra argument at ten), as the issue predicted.

## Correctness gates

`test262-extra/function-arguments-accessor-borrowed-call-record.js` reads `f.arguments` through a computed key from
inside the callee: extra arguments beyond the parameter list, a mutated mapped parameter, writes through the
materialized object aliasing the parameter, object identity per activation, objects reachable only through the record
across `$262.gc()` (including a non-simple parameter list), recursive activations, the cleared record after return,
caller versus callee records, and 0, 1 and 10 arguments. It passes on the base binary too (no semantic change), in both
modes and under `JSSE_GC_STRESS=1`.

Full test262 default and `--no-bytecode`: 99,911/99,911 each; `test262-extra/` 1,421/1,421 normal, `--no-bytecode`, and
both under `JSSE_GC_STRESS=7`, and with the `release-checked` binary in both modes; `cargo test --lib` 1,112 passed;
`scripts/run-custom-tests.py` 19/19; `./scripts/lint.sh` clean.
