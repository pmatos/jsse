# #1053: return cached `typeof` result strings instead of allocating

`Interpreter::typeof_value` built a new `JsString` per evaluation (`encode_utf16().collect()`, then an `Arc<StrBuf>`
and an `Arc<StrView>`, and the matching frees on drop), although the result is one of eight constants.

## Change

- `helpers.rs`: `typeof_val` (returned `&str`) became `typeof_index` (same match order: HTMLDDA is `undefined`, a callable
  is `function`, `null` is `object`) plus `TYPEOF_NAMES: [&str; 8]` and `TYPEOF_UNDEFINED`.
- `CommonKeys.typeof_results: [JsValue; 8]`, built once in `CommonKeys::new()` with `JsValue::from_str`. `typeof_value` and the
  unresolvable-identifier arm of `eval_typeof_identifier` clone from it.
- No GC change: primitive strings are `Arc`-backed and live outside the object arena, so the table needs no root and
  serves every realm (ShadowRealm shares the interpreter). No `unsafe`.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`, 200k iterations. base = `origin/main` at `0b264d5a` (binary sha256
`34712a1dc939...`), new = this change (`d0b67a4e0739...`). Per-op = (loop - own-binary empty control loop) / 200k, where
the control is `t = x` / `x === "number"` (string var).

| repro | base Ir | new Ir | delta | per-op base | per-op new |
|---|---:|---:|---:|---:|---:|
| `t = typeof x` | 376,844,943 | 197,050,469 | -47.7% | 1,025 | 126 |
| `typeof x === "number"` | 420,458,966 | 239,661,709 | -43.0% | 1,014 | 110 |
| ctl-assign (control) | 171,831,968 | 171,847,345 | 0.0% | | |
| ctl-compare (control) | 217,646,696 | 217,659,703 | 0.0% | | |

JetStream drivers at `--iterations 1` (fib/opmix are the micro drivers), whole-process Ir:

| benchmark | base | new | delta |
|---|---:|---:|---:|
| hash-map | 20,940,923,145 | 19,212,615,062 | -8.3% |
| bigint-bigdenary | 5,854,708,002 | 5,697,255,934 | -2.7% |
| proxy-vue | 1,460,647,264 | 1,421,948,885 | -2.6% |
| bigint-noble-bls12-381 | 73,080,909,358 | 71,801,698,156 | -1.8% |
| fib | 7,425,577,815 | 7,425,603,616 | 0.0% |
| richards | 13,598,041,108 | 13,599,954,364 | +0.01% |
| opmix | 61,105,710,322 | 61,105,724,704 | 0.0% |

The remaining ~110-126 Ir per `typeof` is opcode dispatch plus an atomic refcount increment and decrement.

The "zero malloc blocks in dhat" criterion could not be re-measured: the shipped binary uses jemalloc (#1047), which
dhat does not intercept (19 blocks for every variant). The allocation is gone by construction: `typeof_value` is an
index and a `JsValue::clone`.

## Correctness gates

`test262-extra/typeof-cached-result-strings.js` (sec-typeof-operator-runtime-semantics-evaluation) asserts all eight
results, class and callable/non-callable Proxy, HTMLDDA, an undeclared identifier, a TDZ binding, and that
concatenating onto a typeof result (`+=`, `+` twice from one result) never changes later results. Full test262 default
and `--no-bytecode`: 99,911/99,911 each; `test262-extra/` 1,429/1,429 normal, `--no-bytecode`, and both under
`JSSE_GC_STRESS=7`; `scripts/run-custom-tests.py` 19/19; `./scripts/lint.sh` clean.

Not done (per the issue): `typeof x === "lit"` fusion and string-literal interning; both belong with #1059 follow-ups.
