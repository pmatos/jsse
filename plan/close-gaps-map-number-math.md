# Close Gaps: Map, Number, Math → 100%

**Estimated impact: ~30 new test262 passes**

## Summary

Fix remaining non-cross-realm failures in Map (0 fixable — already fixed by cross-realm merge), Number (6 scenarios), and Math (22 scenarios). These are all pure logic bugs with no architectural dependencies.

---

## Map — 403/405 (100% of fixable)

The 2 remaining failures are cross-realm tests (`proto-from-ctor-realm.js`). The 2 previously-failing tests (`callback-this-strict.js`, `getOrInsertComputed/check-callback-fn-args.js`) now pass after the cross-realm merge. **No work needed.**

---

## Number — 6 fixable scenarios (4 files)

### N1. `toExponential(undefined)` / `toExponential()` — uses max digits instead of minimal

**File:** `src/interpreter/builtins/number.rs`

**Bug:** When `fractionDigits` is `undefined` or omitted, `toExponential` should use the minimal number of digits needed to uniquely identify the number (spec step 10.b). Currently it treats `undefined` as 0 or uses maximum precision.

**Expected:** `(123.456).toExponential()` → `"1.23456e+2"`
**Actual:** `(123.456).toExponential()` → `"1.23456000000000010175e+2"`

**Fix:** In the `toExponential` implementation, check if `fractionDigits` is `undefined` (or argument count is 0). If so, use Rust's `format!("{:e}", x)` or `ryu_js` to produce the shortest representation, then format with the correct exponent notation (`e+N` not `e-N`).

**Tests:** `return-values.js` (2 scenarios), `undefined-fractiondigits.js` (2 scenarios)

### N2. `toExponential(0)` rounding — truncates instead of rounding

**Bug:** `(25).toExponential(0)` returns `"2e+1"` instead of `"3e+1"`. The value 25 = 2.5×10¹, rounded to 0 fraction digits should be 3×10¹ (round half-up).

**Fix:** The digit extraction logic needs proper rounding. When computing the single digit for `fractionDigits=0`, round the mantissa instead of truncating. Use `(x / 10^e).round()` or equivalent.

**Tests:** `return-values.js` (2 scenarios — same file as N1)

### N3. `toPrecision` exponential format — high-precision edge cases

**File:** `src/interpreter/builtins/number.rs`

**Bug:** `toPrecision` likely has precision issues at high digit counts (17-21 digits). The test checks values like `(1.2345e+27).toPrecision(18)` → `"1.23449999999999996e+27"`.

**Fix:** Ensure `toPrecision` uses `ryu_js` for full-precision digit extraction, then formats correctly. The exponential branch (spec step 10.c: `e < -6 || e >= p`) must produce exact digit strings.

**Tests:** `exponential.js` (2 scenarios)

---

## Math — 22 fixable scenarios (11 files)

### M1. `Math.hypot` — Infinity check must come before NaN check

**File:** `src/interpreter/builtins/mod.rs` (Math.hypot implementation)

**Bug:** `Math.hypot(NaN, Infinity)` returns `NaN` instead of `Infinity`. Per spec §21.3.2.18 step 4: "If any element of coerced is +∞𝔽 or -∞𝔽, return +∞𝔽." This must be checked BEFORE the NaN check (step 5).

**Current code likely:** checks for NaN first, returns NaN, never reaches infinity check.

**Fix:** Scan all coerced arguments for ±Infinity FIRST (return +∞). Then scan for NaN (return NaN). Then compute.

**Tests:** `Math.hypot_InfinityNaN.js` (2 scenarios), `Math.hypot_ToNumberErr.js` (2 scenarios — likely also about coercion order)

### M2. `Math.max` / `Math.min` — signed zero handling

**File:** `src/interpreter/builtins/mod.rs`

**Bug:** `Math.max(-0, 0)` returns `-0` instead of `+0`. `Math.min(0, -0)` returns `+0` instead of `-0`. Per spec: +0 is considered larger than -0 for both max and min.

**Fix:** In the comparison loop:
- For `max`: when values are equal and one is `-0` and other is `+0`, pick `+0`
- For `min`: when values are equal and one is `-0` and other is `+0`, pick `-0`

Check using `val == 0.0 && val.is_sign_negative()` in Rust.

**Tests:** `Math.max/zeros.js` (2 scenarios), `Math.min/zeros.js` (2 scenarios), `Math.max/Math.max_each-element-coerced.js` (2 scenarios — likely ToNumber coercion order), `Math.min/Math.min_each-element-coerced.js` (2 scenarios)

### M3. `Math.pow` — `pow(base, NaN)` when base is ±1

**File:** `src/interpreter/builtins/mod.rs` or `src/interpreter/helpers.rs`

**Bug:** `Math.pow(1, NaN)` returns `1` instead of `NaN`. Per spec §6.1.6.1.4 step 3: "If exponent is NaN, return NaN." This applies even when base is 1.

**Fix:** Check `if exponent.is_nan() { return NaN; }` BEFORE checking for base == 1. Currently the base==1 check likely comes first.

**Tests:** `applying-the-exp-operator_A1.js` (2 scenarios)

### M4. `Math.pow` — `pow(±1, ±Infinity)` should return NaN

**File:** same as M3

**Bug:** `Math.pow(1, Infinity)` returns `1` instead of `NaN`. Per spec step 10: "If abs(base) = 1 and exponent is +∞ or -∞, return NaN."

**Fix:** After the NaN checks, add: `if base.abs() == 1.0 && exponent.is_infinite() { return NaN; }`

**Tests:** `applying-the-exp-operator_A7.js` (2 scenarios), `applying-the-exp-operator_A8.js` (2 scenarios)

### M5. `Math.round` — signed zero and precision edge cases

**File:** `src/interpreter/builtins/mod.rs` or `src/interpreter/helpers.rs`

**Bug 1:** `Math.round(-0.5)` returns `-1` instead of `-0`. Per spec: "If x ≥ -0.5 and x < +0, return -0."

**Bug 2:** `Math.round(0.49999999999999994)` returns `0` instead of `1`. This is `0.5 - ε/4` which is the closest double to 0.5 that's less than 0.5. `floor(x + 0.5)` = `floor(0.99999999999999994)` = `floor(1.0)` = `1` (because `x + 0.5` rounds to exactly 1.0 in double arithmetic). The current implementation likely uses a naive approach.

**Bug 3:** Large odd integers near `1/ε` may not round correctly due to `x + 0.5` losing precision.

**Fix:** Implement spec-compliant round:
```
if x < 0 && x >= -0.5: return -0.0
let r = floor(x + 0.5)
if r == 0 && x < 0: return -0.0  // handle -0 case
return r
```
But the naive `floor(x + 0.5)` is actually wrong for some edge cases. The spec says: "the most integer-like Number value" — implement using `x.round()` in Rust which uses "round half to even", but spec wants "round half away from zero" (ties round up). Better: use `(x + 0.5).floor()` but handle the -0.5 case specially.

Actually, the correct implementation per spec:
```
if x is NaN: return NaN
if x is +0, -0, +∞, -∞: return x
if x >= -0.5 && x < 0: return -0.0
return floor(x + 0.5)
```

**Tests:** `S15.8.2.15_A6.js` (2 scenarios), `S15.8.2.15_A7.js` (2 scenarios)

---

## Implementation Order

1. **M1** — Math.hypot infinity-before-NaN (trivial, 4 scenarios)
2. **M2** — Math.max/min signed zero (small, 8 scenarios)
3. **M3+M4** — Math.pow NaN/Infinity with ±1 base (small, 6 scenarios)
4. **M5** — Math.round edge cases (medium, 4 scenarios)
5. **N1+N2** — Number.toExponential undefined/rounding (medium, 4 scenarios)
6. **N3** — Number.toPrecision high precision (small, 2 scenarios)

## Key Files

- `src/interpreter/builtins/mod.rs` — Math.hypot, Math.max, Math.min, Math.round, Math.pow
- `src/interpreter/builtins/number.rs` — Number.prototype.toExponential, toPrecision
- `src/interpreter/helpers.rs` — possibly `js_pow()` or exponentiation helper

## Verification

After each fix, run:
```bash
uv run python scripts/run-test262.py test262/test/built-ins/Math/ -j 128
uv run python scripts/run-test262.py test262/test/built-ins/Number/ -j 128
```
