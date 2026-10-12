# Function-object creation cost (#901)

## Change

`Interpreter::create_function` no longer interns five property keys through the
SipHash `KEY_CACHE`, spills the `PropertyMap` into a `HashMap`, allocates a
`class_name` `String`, builds a fresh empty-name `JsString` and inserts into a
`function_realm_map`:

- `length`/`name`/`caller`/`arguments`/`prototype`/`constructor` are interned once
  (`FunctionKeys`) and appended with `JsObjectData::push_fresh_property`.
- `PropertyMap` keeps up to `LINEAR_MAX = 8` entries in its linear `SmallVec`
  (heap-backed past `INLINE_CAP = 4`), so a sloppy constructable function's five
  properties never hash. `INLINE_CAP` is unchanged; `function_realm` adds one
  `Option<usize>` to `JsObjectData` (still under the 3 KB tripwire).
- `class_name` is `Cow<'static, str>`: every object, not just functions, stops
  allocating a `String` for its class tag.
- Anonymous functions share one empty `JsString`.
- Bound functions, `Proxy` and class constructors are created in their final
  shape (`FunctionInit`) instead of create-then-strip.
- The function realm is a `JsObjectData` field (`function_realm`), replacing the
  `function_realm_map` insert per function and remove per freed object.

Per-literal parameter lists and flags were shared by #956, which landed while this
was in review; this change reuses that.

## Measurement

`perf` and `valgrind` are unavailable on the build host, so instruction counts
were not measured. `user` CPU seconds, median of 3 runs per round, rounds
interleaved (`origin/main` @ `4fa14fac` binary, this branch, boa 0.22.0),
shared host at load average 16-26, so absolute numbers are noisy:

| benchmark | origin/main | branch | boa 0.22 | branch / boa |
|---|---|---|---|---|
| `bench_calls_closure_create.js` r1 | 1.12 s | 0.70 s | 0.37 s | 1.9x |
| r2 | 1.12 s | 0.82 s | 0.40 s | 2.1x |
| r3 | 1.27 s | 0.79 s | 0.34 s | 2.3x |
| `bench_promises_new_promise.js` r1 | 2.67 s | 2.70 s | 0.82 s | 3.3x |
| r2 | 3.64 s | 2.66 s | 1.46 s | 1.8x |
| r3 | 3.66 s | 2.27 s | 1.66 s | 1.4x |

The closure acceptance ratio (<= 3.0 vs boa) is met in every round. The
promise numbers swing too much under load to quote a ratio; the mean branch vs
main change is about -20%.

Object-heavy no-regression check (`LINEAR_MAX` changes lookups for 5-8
property objects), main vs branch, two interleaved rounds: `bench_object.js`
0.40/0.39 and 0.39/0.42; `bench_opmix.js` 3.95/4.67 and 6.46/5.57 — the sign
flips between rounds, so no consistent regression is visible at this noise
level; `bench_closures.js` 2.83/2.08 and 2.46/1.78 (-27%).

The deterministic gate is `tests/perf_counters_function_creation.rs`
(`key_intern_calls` stays flat as closures and promises grow). The
instruction-count criterion (<= 5k per closure) is unverified.

## Not done

The issue's lazily-materialised own properties were not needed to reach the
wall-clock target; they remain a follow-up if closure creation needs to go
further (the second allocation of the `.prototype` object is the largest
remaining per-closure cost).
