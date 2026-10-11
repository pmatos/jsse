# BigInt arithmetic: jsse vs boa 0.22 (investigation, 2026-10-10, jsse 2b621c5 / v0.11.0)

## Summary

- The five JetStream bigint benchmarks are 2.4x-3.5x slower than boa, but **BigInt arithmetic itself is not the
  bottleneck**. Both engines use the same `num-bigint 0.5.1`. By callgrind, `eval_binary` (all BigInt/Number
  operator dispatch + the num-bigint work) is 8.7% of noble-secp256k1 and 2.3% of bigdenary; the remaining
  ~90% is generic interpreter cost (SipHash name resolution ~26%, call/construct setup ~25%, malloc/free).
  A BigInt-free numeric clone of the BigDenary workload shows the *same* ratio, 2.82x vs 2.81x for the real BigDenary (`bench_bigint_bigdenary_numeric_control.js`).
- Two things make BigInt code worse than ordinary code: (1) the bytecode compiler cannot compile BigInt
  literals (`BAIL literal`) nor `let`/`const`/`instanceof`/`throw`/`typeof`, so every hot noble/paillier/bigdenary
  function (`mod`, `invert`, `countTrailingZeros`, `modPow`, `BigDenary` ctor) runs on the tree-walker; (2) a few
  BigInt builtins/paths are 3-5x slower than boa (`toString(radix)`, literals re-parsed per evaluation).
- BigInt-specific fixes (work items B1-B3) are worth roughly 4-8% on the JetStream benchmarks and 1.5-2x on
  the literal/toString microbenchmarks. Closing the JetStream gap needs the cross-cutting items G1-G3 (compiler
  coverage, slot-resolved names instead of SipHash string lookups, call setup), which other themes own;
  the measurements below are given so those items can be prioritised with BigInt evidence.

## Gap table

loadavg1 is the box 1-minute load average read just before each repro's runs (max over the three engines'
rows); every row is <= 9.8. Median of 5 runs, `microbench.py`, engines interleaved per script.

| repro | models | jsse s | boa s | jsse/boa | jsse --no-bytecode s | jsse env-Fx s (experiment) | loadavg1 |
|---|---|---|---|---|---|---|---|
| `bigdenary_numeric_control` | shape control (no BigInt) | 3.79 | 1.34 | 2.82x | 3.89 | 3.49 | 9.8 |
| `bigdenary_real` | JetStream bigint-bigdenary, verbatim | 4.20 | 1.49 | 2.81x | 4.30 | 3.89 | 9.7 |
| `compare` | generic-cost repro | 6.40 | 1.29 | 4.95x | 6.40 | 4.70 | 8.8 |
| `compare_numeric_control` | shape control (no BigInt) | 6.03 | 0.99 | 6.12x | 6.65 | 4.71 | 7.6 |
| `convert` | BigInt<->string/Number | 2.01 | 0.84 | 2.40x | 1.96 | 1.81 | 6.6 |
| `invert` | noble `invert` (Euclid) | 1.85 | 0.37 | 4.95x | 1.86 | 1.55 | 7.2 |
| `literal` | BigInt literals in loop | 2.77 | 0.53 | 5.26x | 2.46 | 2.31 | 7.5 |
| `modpow` | 1024-bit modPow (arithmetic-dominated) | 2.26 | 1.14 | 1.98x | 2.57 | 2.16 | 7.7 |
| `mulmod256` | noble `mod(a*b)` let/const/default-param | 2.67 | 0.79 | 3.40x | 2.68 | 2.37 | 7.3 |
| `mulmod256_vm` | same, VM-compilable (var, no literals) | 1.96 | 0.68 | 2.86x | 2.02 | 1.65 | 6.6 |
| `ops_vm_add` | `+ * & >>` BigInt, VM loop | 2.83 | 0.97 | 2.92x | 5.14 | 2.42 | 7.6 |
| `ops_vm_numadd` | shape control (Numbers) | 1.92 | 0.52 | 3.72x | 3.98 | - | 6.5 |
| `shift_bitops` | bytes->bigint, `& 1n`, `>>=` | 1.12 | 0.32 | 3.54x | 1.12 | - | 5.4 |
| `small` | tiny operands (`% 10n`, `/ 2n`) | 2.17 | 0.47 | 4.66x | 2.17 | - | 5.0 |
| `tostring` | `toString(16)`/`toString()` | 1.27 | 0.37 | 3.47x | 1.12 | - | 4.2 |

Real JetStream benchmarks (1 iteration, perf-counters build for the counters, plain HEAD build for times):
bigint-noble-secp256k1 3.52x, ed25519 3.24x, paillier 2.94x, bigdenary 2.90x, bls12-381 2.36x slower than boa
(brief data). Standalone JetStream-script runs on the HEAD binary: secp 4.1 s, ed25519 4.6 s, bigdenary 4.3 s,
paillier 8.4 s, bls12-381 48 s.

Every file prints the same single line on jsse, jsse `--no-bytecode` and boa (diffed). All 15 final timings were
taken at 1-minute load <= 9.8 (first pass at load 15-37 was discarded for any row above 12 and re-run; the
second/third pass files are `rerun`/`rerun2` in the remote work dir). Timer quantisation on the box is ~0.05 s.

## Root causes (ranked by share of JetStream bigint time)

### RC1. Hot BigInt code never reaches the bytecode VM (generic, BigInt-evidenced)
- `src/interpreter/bytecode/compiler.rs:552` `compile_var_declaration` rejects every `let`/`const`
  (`Unsupported("lexical declaration")`); `:571-602` `compile_literal` handles only Number/String/Boolean/Null so a
  BigInt literal bails (`"literal"`); `:249` `binary_op` rejects `in`/`instanceof` (`"binary op"`); `throw`,
  `try`, `typeof`, object/array/function/arrow expressions, default/destructured params also bail (see
  the `statement_kind`/`expression_kind` tables at `:980-1030`).
- perf-counters build, 1 JetStream iteration (BAIL / BODY rows): secp256k1 `mod` (234,707 calls) =
  "lexical declaration", `invert` = "statement:Throw"; bls12-381 `mod` (3.31M calls, 38% of tree-walker work),
  `Fp2` = "expression:Typeof"; bigdenary `countTrailingZeros` = "literal", `BigDenary` ctor = "binary op"
  (instanceof); paillier `w`/`a` = "literal"/"expression:Typeof". secp256k1: 12.7M AST work units vs 8.7M VM ops.
- Effect when it does compile (same program, `var` + no literals, `bench_bigint_mulmod256_vm.js`):
  jsse 1.96 s vs 2.67 s for the let/const/default-param/literal version = -27%. The VM only wins ~30% because it
  still resolves every name through a hashed string lookup (RC2).
- `bench_bigint_literal.js`: replacing `10n` by a pre-built variable makes the same loop 2.2x faster
  (2.17 s -> 0.99 s) *only because* the literal forces the tree-walker (`--no-bytecode`: 2.16 s vs 1.96 s).

### RC2. Name resolution = SipHash over String keys on every variable access (generic)
- `src/interpreter/types.rs:844-848`: `Environment.bindings: HashMap<String, Binding>` (std `RandomState`,
  deliberately randomised, see the comment at :845). The VM's `LoadName`/`LoadCalleeName`/`StoreName` and the
  tree-walker's `resolve_identifier` both hash the identifier text per access.
- callgrind, noble-secp256k1: `hash_one::<&str>` 18.9% inclusive + `hash_one::<&String>` 5.9% + `<&[u8]>` 1.4%
  = ~26% of all instructions; bigdenary 7.8%+2.5%+2% = 12%+ (Sip13 `write` is the single hottest symbol in
  every profile: 11.7% secp, 9.2% bigdenary, 11.7% mulmod). An empty `for` iteration (`i < N; i++; k += 1`)
  costs 3,240 Ir in the VM, 31% of it SipHash.
- History: PR #389 (05cddd3) moved these maps to `FxHashMap` for -13% on mandreel; #393 (1945998, design note
  `docs/specs/2026-07-25-environment-binding-hashing-design.md`) reverted it to randomised SipHash because
  script-controlled identifier names must not be HashDoS-predictable, and explicitly allows only "a keyed fast
  hasher ... with an explicit HashDoS-resistance argument and benchmarks". A plain hasher swap is therefore NOT
  an acceptable proposal; compile-time slot indices (no hashing at all) or interned atoms are.
- Measured ceiling (experiment, not a proposal): I built HEAD with only `Environment.bindings` switched to
  `FxHashMap` (separate target dir, binary not shipped). Same-run medians vs unmodified HEAD: mulmod256 0.89x,
  mulmod256_vm 0.84x, invert 0.83x, literal 0.83x, ops_vm_add 0.76x, compare 0.73x, bigdenary_real 0.92x
  (i.e. -8% to -27%). So removing string hashing from variable access is worth ~10-25% on these; the rest of
  the gap is elsewhere (RC1/RC3, interpreter structure).

### RC3. Call / construct setup (generic)
- callgrind secp256k1: `bind_function_parameters` 9.2%, `instantiate_body_declarations` 6.2%,
  `construct_from_evaluated` 19.8% inclusive, `HashMap<String,Binding>::insert` 2.4% self +
  `hash_one::<&String>` 3.4% self (every call re-inserts the parameter/var names as owned `String`s into a
  fresh HashMap), `Rc<RefCell<Environment>>::drop_slow` 1.5%, `drop_elements<(String,Binding)>` 3.3% inclusive.
- bench_bigint_mulmod256 under callgrind costs ~75k Ir per loop iteration (3 `mod` calls + 2 mul + 1 add);
  64% of the program is inside `call_function_inner_impl`, 12% in `bind_function_parameters` (default param
  `b = CURVE.P` / `b = P`).

### RC4. Per-operator BigInt overhead in `eval_binary_slow` (BigInt-specific, small)
- `src/interpreter/eval.rs:1897 eval_binary` has only a Number/Number fast path; BigInt operands fall to
  `:1957 eval_binary_slow`, which runs `to_primitive` on both (clones the Arc-wrapped value), `to_numeric`
  (clones again), then `as_bigint()` (`types.rs:577`; `with_bigint` at `:614` borrows without touching the count) on both
  sides before calling `bigint_ops::*`. Relational ops go through `abstract_relational_inner` (`:1720`),
  same pattern, plus an `Arc` clone per operand.
- callgrind (BigInt `r = sa + sb` statement vs Number `r = n + m`, VM, 20k iterations):
  2,045 Ir vs 1,460 Ir => ~585 Ir of BigInt-only overhead per operator, of which the actual
  `num_bigint` add is ~350. In the mulmod loop `eval_binary_slow` is 52.8M of 464M Ir (11.4%); 67% of that is
  `num-bigint` math (`remainder` 22.0M, `multiply` 9.1M, `add` 4.2M), 33% (17.5M, 290 Ir/op) is dispatch
  overhead + `JsBigInt::new` (2 allocations: Arc box + digit Vec) + `to_numeric`.
- Wall-clock marginal cost of a BigInt operator over the same statement on Numbers (VM, 256-bit operands,
  `bench_bigint_ops_vm_add` vs `_numadd`, 4.8M operators): jsse +190 ns, boa +94 ns, i.e. the BigInt-specific
  part is 2.0x boa's, whereas the Number-only loop is 3.7x boa's (1.92 s vs 0.52 s). The BigInt-specific
  part is therefore about a quarter of the jsse/boa gap on this repro; the rest is statement-level overhead.

### RC5. BigInt literals re-parsed at every evaluation (BigInt-specific)
- `src/interpreter/eval/literals.rs:93-106`: `Literal::BigInt(String)` is parsed with
  `BigInt::parse_bytes`/`str::parse` and a fresh `Arc<BigInt>` allocated on *every* evaluation (+0.2 us for
  `10n`, +0.4 us for a 256-bit hex literal, `--no-bytecode`, 1M iterations). Combined with RC1 (the literal also
  evicts the enclosing function from the VM) this is why `bench_bigint_literal.js` is 5.3x slower than boa.
- Used by paillier (`2n`, `0n`, `1n`, `8n` in `modPow`, `bytesToBigint`, `isProbablyPrime`) and bigdenary
  (`0n`, `1n`, `10n`, `-1n`); the noble bundles build constants with `BigInt(0)`/`BigInt('0x..')` once.

### RC6. `BigInt.prototype.toString(radix)` and primitive-receiver method calls (BigInt-specific + generic)
- `bench_bigint_tostring.js`: 3.5x slower than boa (callgrind: 11.9k Ir per `a.toString(16)` call).
  Breakdown: `bigint_to_string_radix` (`builtins/bigint.rs:315`) 3.75k Ir (`to_radix_be` into a Vec<u8>, then a
  per-digit `String::push`), `JsValue::from_str` (`types.rs:506`) 2.45k Ir (`Vec<u16>::from_iter(EncodeUtf16)`, grown by
  `realloc`, 15.5% inclusive), `this_bigint_value` (`:40`) deep-clones the BigInt, and `member_get`
  (`bytecode/vm.rs:103` -> `Interpreter::to_object`, `eval.rs:1394`) allocates a throw-away wrapper object
  (`class_name = "BigInt".to_string()` + arena slot) for every method call on a primitive (0.93k Ir). The same per-call wrapper affects every method call on a string/number primitive
  (`charCodeAt`, `toFixed`, ...): cross-reference the strings theme; the fix is one place (`to_object` callers in
  `member_get`/`member_get_computed` and the tree-walker's `eval_member`/`eval_call`).
- `BigInt(number)` / `BigInt(str)` / `Number(big)` natives: the constructor body itself is 175 Ir; the 3.4k Ir per
  call is the generic native-call path (global-name lookup `BigInt` through the env chain + arg Vec +
  `call_function_ic_validated`), 3.8x boa. Paillier makes 1.4M native calls per iteration (1 `BigInt(byte)` per
  random byte).
- `BigInt.asUintN/asIntN` (`builtins/bigint.rs:246-297`) compute `1n << bits` then `%`; not used by any
  JetStream bigint benchmark and at parity in the micro (1.5 us vs 1.2 us), so not ranked.

## Proposed work items

### Experiment (RC2 ceiling)
`Environment.bindings` -> `FxHashMap` only (3 constructor sites + the field, `types.rs:847,955,981,1012`), built in
`$R/work-bigint/src` with `CARGO_TARGET_DIR=$R/work-bigint/target-fx`: results in the gap table column
"env-Fx". Upper bound for any hashing-removal work; not a deliverable because of #393.

### BigInt-specific items (owner: this theme)

**B1. `perf(interpreter): bigint-bigint fast path for binary/relational operators`**  (small, 1 PR)
- Scope: in `eval_binary` (`eval.rs:1897`) add, after the Number/Number check, a BigInt/BigInt arm using
  `left.with_bigint(|a| right.with_bigint(|b| ...))` for `+ - * / % ** << >> & | ^ < > <= >=` (division by zero,
  negative exponent, shift-amount range errors keep today's messages), so neither `to_primitive`, `to_numeric`
  nor `as_bigint()` (Arc clone+drop) runs. Do the same in `abstract_relational_inner` (`eval.rs:1720`) before
  the `to_primitive` calls when both are BigInt. Mixed BigInt/Number and objects keep the slow path.
  No rooting changes (no allocation of JS objects, no user code).
- Expected: ~290 Ir of ~2,000 per BigInt operator statement (-10..14% on `bench_bigint_ops_vm_add`), ~2-4% on
  secp256k1/ed25519/bls12-381 (`eval_binary` is 8.7% of secp256k1, half of it non-arithmetic).
- Acceptance: `bench_bigint_ops_vm_add` >= 8% faster; `bench_bigint_small` >= 5% faster; callgrind Ir for
  `r = sa + sb` <= 1,800 (HEAD 2,045); test262 `built-ins/BigInt`, `language/expressions/*bigint*`, `language/
  expressions/{addition,subtraction,multiplication,division,modulus,exponentiation,left-shift,right-shift,
  bitwise-*,less-than,...}` unchanged; add test262-extra cases for observable ordering (ToPrimitive on mixed
  operands still runs left-to-right).
- Risk: low. Must keep the Number/BigInt `TypeError` ordering (`ToNumeric(lval)` before `ToNumeric(rval)`).

**B2. `perf(interpreter): pre-parse BigInt literals once; support them in the bytecode compiler`**  (small)
- Scope: parser stores the literal as a ready `JsBigInt` (or `Arc<BigInt>`) in `Literal::BigInt` (ast.rs:567)
  instead of the source text; `eval/literals.rs:93-106` clones the Arc; `bytecode/compiler.rs:571-602` adds a
  `Constant::BigInt` and `LoadConst` path (no new opcode).
- Expected: removes the per-evaluation parse (+0.2 us small / +0.4 us 256-bit literal, ~17% of
  `bench_bigint_literal` on the tree-walker, ~3-4% of bigdenary/paillier) and the `BAIL literal` for every
  function whose only blocker is a BigInt literal (`bench_bigint_literal.js` is `var`-only, so it moves wholesale
  onto the VM: equivalent measured variant 2.17 s -> 0.99 s).
- Dependency: for real code (`let`/`const`, default params) the VM benefit only materialises together with
  G1; the tree-walker parse saving stands alone.
- Acceptance: `bench_bigint_literal` jsse <= 1.4 s (HEAD 2.77 s, ratio <= 2.7x); `BAIL literal` count 0 in
  the perf-counters report for it; test262 BigInt literal / `numeric-separator` / `Function.prototype.toString`
  literal tests unchanged (the AST keeps the source text for `toString` if it is read from the token).

**B3. `perf(bigint): allocation-free BigInt.prototype.toString(radix) and wrapper-free primitive method calls`**  (small-medium)
- Scope: (a) `builtins/bigint.rs:40` `this_bigint_value` returns a borrowed/`Arc` handle instead of deep-cloning
  the `BigInt`; (b) `bigint_to_string_radix` (`:315`) writes ASCII digits straight into a `Vec<u16>` sized
  from `bits()` and builds the `JsString` without the `String` -> `EncodeUtf16` round trip (use `to_str_radix`
  for non power-of-two radices); (c) `member_get`/`member_get_computed` (`vm.rs:103,124`) and the tree-walker
  member path look the property up on the realm's `bigint_prototype`/`string_prototype`/... directly with the
  primitive as receiver instead of calling `to_object` (spec-observable only through getter `this`, which must
  stay the primitive in strict code and be wrapped in sloppy code: keep the wrapper for accessor properties).
- Expected: `a.toString(16)` 11.9k Ir -> ~5k Ir; `bench_bigint_tostring` 3.5x -> <= 2x. JetStream impact is
  small (hex helpers are ~1% of noble time), so rank last of the BigInt items; (c) is shared with the strings theme.
- Acceptance: `bench_bigint_tostring` jsse <= 0.85 s (HEAD 1.27 s); `bench_bigint_convert` <= 1.6 s;
  test262 `built-ins/BigInt/prototype`, `language/expressions/call/*primitive*`, `annexB` string-method tests
  unchanged; sloppy/strict `this` identity tests in test262-extra for getters defined on `BigInt.prototype`.

**B4. (optional, UNVERIFIED) `perf(bigint): small-value representation for JsBigInt`**
- Idea: `Arc<BigIntRepr>` with `Small(i64)`/`Big(BigInt)`, or an inline 47-bit payload in the NaN-box using the
  pointer's low bit; arithmetic on small values uses checked i64 ops and skips the digit `Vec` (halves the
  allocations per result: today 2 malloc + 2 free per BigInt result).
- Only helps tiny-operand code (`bench_bigint_small`, bigdenary/paillier control flow); noble operands are
  256-381 bit. Estimate: <= 150-250 Ir of ~2,000 per tiny op, i.e. 5-10% on `bench_bigint_small`, ~0 on noble.
  Invasive (`types.rs` NaN-box, `with_bigint` signature, GC-free so no rooting impact). Do only after B1/B2.

### Cross-cutting items (owned by the calls / numeric / strings themes; BigInt evidence supplied for prioritisation)

**G1. `perf(bytecode): compile let/const, instanceof, throw, typeof, default params` (compiler coverage)**
- BigInt evidence: every hot noble/paillier/bigdenary function bails (RC1). Measured benefit when a
  noble-shaped `mod` loop does compile: 2.67 s -> 1.96 s (-27%, `mulmod256` -> `mulmod256_vm`).
  The cheapest subset that moves the five bigint benchmarks: `const`/`let` declarations (incl. `for (let ...)` without closure
  capture), `instanceof`, `throw`, `typeof`, default parameter initialisers, plus B2's literal.
- Acceptance on this theme's repros: `bench_bigint_mulmod256` <= 2.0 s (ratio <= 2.7x); `BAIL lexical declaration`
  / `binary op` rows disappear for `mod`, `invert`, `countTrailingZeros`, `BigDenary` in the BODY table.

**G2. `perf(interpreter): slot-resolved variable access (no per-access string hashing)`**
- BigInt evidence: 26% of secp256k1 instructions are SipHash on identifier strings; env-Fx ceiling is -8..-27%.
  Must respect #393 (no deterministic hasher on script-controlled names): compile-time (depth, slot)
  resolution in the VM, or atom-interned names with a precomputed random-keyed hash.
- Acceptance: `bench_bigint_ops_vm_numadd` ratio <= 2.5x (HEAD 3.7x); SipHash `hash_one::<&str>` < 5% of
  `bench_bigint_mulmod256_vm` Ir.

**G3. `perf(interpreter): cheaper call/construct setup`**
- BigInt evidence: `bind_function_parameters` 9.2% + `instantiate_body_declarations` 6.2% + env teardown ~5%
  of secp256k1 Ir; owned `String` keys re-inserted into a fresh `HashMap` per call; native calls such as
  `BigInt(byte)` cost ~3.4k Ir (BigInt body itself 175 Ir). Paillier makes 1.4M native calls per iteration.
- Acceptance: `bench_bigint_bigdenary_numeric_control` ratio <= 2.0x (HEAD 2.82x) -- this shape control is
  the cleanest attribution target for construct/property overhead since it contains no BigInt.

Expected combined effect on the five JetStream benchmarks (estimates from the counters above, not measured):
B1+B2+B3 ~ 4-8%; G1 ~ 15-25% on noble (the `mod`/`invert` bodies are 19-38% of tree-walker work); G2 ~ 10-25%
(env-Fx measured); G3 ~ 10-20%. Reaching boa parity needs G1-G3.


## What did NOT reproduce

- **BigInt arithmetic being slow per se.** `bench_bigint_modpow.js` (1024-bit multiply/remainder dominated):
  jsse is 2.0x boa (smallest gap of the set) and the difference is the fixed per-operator overhead (~0.4 s per 1M operators), not num-bigint (same crate).
  `bench_bigint_ops_vm_*`: BigInt-specific marginal cost per op is 1.6x boa's, while the surrounding statement
  is 3.9x.
- **BigInt comparison / equality being slower than Number comparison.** `bench_bigint_compare.js` and its
  numeric twin take the same time in jsse (6.40 s vs 6.03 s); `strict_equality` (`helpers.rs:346`) borrows both
  Arcs without cloning. The 5.0x gap vs boa on that file (6.1x for the numeric twin) is the generic lexical-declaration/tree-walker cost.
- **BigInt wrapper/GC pressure.** BigInts are `Arc` values (types.rs:393) and never hit the object arena; the
  `gc_ms` counters are 31 ms (secp), 66 ms (bigdenary), 313 ms (paillier, from 1.4M byte-wrapper strings),
  i.e. <8% of those runs.
- **BigInt vs Number mixed compare** (`n < 0`, bigdenary `base >= 0`) uses `f64_to_bigint` + cmp
  (`eval.rs:10384`): measurable only as ~1 allocation, not worth a work item.
- **asUintN/asIntN**: unused by the benchmarks, parity in micro.

## Reproduction commands

```
R=/home/igalia/pmatos/dev/bench-workspace/run-2026-10-10
# all repros, 5 runs, three engines, results json
python3 $R/microbench.py '{"jsse":["'$R'/jsse-head/target/release/jsse"],"jsse_nobc":["'$R'/jsse-head/target/release/jsse","--no-bytecode"],"boa":["'$R'/boa-v0.22-src/target/release/boa"]}' <dir with bench_bigint_*.js> out.json 5
# one repro by hand
$R/jsse-head/target/release/jsse bench_bigint_mulmod256.js ; $R/boa-v0.22-src/target/release/boa bench_bigint_mulmod256.js
# real JetStream scripts (HEAD): build with scripts/run-jetstream.py helpers, or
python3 $R/jsse-head/scripts/run-jetstream.py --engine $R/jsse-head/target/release/jsse \
  --jetstream /home/igalia/pmatos/dev/bench-workspace/JetStream --iterations 1 --timeout 120 --no-idle-gate \
  --test bigint-noble-secp256k1 --json out.json
# counters (BAIL/BODY/OP) -- perf-counters build in $R/work-bigint/target/release/jsse, stderr:
$R/work-bigint/target/release/jsse $R/work-bigint/js_bigint-noble-secp256k1.js 2>&1 >/dev/null | grep -v '^OP'
# callgrind (per-function Ir), reduced iteration counts:
valgrind --tool=callgrind --callgrind-out-file=cg.out $R/jsse-head/target/release/jsse reduced.js; callgrind_annotate [--inclusive=yes] cg.out
```

Per-op marginal costs quoted in RC4/RC6 come from 1M-iteration single-statement loops
(`$R/work-bigint/ops/op_*.js`, scratch, not delivered) run on `jsse`, `jsse --no-bytecode`, `boa`; Ir/iteration
from the same files at N=20000 under callgrind minus an empty-loop run (loop body `k += 1`: 3,240 Ir/iteration).
