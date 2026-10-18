# #1024: bind default and rest parameters through cached atoms, run the body in the parameter environment

A function with a default or rest parameter took the general call path: `bind_function_parameters` walked every parameter through
`bind_pattern` (two `Atom::intern` calls and a keyed probe per parameter, ~1,690 Ir per call for two parameters), a fresh body
`Environment` was allocated per call (not pooled), the hoisting analysis was looked up by Body key, and the frame layout was
unknown to the compiler, so every parameter read was a by-name `LoadName`.

## Change

- **`NamedParamPlan`** (`LiteralFrame::named_param_plan`): for a parameter list made only of plain names, `name = default` and a
  final `...rest`, the interned atoms, in order. `bind_named_parameters` replays what `bind_pattern` does for those forms in a
  function scope with no `with` scope on the chain (a default is evaluated with its own name already in TDZ; the final binding
  replaces that entry in place); `has_ever_entered_with` keeps the generic path. A parameter named `arguments` has no plan.
- **Shared parameter environment** (`LiteralFrame::shares_param_env`). The separate body environment of
  FunctionDeclarationInstantiation step 28 exists so closures created by parameter expressions cannot see body declarations.
  When no default contains a function, arrow, class or direct `eval`, the body contains no direct `eval` (its `var`s are checked
  against the parameter scope), no function declaration (Annex B hoisting distinguishes parameter bindings) and no `var`/lexical
  called `arguments`, the difference is unobservable: defaults run before any body binding exists, and `var x` over a parameter `x`
  is the same binding with the same initial value. The body then runs in the parameter environment, and the compiler lays the
  frame out as `arguments`, the parameters in order, the vars and the top-level lexicals, exactly like a simple-parameter function,
  so parameter reads and writes are `LoadLocal`/`StoreLocal` (`function_frame_shape` returns that shape for such non-generator,
  non-async functions).
- **Otherwise** the body environment comes from the function-environment pool (`acquire_body_environment`, recycled after the
  call), is filled from the literal's `BodyVarPlan` (atoms plus whether each `var` copies the parameter of the same name) and
  reads the frame's hoisting analysis instead of probing the Body-keyed memo.
- **Compiled-call path.** `CompiledCall` is installed for these functions too, and `call_compiled_function_with_defaults` (out of
  line, so the simple-parameter `call_compiled_function` is untouched) serves them; it is skipped once a `with` statement has run.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`; base = `origin/main` `ead00de7` (#1126). Per-iteration Ir from runs of 50k and
150k iterations of `function add(a, b = 1) { return a + b; }` called from a loop:

| form | base Ir/iter | new Ir/iter | delta |
|---|---:|---:|---:|
| `add(a, b)` (simple parameters, control) | 2,845 | 2,851 | +0.2% |
| `add(a, b = 1)`, `b` supplied | 5,271 | 2,950 | -44.0% |
| `add(a, b = 1)`, `b` omitted | 5,396 | 3,030 | -43.8% |

The default form is 1.035x the simple form (issue acceptance: <= 1.1x). `atom_intern_calls` is flat across call counts and no
`LoadName` executes (`tests/perf_counters_env_hashing.rs::default_and_rest_parameters_intern_nothing_and_resolve_through_slots`).

Whole-process, 40 workloads (geometric mean -0.90%):

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| bigint-noble-secp256k1 | 5,915,520,611 | 5,152,624,504 | -12.90% |
| bigint-noble-ed25519 | 6,782,080,986 | 6,012,470,026 | -11.35% |
| js-tokens | 4,844,517,541 | 4,557,312,079 | -5.93% |
| Air | 1,898,735,175 | 1,790,055,010 | -5.72% |
| lazy-collections | 3,961,279,185 | 3,866,555,950 | -2.39% |
| proxy-vue | 1,131,751,696 | 1,123,649,999 | -0.72% |
| micro:fib | 5,600,672,766 | 5,616,826,122 | +0.29% |
| richards | 8,660,674,128 | 8,672,818,761 | +0.14% |
| delta-blue | 6,408,411,688 | 6,422,052,873 | +0.21% |
| hash-map | 13,943,873,509 | 14,011,561,622 | +0.49% |
| proxy-mobx (noise: 574.8M-598.9M over runs of either binary) | 586,555,407 | 594,840,700 | +1.41% |

A first version that folded the default-parameter handling into `call_compiled_function` cost micro:fib +2.6% by code layout alone;
splitting it out of line brought that to +0.29%.
