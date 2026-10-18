# #1025: cheaper arguments objects and arguments-using calls

`arguments`-using calls were the slowest ordinary call shape: `create_arguments_object` cost 3,806 Ir for three arguments
(6 eager `insert_property` calls through the key interner, `PropertyMap` growth by append), the call path re-declared the
literal name `arguments` through the string API (an `Atom::intern` per call), a mapped object built a
`HashMap<String, (EnvRef, String)>` (two `String`s per mapped parameter), every `arguments[i]` took the generic
`ToPropertyKey` + [[Get]] path, a mapped object's `arguments.length` took the generic path too (the Arguments kind was
classified as exotic for every named access), and the function never reached the compiled-call fast path.

## Change

- `create_arguments_object` builds the object from pre-interned keys (`length`, `callee` in `CommonKeys`, shared small index
  keys) with `push_fresh_property` into a map reserved for `length + callee + n + @@iterator`.
- The `arguments` binding is a pre-interned atom pushed first into the fresh function environment
  (`Interpreter::bind_arguments_binding`), the layout the compiler's local slots already assume. No `declare`/`env_set`, no
  interner call per call.
- `ObjectKind::Arguments` carries a `ParameterMap` instead of a `HashMap`: the environment, the literal's shared parameter
  atoms (`ParamAtoms::shared_atoms`) and one `live` flag per parameter (`ParamAtoms::last_occurrence` marks the last occurrence of a
  duplicated name; the flags are cut at the argument count). Reads and write-through use the atom, with no hashing; `delete`,
  accessor and `writable: false` redefinition clear a flag. Index keys are parsed canonically (`"00"`, `"+0"`, `"1.0"` never alias).
- `ObjectKind::Arguments` is no longer `is_named_access_exotic`: only Array-index keys are answered through the map, and every
  named fast path already hands digit-leading keys to the generic path (as it does for Arrays). `arguments.length` and
  `arguments.callee` of a mapped object now use the slot-hinted named paths.
- `numeric_index_fast_get` serves a computed index read of an Ordinary or Arguments object whose own data property exists
  (reading a mapped parameter live); anything else, including an inherited or accessor index, takes the generic path. This also
  speeds `obj[0]` on ordinary objects with integer keys.
- Compiled calls take non-arrow functions with simple parameters that mention `arguments` (`CallShape::Arguments`,
  `call_compiled_function_with_arguments`, out of line so the plain path is unchanged). `CompiledCall.has_parameter_expressions`
  became `CompiledCall.shape`.
- `Function.prototype.apply` reads the argument list through `numeric_index_fast_get` before falling back to the string-keyed Get
  (still one `length` Get, indices in order, accessors and Proxy traps unchanged).

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

Base = `origin/main` at `f52b9219` (binary sha256 `91545a930b59...`), new = base plus this change (`2a860f88bcee...`; the PR's
source differs only in `#[cfg(test)]` code and formatting). cachegrind `--cache-sim=no`, whole-process `I refs`.

Fixtures (`fx/*.js`: Ir per iteration = (N=150,000 minus N=50,000) / 100,000):

| fixture | base | new | delta |
|---|---:|---:|---:|
| `add(i,1,2)` summing `arguments` (the issue's args-object) | 12,422 | 9,140 | -26.4% |
| `f(){return arguments.length}` with 3 arguments | 7,434 | 4,485 | -39.7% |
| sloppy `add(a,b)` reading `arguments[1..]` (mapped) | 16,776 | 9,333 | -44.4% |
| strict `add()` summing `arguments` (unmapped) | 12,356 | 9,035 | -26.9% |
| `Base.apply(this, arguments)` through `new Derived(i,1)` | 17,798 | 12,835 | -27.9% |
| `Base.call(this, a, b)` through `new Derived(i,1)` (control) | 8,711 | 8,718 | 0.0% |
| `add(a,b,c)` explicit parameters (control) | 2,544 | 2,546 | 0.0% |

`create_arguments_object` (callgrind, 100,000 calls, three arguments): 3,806 Ir -> 1,371 Ir inclusive (the issue's target is <= 1,600;
`create_object_id` 262 and the reserve 220 remain).

Wall, 1,000,000 iterations, one pinned core, load average 0.3-0.7, hyperfine 9 runs:

| fixture | base | new | boa 0.22 (same session) |
|---|---:|---:|---:|
| args-sum | 2.749 s | 2.032 s | 2.343 s |
| args-len | 1.571 s | 0.962 s | 0.897 s |
| args-mapped | 3.853 s | 2.271 s | 2.451 s |
| args-apply | 4.358 s | 3.139 s | 4.727 s |
| args-params (control) | 509 ms | 489 ms | 645 ms |

The bare `arguments.length` call (args-len) is still 8% behind boa; boa's object creation is lazier.

Whole-process Ir, every JetStream driver and micro (no control rises by more than 0.21%; geometric mean -0.20%):

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| proxy-mobx | 573,506,053 | 554,265,375 | -3.35% |
| micro:opmix | 39,288,491,582 | 38,625,326,132 | -1.69% |
| Box2D | 5,387,283,460 | 5,341,160,292 | -0.86% |
| js-tokens | 4,751,328,228 | 4,712,467,516 | -0.82% |
| bigint-noble-secp256k1 | 4,745,969,231 | 4,724,761,282 | -0.45% |
| sync-fs | 6,001,511,593 | 5,977,338,646 | -0.40% |
| earley-boyer | 4,920,156,333 | 4,901,593,569 | -0.38% |
| proxy-vue | 1,079,673,730 | 1,076,955,886 | -0.25% |
| ML | 49,134,101,912 | 49,032,201,604 | -0.21% |
| crypto | 3,834,421,939 | 3,826,836,246 | -0.20% |
| micro:regex | 116,969,729 | 116,748,925 | -0.19% |
| Air | 1,742,131,222 | 1,740,593,037 | -0.09% |
| bigint-noble-ed25519 | 5,521,137,435 | 5,516,292,594 | -0.09% |
| async-fs | 7,501,922,578 | 7,497,225,646 | -0.06% |
| Basic | 3,475,199,320 | 3,473,300,751 | -0.05% |
| micro:array | 722,216,965 | 721,896,870 | -0.04% |
| json-parse-inspector | 9,167,052,962 | 9,164,000,635 | -0.03% |
| raytrace | 11,488,581,101 | 11,485,116,117 | -0.03% |
| micro:json | 312,784,280 | 312,727,458 | -0.02% |
| Babylon | 2,318,189,419 | 2,318,066,227 | -0.01% |
| micro:string | 335,980,591 | 335,979,641 | -0.00% |
| micro:loop | 3,087,544,864 | 3,087,543,389 | -0.00% |
| doxbee-async | 2,365,154,454 | 2,365,525,962 | +0.02% |
| micro:object | 1,466,121,636 | 1,466,408,448 | +0.02% |
| gaussian-blur | 19,859,253,495 | 19,863,590,332 | +0.02% |
| richards | 8,410,099,858 | 8,412,004,827 | +0.02% |
| stanford-crypto-sha256 | 3,536,418,217 | 3,537,676,094 | +0.04% |
| micro:promise_new | 5,771,364,014 | 5,773,609,961 | +0.04% |
| doxbee-promise | 3,962,889,191 | 3,964,993,701 | +0.05% |
| micro:fib | 4,893,879,557 | 4,896,571,225 | +0.06% |
| hash-map | 12,690,987,992 | 12,698,129,156 | +0.06% |
| delta-blue | 6,206,468,549 | 6,210,089,864 | +0.06% |
| bigint-noble-bls12-381 | 55,497,960,573 | 55,531,110,151 | +0.06% |
| splay | 7,571,867,721 | 7,576,950,953 | +0.07% |
| micro:closures | 5,730,651,274 | 5,734,866,701 | +0.07% |
| micro:closure_create | 2,465,453,627 | 2,468,570,561 | +0.13% |
| navier-stokes | 3,147,541,773 | 3,152,843,605 | +0.17% |
| lazy-collections | 3,720,451,767 | 3,726,888,122 | +0.17% |
| bigint-bigdenary | 4,146,886,040 | 4,154,995,381 | +0.20% |
| ai-astar | 9,958,592,701 | 9,979,719,328 | +0.21% |

## Correctness gates

`test262-extra/arguments-object-indexed-read-and-parameter-map.js` (passes on Node, which is the reference for the semantics):
live index reads of mapped parameters, write-through, fractional/negative/-0 and non-canonical keys, a 12-parameter list, an
index past the argument count, duplicate parameter names, mapping severed by `delete`, `Reflect.deleteProperty`, accessor and
`writable: false` redefinition, an own accessor index, an inherited index, ordinary-object index reads (getter receivers,
holes, own `undefined`), named properties of a mapped object (data, inherited getter, `+=`), and `apply` with mapped,
unmapped, short, sparse, accessor, Proxy and frozen argument lists.

Full test262 default and `--no-bytecode`: 99,911/99,911 each; `test262-extra/` 1,495/1,495 normal, `--no-bytecode`, and both under
`JSSE_GC_STRESS=7`, all with the `release-checked` binary; a 10% sample with `release-checked`; `cargo test --lib` 1,159 passed;
`scripts/run-custom-tests.py` 19/19; `./scripts/lint.sh` clean. `tests/perf_counters_env_hashing.rs` gains a case proving
`atom_intern_calls`/`atom_lookup_calls` stay flat as the number of `arguments`-using calls grows.
