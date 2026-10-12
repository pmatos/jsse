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
  properties never hash. `INLINE_CAP` and `size_of::<JsObjectData>()` are unchanged.
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

`perf` and `valgrind` are unavailable on the build host, so instruction counts were
not measured. `user` CPU seconds, median of 5 (3 for promises), same host under
load average 17-42, HEAD `9703e359` baseline binary vs this branch:

| benchmark | baseline | branch | change |
|---|---|---|---|
| `bench_closure_create.js` | 1.85 s | 0.55 s | -43% to -70% by run (load-dependent) |
| `bench_promise_new.js` | 2.82 s | 1.72 s | -39% |

The deterministic gate is `tests/perf_counters_function_creation.rs`
(`key_intern_calls` stays flat as closures and promises grow).

## Not done

The issue's lazily-materialised own properties were not needed to reach the
wall-clock target; they remain a follow-up if closure creation needs to go
further (the second allocation of the `.prototype` object is the largest
remaining per-closure cost).
