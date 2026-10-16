# #1059: inline non-Number `===`/`!==`, fused `switch` case compare

`StrictEq`/`StrictNotEq` (and `CmpJumpIfFalse` carrying one) were inline only for two Numbers. Any other pair popped
both operands and went `binary_slow` -> `eval_binary` -> `bigint_binary` (dead probe) -> `eval_binary_slow` ->
`strict_equality`, and the following `JumpIfTrue` paid an out-of-line `to_boolean_val`. A `switch` case test was
`DupN 1; <selector>; StrictEq; JumpIfTrue`.

## Change

- `NanBoxedValue::strict_equal_non_number` (`types.rs`): `None` for two Numbers (IEEE rules stay with the Number path),
  `false` for different kinds (a Number against a non-Number included), identical bits `true`, Objects by id, Strings by
  content, Symbols by `id` (compared out of line in `symbol_or_bigint_equal`; a symbol rebuilt by own-key enumeration is a different `Arc` with the same id), BigInts by
  value. `strict_equality` (`helpers.rs`, used by the tree-walker, `SameValueZero`, `Array.prototype.includes`, ...) is
  now that helper plus the Number case. `with_string`/`with_symbol`/`with_bigint` are `#[inline(always)]` so the
  nested closures fold into the compare.
- `binary_arm!` (`StrictEq`/`StrictNotEq`) and `CmpJumpIfFalse` with a strict comparison take it after the Number
  check: operands are popped with `pop_value` (right, then left, LIFO against `gc_bytecode_roots`), nothing allocates
  or runs user code.
- `Op::CaseStrictEq offset`: `[..., discriminant, selector]` -> `[..., discriminant]`, jumping on `===`. `compile_switch`
  emits `<selector>; CaseStrictEq` per case; selectors still evaluate lazily and in order, a throwing selector leaves the
  discriminant exactly where it was.
- `Interpreter::to_boolean_val` is an `#[inline]` Boolean test with the object-arena probe out of line
  (`to_boolean_val_slow`), so `JumpIfFalse`/`JumpIfTrue`/`Not`/`&&`/`||` on a Boolean no longer call out. `is_htmldda`
  objects still go through the slow path.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`. base = `origin/main` at `5784ea97` (binary sha256 `a346c1cb51e4...`),
new = base plus this change (`345caacfc876...`). Repro = the issue's `repro.js` (switch Ir per 8-case iteration, net of
the N=0 run; direct loop Ir per compare, two compares per iteration).

| operands | switch base | switch new | delta | `a === b; a !== b` base | new | delta |
|---|---:|---:|---:|---:|---:|---:|
| Number (control) | 4,060 | 2,735 | -32.6% | 662 | 669 | +1.1% |
| strings (locals) | 6,143 | 3,168 | -48.4% | 973 | 790 | -18.8% |
| string literals | 5,528 | 2,541 | -54.0% | 973 | 790 | -18.8% |
| Symbols | 5,839 | 2,982 | -48.9% | 938 | 738 | -21.3% |
| objects | 6,198 | 3,065 | -50.6% | 981 | 785 | -20.0% |
| mixed tags | 5,737 | 2,808 | -51.1% | 941 | 763 | -18.9% |

The Number direct loop is the only row that moved the wrong way: +7 Ir per compare in every build of this change,
including builds that leave the Number arm byte-identical in source (an out-of-line strict helper gave +17). It is
code layout of `run_chunk_inner`, not extra work on the Number path.

JetStream drivers at `--iterations 1`, whole-process Ir:

| benchmark | base | new | delta |
|---|---:|---:|---:|
| Air | 2,751,217,125 | 2,401,750,600 | -12.7% |
| hash-map | 22,337,705,990 | 20,998,547,187 | -6.0% |
| proxy-mobx | 782,569,966 | 760,465,379 | -2.8% |
| Babylon | 3,466,135,005 | 3,397,469,558 | -2.0% |
| fib | 7,515,772,271 | 7,425,572,871 | -1.2% |
| Box2D | 8,296,152,067 | 8,237,207,639 | -0.7% |
| richards | 13,675,239,099 | 13,598,433,175 | -0.6% |
| splay | 10,467,163,823 | 10,430,969,212 | -0.3% |
| bigint-noble-ed25519 | 7,858,884,522 | 7,844,951,127 | -0.2% |
| gaussian-blur | 31,362,316,548 | 31,302,467,906 | -0.2% |
| delta-blue | 9,005,883,018 | 8,999,112,847 | -0.1% |

## Correctness gates

`test262-extra/strict-equality-and-switch-non-number-operands.js` runs each case as an expression, an `if` condition and
a `switch` case test, in a plain function (bytecode) and a generator body (tree-walker): rebuilt Symbols
(`getOwnPropertySymbols`, `Reflect.ownKeys`, `Symbol.for`, well-known) against the originals, equal strings from
distinct allocations and from substring/append views, NaN/`-0`, BigInt equal/unequal and against Number, `null`/
`undefined`/booleans, objects with `valueOf`/`toString` that must not run, `$262.IsHTMLDDA` as a falsy condition that
still compares by identity, selector order and fall-through with a throwing selector, and object operands under
`$262.gc()` pressure. It passes on the base binary too (no semantic change). Full test262 default and `--no-bytecode`:
99,911/99,911 each; `test262-extra/` 1,428/1,428 normal, `--no-bytecode`, and both under `JSSE_GC_STRESS=7`, and with
the `release-checked` binary in both modes; `cargo test --lib` 1,122 passed; `scripts/run-custom-tests.py` 19/19;
`./scripts/lint.sh` clean.
