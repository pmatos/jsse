# Plan: fix `Number.prototype.toString(radix)` for values ≥ 2^63 (#678)

## 1. Problem restated

`format_number_radix` (`src/interpreter/builtins/number.rs:3`), the helper behind
`Number.prototype.toString(radix)` for `radix !== 10`, computes the integer part of
the number with `x.trunc() as i64`. For `|x| >= 2^63` this cast saturates to
`i64::MAX`/`i64::MIN` instead of producing the true value, so e.g.
`(0xffff000000000000).toString(16)` returns `"7fffffffffffffff.00000000000000000000"`
instead of `"ffff000000000000"`. The bogus fractional suffix is a second-order
symptom: `frac_part` is then computed as `x - (saturated_int_part as f64)`, which is
nowhere near zero, so the fractional-digit loop runs and emits garbage/zero digits.
The fix must produce the exact integer-part digits for any finite `x` up to
`Number.MAX_VALUE` (~1.7976931348623157e308), not just the `i64` range.

## 2. Spec basis

- `spec/spec.html`, `sec-number.prototype.tostring` — **Number.prototype.toString ( [ radix ] )**.
  Step 5 delegates to `Number::toString(x, radixMV)` after validating `radixMV` is in
  `[2, 36]`.
- `spec/spec.html`, `sec-numeric-types-number-tostring` (`oldids="sec-tostring-applied-to-the-number-type"`)
  — **Number::toString ( x, radix )**. This is the operation that's actually broken:
  - "The representation of numbers with magnitude greater than or equal to 1 never
    includes leading zeroes" — the saturated output violates this by construction
    (it's not even a representation of `x`).
  - Step 5 requires integers `n, k, s` with `k` as small as possible such that
    `𝔽(s × radix^(n-k))` is `x` — i.e. the digits must reconstruct `x` exactly
    (mathematical equality after the `𝔽` rounding), not an approximation.
  - The step-5 prose notes "the least significant digit of `s` is not necessarily
    uniquely determined by these criteria" — relevant to radix 36 test design below
    (§5), not to the fix itself.

No JS syntax changes; this is a pure semantics bug in an existing, spec-mandated
conversion.

## 3. Files to touch

- `src/interpreter/builtins/number.rs` — fix `format_number_radix`; add a
  `#[cfg(test)]` unit-test module exercising it directly.
- `test262-extra/Number-radix-formatting.js` — extend with regression assertions
  (existing file already covers small-value radix formatting for this spec clause).

No changes to `src/interpreter/helpers.rs` (`format_radix` keeps its current `i64`
signature and stays the fast path for magnitudes that fit) or to
`src/interpreter/builtins/bigint.rs` (`f64_to_bigint` is reused read-only — see §4).
No `docs/adr/` entry: this is a bug fix with no new architectural decision, and no
`CONTEXT.md` vocabulary change.

## 4. TDD slices

1. **RED — test262-extra, issue repro.** Append to
   `test262-extra/Number-radix-formatting.js`:
   `(0xffff000000000000).toString(16) !== "ffff000000000000"` → throw
   `Test262Error`. Run
   `uv run python scripts/run-test262.py test262-extra/Number-radix-formatting.js`
   — fails against current code (produces the saturated/garbage string from §1).

2. **GREEN — production fix.** In `format_number_radix`:
   - Compute `let int_f = x.trunc();` and `let frac_part = x - int_f;` directly in
     `f64`, with no intermediate `i64` round-trip. This alone fixes the bogus
     fractional suffix: `frac_part` is now always exactly `0.0` for `int_f >= 2^52`
     or so, since a whole-number `f64` minus its own truncation is exact.
   - Render `int_f` to the target radix by magnitude:
     - `int_f < 9223372036854775808.0` (i.e. `< 2^63`): unchanged existing fast
       path, `format_radix(int_f as i64, radix)` — this cast is exact for every
       `f64` below `2^63`, so behavior for all currently-passing cases is bit-for-bit
       unchanged.
     - otherwise (up to `Number::MAX_VALUE`): `f64_to_bigint(int_f).to_str_radix(radix)`,
       reusing the mantissa/exponent decomposition already in
       `src/interpreter/builtins/bigint.rs` (backs `BigInt(number)` /
       `NumberToBigInt` today) — read-only reuse, add
       `use super::bigint::f64_to_bigint;` to `number.rs`. `BigInt::to_str_radix`
       (num-bigint 0.5.1) accepts radix `2..=36` and emits lowercase `a`-`z`,
       matching the spec's required digit alphabet.
   - The two branches cannot disagree with each other: both are exact for their
     respective domains, and the boundary (`2^63`) is the same threshold
     `f64_to_bigint` itself uses internally.
   - Re-run slice 1's test — green.

3. **Boundary coverage, power-of-two radixes.** Extend
   `test262-extra/Number-radix-formatting.js` with exact-string assertions at the
   `2^63` and `2^64` boundaries for radix 2 and 16, e.g.
   `(2**63).toString(16) === "8000000000000000"`,
   `(2**63).toString(2) === "1" + "0".repeat(63)`,
   `(2**64).toString(16) === "10000000000000000"`. For `Number.MAX_VALUE` (whose
   hex/binary strings are hundreds of characters), assert `.length` (1024 for radix
   2, 256 for radix 16 — `MAX_VALUE` has exactly 1024 significant bits) plus a
   round-trip check via native `BigInt`: `BigInt("0b" + s) === BigInt(Number.MAX_VALUE)`
   / `BigInt("0x" + s) === BigInt(Number.MAX_VALUE)`, rather than hand-copying a
   giant literal. These are uncontroversial: power-of-two radixes have no digit
   ambiguity (each digit is a fixed bit-group of the exact value), so the exact
   expansion is provably both correct and minimal.

4. **Boundary coverage, radix 36 — round-trip property, not a literal.** Add the
   same three magnitudes (`2^63`, `2^64`, `Number.MAX_VALUE`) for radix 36, but
   assert a *property* instead of a hardcoded string: decode the produced string
   back to a `BigInt` with a small manual base-36 decoder written inline in the
   test (`reduce` over the chars via `parseInt(ch, 36)`), then assert
   `Number(decoded) === original`. Rationale to put in a one-line comment: per the
   spec note quoted in §2, once a magnitude's ULP exceeds 1 (true for everything
   `>= 2^53`), multiple digit strings can validly satisfy step 5 for a
   non-power-of-two radix, so no single hardcoded string is "the" spec-mandated
   answer. This was verified empirically, not just theoretically: Node's own
   `(0xffff000000000000).toString(36)` is `"3w5b996cn4k00"`, which decodes to
   `18446462598732842304n` — a value whose nearest `f64` is
   `18446462598732843008` (`0xffff000000000800`), **not** the original
   `0xffff000000000000`. I.e. Node's own radix-36 output for this exact case does
   not satisfy the spec's round-trip requirement, so it must not be copied into
   `test262-extra/` as ground truth (authority order: spec, then test262, then
   Node — Node loses here). jsse's fix computes the literal exact integer, which
   trivially satisfies the round-trip identity with equality, so the property-based
   assertion is expected to pass immediately once slice 2 lands.

5. **Fast regression net.** Add `#[cfg(test)] mod tests` to
   `src/interpreter/builtins/number.rs` calling `format_number_radix` directly for
   `0xffff000000000000u64 as f64`, `2f64.powi(63)`, `2f64.powi(64)` at radix 2 and
   16, asserting the same exact strings as slice 3. This pins the fix at the unit
   level (fast, no parser/interpreter startup) so a future refactor of this hot
   path regresses immediately in `cargo test`.

## 5. Test surface

- `uv run python scripts/run-test262.py test262/test/built-ins/Number/prototype/toString/`
  — targeted run. Every existing file here only exercises values `< 36`
  (`a-z.js` iterates `i < radix <= 36`; the `S15.7.4.2_*` and
  `numeric-literal-tostring-radix-*` files use small literals), so **no flips are
  expected**; this run exists to confirm that claim rather than to catch a fix.
- `uv run python scripts/run-test262.py test262-extra/Number-radix-formatting.js`
  — the real coverage for this issue (no dedicated test262-extra runner exists;
  pass the file path directly).
- `cargo test` (plain, not `--release` — matches this repo's guidance for
  iterating on `.rs` edits) for the new unit-test module in slice 5.
- `cargo build --release` + the two `run-test262.py` invocations above as the
  final gate before considering the fix done.
- Not required, but worth a spot-check given §6: `./scripts/run-library-tests.sh uuid`
  (its `crypto.getRandomValues`-backed v4 UUIDs format random bytes via
  `.toString(16)`, but always well within `i64` range, so no flip is expected —
  this is a belt-and-suspenders check, not a gate).

## 6. Regression risk

- The fix is gated by magnitude (`int_f < 2^63` vs. not): every previously-passing
  case takes the exact same code path (`format_radix(int_f as i64, radix)`) with
  the exact same input it always got, so `test262-pass.txt`-tracked behavior for
  all in-range values is bit-for-bit unchanged. The new `BigInt` path is only
  reached for `|n| >= 2^63`, which no current `test262/test/built-ins/Number/`
  test exercises (§5).
- Library harnesses that call `.toString(16)` — `uuid` (random byte formatting),
  `prismjs`/`highlight.js` (syntax highlighting, no huge-number formatting) — never
  format numbers anywhere near `2^63`, so none should see a count change.
- No involvement of the tree-walker hot paths (`eval_expr`/`exec_statement`), the
  property MOP (`property.rs`), GC rooting/`gc_safepoint()`, the exhaustive
  `ObjectKind` matches, or the bytecode fast path: `Number.prototype.toString` is a
  native Rust closure (see `number.rs` around line 421) invoked identically
  regardless of which execution path called it, and the change is entirely inside
  that closure's own helper.
- `f64_to_bigint` (`bigint.rs`) is not modified, only called — zero risk to
  existing `BigInt(number)` / `NumberToBigInt` behavior.
- `format_radix` (`helpers.rs`) keeps its current `i64` signature and its one call
  site — no other code depends on it, so no shared-machinery risk.

## 7. Out of scope

- **Shortest round-trip ("k-minimal") digit selection for non-power-of-two radixes**
  at magnitudes `>= 2^53` (radix 3, 5, 6, 7, 9, 11–36 except powers of two — 10 is
  handled by the separate decimal algorithm). jsse always emits the exact integer
  expansion, which is spec-valid (§2) but not necessarily what another conformant
  engine would print, once ULP exceeds 1 digit. This is a pre-existing property of
  jsse's exact-conversion strategy — already true today for e.g. `(2**60).toString(36)`
  in the untouched `< 2^63` fast path — not something this fix introduces. A true
  "shortest valid digit string" search (V8-style radix dtoa) is a much larger,
  separate undertaking; file a follow-up issue rather than bundling it here.
- The fractional-digit loop in `format_number_radix` (the 20-iteration cap and
  `1e-10` cutoff) is untouched: it's unreachable for this issue's values, since
  every `f64` with `|x| >= 2^53` is already a whole number, so `frac_part` is
  always exactly `0.0` and the loop never runs.
- No consolidation of `Number.prototype.toString`'s radix formatting with
  `BigInt.prototype.toString`'s, beyond the read-only reuse of `f64_to_bigint`
  described in slice 2.
- No `test262-pass.txt` baseline update (that's a `main`-branch-only operation).
- No cleanup of `format_radix`'s embedded negative-number branch, which is already
  unreachable from its sole call site (the caller pre-normalizes to `x.abs()`) —
  it's a generically-correct small utility, not new dead code, and touching it is
  unrelated refactoring.
