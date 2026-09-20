# Plan: issue #654 — `parseInt` mis-rounds integers above 2^53 (JetStream OfflineAssembler)

## 1. Problem restated

JetStream `OfflineAssembler` prints `const TagTypeNumber = 18446462598732839000`
instead of `...840000`. The issue's hypothesis ("`Number.parseInt("ffff…", 16)`
is right, so the error is elsewhere") is correct as far as it goes, but the
error is in `parseInt` itself, on a different input than the one the reporter
checked. The benchmark lexer (`RexBench/OfflineAssembler/parser.js:233`) turns
`0xffff000000000000` into the Number `1.8446462598732841e19`; the parser later
calls `Number.parseInt(this.tokens[this.idx])` (`parser.js:556`), which
stringifies that Number to `"18446462598732840000"` and parses it **as radix 10**.
jsse's `parseInt` (`src/interpreter/builtins/mod.rs:1837-1851`) accumulates the
digits with `result = result * radix + digit` in `f64`, rounding at every step,
so a 20-digit decimal string lands one ULP (2048) low.

Reproduced on a current release binary (no JetStream needed):

```
parseInt("18446462598732840000")  → 18446462598732839000   (node: …840000)
Number.parseInt(String(0xffff000000000000)) === 0xffff000000000000  → false
```

Same defect, wider blast radius (compared against node, all currently wrong on
jsse): radix 10 above 2^53 (`"12345678901234567890"`, `"99999999999999999999"`),
every radix that is not a power of two, and even radix 2 with a sticky-bit
tie (`"1000000000000000000000000000000000000000000000000000011000000000000"`
gives …210000, correct is …220000, because of per-step double rounding).
`Number("18446462598732840000")`, `Number("0x…")`, and `BigInt` → Number are
already correctly rounded, and the number-to-string conversion is fine.

The fix is to compute the integer exactly (or in a way that rounds once) and
convert to `f64` a single time — reusing the existing exact helper
`radix_digits_to_f64` in `src/interpreter/helpers.rs:170`, which
`StringToNumber` already uses for `0x`/`0o`/`0b`.

## 2. Spec basis

- ECMA-262 §19.2.5 `parseInt ( string, radix )` (`sec-parseint-string-radix`,
  `spec/spec.html` ~line 30005), steps 14-16:
  - step 14 "Let *mathInt* be the integer value that is represented by *Z*
    in radix-*R* notation". The parenthetical relaxations are narrow: only
    (a) if *R* = 10 and *Z* has more than 20 significant digits, digits after
    the 20th may be replaced by 0, and (b) if *R* is not one of 2, 4, 8, 10, 16,
    32, *mathInt* may be an implementation-approximated integer. So for
    *R* ∈ {2, 4, 8, 16, 32} the value must be exact, and for *R* = 10 with ≤ 20
    significant digits it must be exact. `"18446462598732840000"` has exactly 20
    significant digits → exact is **required**; jsse violates the spec here.
  - step 16 "Return 𝔽(*sign* × *mathInt*)".
- §6.1.6.1 The Number Type (`sec-ecmascript-language-types-number-type`), the
  "Number value for *x*" / `𝔽(x)` definition (spec.html line ~1973): picks the
  closest Number, ties to the even significand, and a result of 2^1024 is +∞. This is the rounding contract the
  single final conversion must meet (including the `2^1024 − 2^970` overflow
  tie and the 53-bit ties).
- Sign handling (`-0`): step 15 "If *mathInt* = 0 … return −0𝔽 when *sign* = −1"
  is already satisfied by negating the `f64`; keep it.

For non-2^k radices other than 10 we choose exact correct rounding, which the
spec's "may be approximated" wording permits. That is an engine policy, not a
spec requirement, so it is tested in Rust unit tests, not in `test262-extra/`.

## 3. Files to touch

- `src/interpreter/builtins/mod.rs` — the `parseInt` native (lines ~1797-1855):
  replace the `f64` accumulation loop with "scan the longest valid-digit prefix
  *Z*, then convert *Z* once". Everything before it (ToString, TrimString,
  sign, ToInt32 radix, radix 0/16 prefix handling, NaN for radix outside
  2..=36) is spec-correct and stays byte-for-byte.
- `src/interpreter/helpers.rs` — make `radix_digits_to_f64` (line ~170)
  `pub(crate)` and harden it (see slices 2-3): u64 fast path, direct
  `str::parse::<f64>` for radix 10, guard for > 1024 significant digits. Update
  its doc comment (it currently says §7.1.4.1 only; it now also serves §19.2.5).
- `src/interpreter/tests.rs` — Rust seam tests for the non-spec-mandated
  behaviour (correct rounding for non-2^k radices), next to
  `string_to_number_seam_tests` (line ~3233).
- `test262-extra/parseInt-large-values-correct-rounding.js` — new spec-mandated
  regression (§19.2.5), test262 file pattern (see §5).
- No `docs/adr/` or `CONTEXT.md` change: no new architectural decision or
  vocabulary.
- `test262-pass.txt` is **not** touched (never rolled forward from this branch).

## 4. TDD slices

Build with `cargo build --release -j4` (memory-capped; explicit long timeout for
the compile) and run quality gates as separate commands (`./scripts/lint.sh`,
`cargo test --release`, test262 targeted runs), never `&&`-chained.

1. **Red → green: the reported case, decimal radix.**
   - Test: new `test262-extra/parseInt-large-values-correct-rounding.js`. First
     assertions only:
     `parseInt("18446462598732840000") === 0xffff000000000000`,
     `Number.parseInt` same, `parseInt("18446462598732840960")` same value,
     `parseInt(String(0xffff000000000000)) === 0xffff000000000000` (the exact
     OfflineAssembler path), `parseInt("9007199254740993") === 9007199254740992`,
     `parseInt("9007199254740995") === 9007199254740996`,
     `parseInt("12345678901234567890") === 12345678901234567000`,
     `parseInt("99999999999999999999") === 1e20`, negatives
     (`-18446462598732840000`).
     Run with `uv run python scripts/run-test262.py test262-extra/` (there is
     no dedicated test262-extra runner; the files rely on the test262 harness,
     so running the `jsse` binary on them directly fails. `run-test262.py` also
     accepts a single `.js` path — it checks `is_file()` — for a faster loop.
     The `test262/` submodule is empty in fresh workspaces; run
     `git submodule update --init --depth 1 test262` first). Must fail on the
     unfixed binary.
     Also add the prefix-handling guards for the untouched code upstream of the
     rewrite: `parseInt("0x10", 10) === 0` (prefix must **not** be stripped when
     *R* is explicitly 10: `'0'` parses, `'x'` stops the scan),
     `parseInt("0x10") === 16`, `parseInt("0x10", 16) === 16`,
     `parseInt("0x10", 0) === 16`, `parseInt("12abc") === 12`, `parseInt("abc")`
     is NaN, `parseInt("1", 37)` / `parseInt("1", 1)` are NaN.
   - Production, no allocation on the common path: in `builtins/mod.rs` find the
     **byte offset** `end` of the first character for which
     `char::to_digit(radix)` is `None` (or `s.len()`), and take `Z = &s[..end]`
     — a borrowed slice, never `.chars().take_while(..).collect::<String>()`.
     Slicing at `end` is always on a char boundary because `to_digit` accepts
     only ASCII alphanumerics (every non-digit, possibly multi-byte, char stops
     the scan and starts at a boundary). This is the same character class as the
     old `0-9a-zA-Z` match. Split the helper so `Z` is validated exactly once:
     the scan already proved every byte of `Z` is a radix-*R* digit, so give
     `helpers.rs` a prevalidated entry point (e.g. `radix_digits_to_f64_unchecked`
     / a `&str` already known-valid) that `parseInt` calls, and keep the
     validating `radix_digits_to_f64` (used by `StringToNumber`) as a thin
     wrapper that validates then delegates. The u64 accumulation is then the
     only pass over `Z` on the fast path. Empty `Z` → NaN ("If *Z* is empty,
     return NaN"). Apply `sign` afterwards; `-0` for zero magnitude with `-` sign
     falls out of `-(0.0)`.

2. **Red → green: exact for 2/4/8/16/32 incl. ties and overflow boundaries.**
   - Test (same JS file; each value cross-checked with `node`, and only where
     the spec mandates exactness — radix 2/4/8/16/32, or radix 10 with ≤ 20
     significant digits): 64-bit all-ones in radix 2/16
     (`parseInt("f".repeat(16),16) === 18446744073709552000`), the radix-2
     sticky-tie `"1000000000000000000000000000000000000000000000000000011000000000000"`
     → `73786976294838220000`, the 2^53+1 / 2^53+3 hex ties
     (`"20000000000001"`, `"20000000000003"`), `"0x"`-prefix cases with radix
     16/0/undefined, and IEEE boundaries in radix 2:
     53 ones + 971 zeros === `Number.MAX_VALUE`; 54 ones + 970 zeros
     === `Infinity` (2^1024 − 2^970 ties to 2^1024); `"1"` + 1023 zeros === 2^1023;
     `"1"` + 1024 zeros === `Infinity`; also `"9".repeat(400) === Infinity`
     (radix 10, >20 digits but any legal reading overflows) and
     `"0".repeat(2000) + "1" === 1`, `"-" + "0".repeat(30) → -0` via `Object.is`.
   - Note: most of these already pass on the unfixed binary (power-of-two
     radices only round wrongly on sticky-bit ties, which the radix-2 tie3
     case exposes); they are boundary guards for the new helper paths, not
     red tests. Only the tie3 case and the slice-1 decimals must be red first.
   - Production: covered by slice 1's helper change for the BigUint path; if a
     boundary fails, fix in `radix_digits_to_f64` (not by patching `parseInt`).
   - Do **not** assert exact values for radix-10 strings with > 20 significant
     digits or for radices outside {2,4,8,10,16,32} in this file: the spec lets
     an implementation approximate them.

3. **Red → green: helper hardening + non-mandated exactness (Rust seam tests).**
   - Test: `src/interpreter/tests.rs`, a new `parse_int_large_value_seam_tests`
     module modelled on `string_to_number_seam_tests`, driving the public
     `parseInt(...)` seam via `run_script`/`global_number`. Cases: a 30-digit
     decimal and 40-digit strings in radix 3/5/7/36 compared against a
     BigInt-derived reference computed *inside the same script*
     (`Number(BigInt(...))` for decimal; for other radices fold with `BigInt`
     multiply-add, then `Number(...)`), asserting single-rounding equality; a
     million-digit radix-7 string returns `Infinity` quickly (guards the
     quadratic BigUint blow-up); a > 1024-significant-digit string with leading
     zeros still converts correctly.
   - Production, all inside `radix_digits_to_f64` (each behaviour-preserving for
     existing callers, so `Number("0x…")` tests in `string_to_number_seam_tests`
     and `test262-extra/StringToNumber-whitespace-nondecimal-infinity.js` stay
     green):
     a. fast path — accumulate into `u64` with `checked_mul`/`checked_add`;
        if it never overflows, `value as f64` (Rust's `u64 as f64` is
        round-to-nearest-even, i.e. a single correct rounding) — keeps the
        common `parseInt("42")` path allocation-free, which matters because
        `parseInt` is hot in benchmarks;
     b. radix 10 overflow path — `digits.parse::<f64>()` directly (the string is
        already validated decimal digits; Rust's parser is correctly rounded for
        any length), skipping BigUint;
     c. other radices overflow path — existing `BigUint` → decimal string →
        `parse::<f64>()`; preceded by a guard with this exact order: strip
        leading `'0'`s; if nothing remains the value is `0.0` (so
        `"-" + "0".repeat(2000)` stays `-0`, never `Infinity`); else if more
        than 1024 significant digits remain the value is ≥ 2^1024 for every
        radix ≥ 2, so return `f64::INFINITY` without building a BigUint.
        Add `"-" + "0".repeat(2000)` → `-0` (via `Object.is`) to the JS test.

4. **Acceptance run (not a committed test).** On this host `/tmp/JetStream` is
   already checked out at `c603c04`; otherwise follow the issue's clone recipe.
   `uv run python scripts/run-jetstream.py --test OfflineAssembler --iterations 1 --timeout 120 --engine target/release/jsse --jetstream /tmp/JetStream`,
   and the same with `--bytecode` (the issue says the symptom is identical in
   both modes; `parseInt` is a single native, so one fix covers both — verify,
   do not assume). Line #42 must now match. `validate()` stops at the *first*
   differing line, so further diffs could surface after it: if the run fails on
   a different line, root-cause it separately and either fix if it is the same
   defect class or file a new issue — say which in the PR body. Record the
   before/after in the PR description.

5. **Refactor/tidy.** Re-read the `parseInt` native after slice 1: the sign /
   radix / prefix logic should read linearly with the new prefix-scan; no other
   cleanups (see §7).

## 5. Test surface

- Targeted test262 (must not regress; run before the full suite):
  - `test262/test/built-ins/parseInt/` (all `S15.1.2.2_*`, `15.1.2.2-2-1`,
    `prop-desc`, `not-a-constructor`; ~110 entries in `test262-pass.txt`)
  - `test262/test/built-ins/Number/parseInt.js` and
    `test262/test/built-ins/Number/S15.7.*` (Number.parseInt identity with the
    global; the attach code at `builtins/mod.rs:1927` is untouched)
  - `test262/test/built-ins/parseFloat/` (sanity; not modified)
  - `test262/test/language/literals/numeric/` and
    `test262/test/built-ins/Number/` (sanity for `radix_digits_to_f64` callers
    through `Number("0x…")`)
  - then the full default suite, `uv run python scripts/run-test262.py`, to
    confirm no baseline entry regresses (baseline read from
    `origin/main:test262-pass.txt`, never rewritten here).
- Not covered by test262 → new tests:
  - `test262-extra/parseInt-large-values-correct-rounding.js` — spec clause
    §19.2.5 (`sec-parseint-string-radix`) plus §6.1.6.1 rounding; header comment
    names the clauses and states the ≤ 20-significant-digit / 2^k-radix
    restriction that makes exactness mandatory, following the header style of
    `StringToNumber-whitespace-nondecimal-infinity.js`; assertions use
    `throw new Test262Error(...)` with `Object.is` for ±0. Confirm the runner
    picks it up with `uv run python scripts/run-test262.py test262-extra/`.
  - Rust seam tests in `src/interpreter/tests.rs` for engine-policy behaviour
    (exact non-2^k radices, huge-digit guard) via `cargo test --release`.
- Gates: `./scripts/lint.sh`, `cargo test --release`, `uv run python scripts/run-custom-tests.py`.

## 6. Regression risk

- Baseline (`test262-pass.txt`): `built-ins/parseInt/*` and `Number/parseInt` are
  the only entries this can move; expectation is zero regressions (no test262
  test asserts approximate results) and no new passes (test262 has no > 2^53
  parseInt assertion, which is why this was never caught).
- Shared machinery: `radix_digits_to_f64` is also the engine of
  `StringToNumber` for `0x`/`0o`/`0b` strings (`Number(...)`, unary `+`,
  arithmetic coercion, loose equality). Adding a u64 fast path and the >1024
  guard must not change any value it returns today; the existing
  `string_to_number_seam_tests` and
  `test262-extra/StringToNumber-whitespace-nondecimal-infinity.js` cover it and
  must be run.
- Performance: `parseInt` is called on hot benchmark paths (tree-walker and
  bytecode both dispatch to the same native). The u64 fast path avoids any
  BigUint/String allocation for ≤ 19 decimal digits (≤ 15 hex digits), which is
  the overwhelming majority of calls; current code does no allocation at all
  today, so the fast path must not add one. No change to `eval_expr`,
  `exec_statement`, property MOP, GC rooting, `ObjectKind`, or the bytecode
  compiler — only a native function body and a pure helper.
- Behavioural risk: a digit-prefix scan must accept exactly what the old loop
  accepted (`0-9`, `a-z`, `A-Z`, ASCII only, value < radix), and must keep
  returning NaN for an empty prefix. A non-ASCII "digit" must still stop the
  scan (`char::to_digit` is ASCII-only, matching the old `match`).
- Node-compat library harnesses (`decimal.js`, `big.js`, `bignumber.js`, …) call
  `parseInt` heavily; correct rounding can only move results toward node's.
  Not in CI; spot-check `./scripts/run-library-tests.sh decimal.js` if time
  allows (long-running: do not block the PR on `big.js`/`uglify-js`).

## 7. Out of scope (found during investigation; file as follow-up issues, do not bundle)

Verify with `gh issue list --search` that they are not already filed, then file
each with a minimal repro:

1. **Lexer: numeric literals wider than 64 bits are a SyntaxError.**
   `0xffffffffffffffffffff`, `0b` + 70 ones, `0o777777777777777777777777`,
   legacy octal `0777777777777777777777777` all throw ("Invalid octal literal" /
   "Invalid eval source") because `src/lexer.rs` (~lines 870-911) uses
   `u64::from_str_radix`. Spec: `sec-literals-numeric-literals` (MV of a
   NumericLiteral, then `𝔽()` rounding). (`Number("0x…")`/`BigInt` already handle this.)
2. **`Number.prototype.toString(radix)` for values ≥ 2^63.**
   `(0xffff000000000000).toString(16)` returns `"7fffffffffffffff.00000000000000000000"`
   instead of `"ffff000000000000"`. Spec: `sec-number.prototype.tostring` (radix
   formatting must represent the integer part exactly). Existing `test262-extra/Number-radix-formatting.js`
   only covers small values.
3. Other unrelated refactors: unifying the three separate NonDecimalIntegerLiteral
   parses (lexer / `StringToNumber` / `StringToBigInt`), moving `parseInt` out of
   the giant `setup_globals` closure, `parseFloat` prefix-scan cleanups
   (`let _ = (has_dot, has_e)`), formatting-only changes, and any change to
   `docs/perf/` reports or the JetStream driver script.

Also out of scope: any change to `test262-pass.txt`, `spec/`, `test262/`, and
any special-casing of the JetStream benchmark or of `0xffff000000000000`.

PR title (squash subject, Conventional Commits):
`fix(parseInt): round integers above 2^53 once instead of per digit (#654)`
