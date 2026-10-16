# #1057: numeric element stores and updates skip key stringification

`o[i] = v`, `o[i] op= v` and `o[i]++` with a Number key took the generic [[Set]] on every path except the bytecode VM's
`SetElement`: the tree-walker (generator state bodies, `--no-bytecode`, `eval`/`with` bodies) built the key string
(`u32` -> `String` -> `Arc<[u8]>`), re-parsed it (`parse_array_index`), and for a TypedArray round-tripped it through
`canonical_numeric_index_string` (`str::parse::<f64>` + ryu-js + compare) before the store. The VM's `Op::UpdateElement`
paid the same twice (read and write).

## Change

- `eval_assign`, computed member: the evaluated key stays a `JsValue`. A Number key reads (compound) through
  `numeric_index_fast_get` before the RHS and stores through `numeric_index_fast_set` after it; the `JsPropertyKey` is
  built only when a helper declines (or for the null-base message). The `JsPropertyKey::from_str("")` placeholder every
  simple assignment allocated is gone. The inline dense-Array block is replaced by the same helper for String keys that
  are array indices (`a["3"] = v`).
- New `Interpreter::update_member_computed` (`eval.rs`), shared by the tree-walker `eval_update` and the VM's
  `member_update_computed`: fast get -> `apply_update_numeric` -> fast set, falling into the generic [[Set]] with the
  already computed value when the store declines (no second read, `ToNumeric` runs once).
- `canonical_numeric_index_string`: 1-15 ASCII digits without a leading zero (and `"0"`) return immediately; this also
  speeds string-keyed typed-array access (`ta["5"]`, `for-in`, `Reflect.*`). The `ta.clone()` in TypedArray [[Set]] is gone.
- `numeric_index_fast_set` is now called from several sites, so LLVM stopped inlining it into the VM loop (+7 Ir per VM
  store). The VM calls `numeric_index_fast_set_inlined` (`#[inline(always)]`, with `dense_array_set_plan`), the
  tree-walker callers share the out-of-line wrapper.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`, runs executed sequentially (parallel runs of `array-fn` showed
about 2% run-to-run variance). base = `origin/main` at `1abeab07` (binary sha256 `55c531db28c8...`), new = base plus this
change (`0d893f754d33...`). The issue's reproducer, per-op Ir = (mode - baseline) / 300,000 with the issue's baselines
(`noop` for store/array/compound, `empty` for update/load):

| op | base | new | issue target |
|---|---:|---:|---:|
| `view[i] = v`, generator body (tree-walker) | 2,069 | 129 | <= 700 |
| `view[i] = v`, `--no-bytecode` | 2,069 | 129 | <= 700 |
| `arr[i] = v`, generator body | 635 | 41 | <= 450 |
| `arr[i] = v`, `--no-bytecode` | 637 | 44 | |
| `view[i] += v`, generator body | 3,473 | 541 | <= 1,500 |
| `view[i] += v`, `--no-bytecode` | 3,477 | 546 | |
| `view[i]++`, generator body | 2,926 | 210 | <= 1,300 |
| `view[i]++`, `--no-bytecode` | 2,930 | 214 | |
| `view[i]++`, bytecode VM | 3,278 | 563 | <= 1,000 |
| `view[i] = v`, bytecode VM (control) | 223 | 224 | within 3% |
| `arr[i] = v`, bytecode VM (control) | 137 | 140 | |
| `view[i] += v`, bytecode VM (control) | 920 | 913 | |
| `s += view[i]`, generator body / VM (control) | 1,154 / 835 | 1,154 / 828 | within 3% |

The issue quoted 2,387 / 873 / 3,778 / 3,241 / 3,611 at `2ea64175`; the profile had already moved (e.g. the store was 2,069
on current main), so the gains are measured against current main, not the issue's table. `canonical_numeric_index_string`,
`number_ops::to_string`, `parse_array_index` and `to_property_key` are absent from the top of the `store-gen` and
`update-fn` per-function profiles.

JetStream drivers at `--iterations 1`, whole-process Ir:

| benchmark | base | new | delta | criterion |
|---|---:|---:|---:|---|
| sync-fs | 12,870,451,742 | 10,447,211,153 | -18.8% | >= 15% |
| async-fs | 9,889,843,234 | 8,276,601,088 | -16.3% | >= 8% |
| gaussian-blur | 31,845,170,690 | 31,321,226,203 | -1.6% | within 1% (no regression) |
| richards | 13,878,352,052 | 13,794,693,432 | -0.6% | within 1% |
| fib | 7,600,596,601 | 7,511,743,167 | -1.2% | within 1% |
| delta-blue | 9,141,792,154 | 9,094,892,115 | -0.5% | |
| Box2D | 8,490,672,921 | 8,423,750,597 | -0.8% | |

Item 4 of the issue (an exact-integer branch in `number_ops::to_string`) is not done: nothing in the acceptance criteria
needs it once the typed-array round trip is gone.

## Correctness gates

`test262-extra/numeric-index-store-and-update-semantics.js` runs each case in a plain function body and in a generator
body (the tree-walker under default flags) and covers: TypedArray wrapping/clamping, out-of-range and `-0` stores,
strict-mode no-ops, detach/shrink by the RHS or a `valueOf`, compound read-before-RHS, BigInt arrays, canonical vs.
non-canonical string keys (`"01"`, `"1e3"`, 15/16 digits, `"-0"`), Array overwrite/append/hole/`undefined`, frozen,
sealed, non-extensible, non-writable element and `length`, inherited index accessors and data properties, once-only
`ToNumeric` with a `valueOf` that truncates/freezes/redefines the array, mapped and unmapped arguments, Proxy base and
Proxy in the prototype chain, ordinary objects with prototype setters, primitive and nullish bases, key-conversion order
and expression results. It passes on the base binary and on Node 26 too (no semantic change). Unit tests cover
`canonical_numeric_index_string`.

Full test262 default and `--no-bytecode`: 99,911/99,911 each; `test262-extra/` 1,426/1,426 normal, `--no-bytecode`, and
both under `JSSE_GC_STRESS=7`; `scripts/run-custom-tests.py` 19/19; `./scripts/lint.sh` clean.
