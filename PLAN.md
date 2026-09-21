# Plan: issue #677 — numeric literals wider than 64 bits are a SyntaxError

## 1. Problem restated

`src/lexer.rs` converts the digits of a non-decimal Number literal with
`u64::from_str_radix`, so any hex / octal / binary / legacy-octal literal whose
value exceeds `u64::MAX` fails with `Invalid hex|octal|binary literal` and the
whole script is rejected. Per spec such a literal is valid and evaluates to the
exact integer rounded once to the nearest Number (round-to-nearest, ties-to-even;
values at or above 2^1024 − 2^970 become `+∞`). Four call sites are affected:
`read_hex_literal`, `read_octal_literal`, `read_binary_literal`
(`src/lexer.rs:868-955`) and `read_legacy_octal_or_decimal` (`src/lexer.rs:909-912`).
`0x…n` BigInt literals and decimal literals already work and are untouched.

## 2. Spec basis

ECMA-262 (`spec/spec.html`):

- `sec-literals-numeric-literals` (§12.9.3 Numeric Literals) — grammar for
  `NonDecimalIntegerLiteral` (`0x`/`0o`/`0b` + `HexDigits`/`OctalDigits`/`BinaryDigits`,
  with `NumericLiteralSeparator`) and `LegacyOctalIntegerLiteral`. The grammar
  places no upper bound on digit count; there is no early error for size.
- `sec-static-semantics-mv` (Static Semantics: MV) — the MV of a digit run is the
  exact mathematical integer (`MV(digits) × radix + MV(digit)`), separators excluded.
- `sec-numericvalue` (Static Semantics: NumericValue) —
  `NumericLiteral :: NonDecimalIntegerLiteral` and
  `NumericLiteral :: LegacyOctalIntegerLiteral` both return `𝔽(MV)`.
- §6.1.6.1 The Number Type (`𝔽(x)`, "the Number value for x") — round to nearest,
  ties to even, overflow to `+∞`. This is why a 256-hex-digit literal is `Infinity`,
  not a SyntaxError.
- `sec-numeric-literals-early-errors` — legacy-octal literals are still a SyntaxError
  in strict code; unchanged and must stay so.

## 3. Files to touch

- `src/lexer.rs` — replace the four `u64::from_str_radix(...)` conversions with a
  shared conversion through `crate::interpreter::prevalidated_radix_digits_to_f64`.
  Path matters: `mod helpers;` is private, but `src/interpreter/mod.rs:21` has
  `pub(crate) use helpers::*;`, so the function is reachable from the lexer at the
  `interpreter` root as a `pub(crate)` item — **no visibility change needed**, and
  `crate::interpreter::helpers::…` would *not* compile from `lexer.rs`. `lexer.rs`
  already imports from `crate::interpreter::` (lines 427/526), so no new layering
  direction. Add lexer unit tests in the existing
  `#[cfg(test)] mod tests` (`src/lexer.rs:1595`).
- `test262-extra/numeric-literal-nondecimal-wider-than-64-bits.js` — new
  end-to-end test (see §5).
- No `docs/`, `CONTEXT.md` or ADR changes: no new vocabulary or architectural decision.
- `src/interpreter/helpers.rs` — no functional change (function reused as-is via the
  `pub(crate) use helpers::*;` re-export); optionally extend the "Shared by …" comment
  to mention the lexer.

Design of the lexer change: add one private lexer helper (e.g.
`fn radix_literal_value(&self, digits: &str, radix: u32, what: &str) -> Result<f64, LexError>`)
that (a) returns the existing `Invalid hex|octal|binary literal` error when `digits`
is empty, and (b) otherwise calls `prevalidated_radix_digits_to_f64`. The emptiness
check must stay in the lexer: the helper `debug_assert`s non-empty input and `0x` /
`0b` / `0o` / `0xg` with no digits must remain a SyntaxError. All other invalid input
is already excluded because `read_digits_with_separators` only accepts radix digits,
so the "prevalidated" precondition holds; separators are stripped before the call
(existing `filter(|&c| c != '_')`). Legacy octal passes `&s[1..]` (non-empty, all `0-7`
by construction of the `is_octal` branch). Keep error message strings as they are.

## 4. TDD slices

Each slice: write the failing test first, run it (`cargo test --release --lib lexer`),
then make it pass. Run fmt/clippy hooks per repo (`./scripts/lint.sh`); quality gates
as separate commands, not `&&`-chained.

1. **Hex, wide (red → green).** In `src/lexer.rs` `mod tests`, add tests using the
   existing `lex()` helper. Genuinely red today (value > `u64::MAX`):
   `lex("0xffffffffffffffffffff")` → `[NumericLiteral(1.2089258196146292e24), Eof]`
   (write the expectation as `2f64.powi(80)`); `0x10000000000000000` → `2^64`;
   `0x1_0000_0000_0000_0000` → `2^64`; `0x` + 256 `f` → `+∞`; `0x` + 255 `f` → `2^1020`.
   Regression guards, already green on `main` (they fit `u64`; do not count them as
   the red step): `0xffffffffffffffff` → `2^64`; `0x` + 64 zeros + `ff` → `255`;
   `lex("0x")` and `lex("0xg")` remain errors.
   Production: add the shared helper and use it in `read_hex_literal`.
2. **Round-to-nearest-even tie.** Test `0x2` + 12×`0` + `1` + 16×`0` (= 2^117 + 2^64,
   exact tie) → `2^117`, and the same with a trailing `1` → `2^117 + 2^65`. Guards
   against double-rounding via an intermediate f64 accumulation. Passes with the
   slice-1 implementation (helper rounds once from the exact decimal string) — this
   slice exists to pin that behaviour, not to add code.
3. **Octal.** `0o777777777777777777777777` → `2^72` (24 sevens = 2^72 − 1, rounds up;
   write as `2f64.powi(72)`); `0o` + 342×`7` → `+∞`; `lex("0o")` still errors (guard). Production: switch `read_octal_literal`.
4. **Binary.** `0b` + 70×`1` → `2^70` (= 2^70 − 1 rounded; `2f64.powi(70)`); `0b` +
   1023×`1` → `2^1023`; `0b` + 1024×`1` → `+∞`; `lex("0b")` still errors (guard). Production: switch
   `read_binary_literal`.
5. **Legacy octal.** `0777777777777777777777777` (24 sevens) → `Token::LegacyOctalLiteral(2^72)`
   (token kind must stay `LegacyOctalLiteral`, since strict-mode rejection keys off it);
   `0` + 400×`7` → `LegacyOctalLiteral(+∞)`; `0` followed by digits containing `8`/`9`
   still yields `NonOctalDecimalLiteral` (unchanged). Production: switch
   `read_legacy_octal_or_decimal`.
6. **End-to-end JS (test262-extra).** Add the JS test described in §5; run it through
   the real binary to confirm the parser/interpreter/bytecode paths accept the
   `Token::NumericLiteral(f64)` unchanged (they take an `f64`, so no change is
   expected). Cross-check every expected value against `node`.
7. **Cleanup pass.** Confirm no remaining `u64::from_str_radix` on literal digits in
   `src/lexer.rs` (`grep -n from_str_radix src/lexer.rs`), run `./scripts/lint.sh`.

## 5. Test surface

Targeted test262 directories (require `git submodule update --init --depth 1 test262`
in this workspace — the submodules are empty in fresh workspaces):

- `test262/test/language/literals/numeric/` (binary/octal/legacy-octal/non-octal-decimal,
  invalid-digit/truncated tests — guards that `0x`, `0b`, `0o`, `0b2`, `0o8` stay errors)
- `test262/test/language/literals/bigint/` (BigInt path shares the readers; must not regress)
- `test262/test/language/literals/numeric/numeric-separators/`
- `test262/test/language/expressions/` subsets that use non-decimal literals
  (`bitwise-*`, `strict-equals`, `does-not-equals`, `less-than-or-equal`, …) and
  `test262/test/built-ins/Number/`, `built-ins/parseInt/`.
- Then the full run: `uv run python scripts/run-test262.py` (no baseline update).
- Also `cargo test --release` and `uv run python scripts/run-custom-tests.py`.

No test262 file appears to exercise a >64-bit non-decimal Number literal, so this
behaviour needs its own test in `test262-extra/` (run with
`uv run python scripts/run-test262.py test262-extra/numeric-literal-nondecimal-wider-than-64-bits.js`):

- Header comment naming the spec clauses: `sec-numericvalue`,
  `sec-static-semantics-mv`, `sec-literals-numeric-literals`, following
  `test262-extra/StringToNumber-whitespace-nondecimal-infinity.js` (same
  `assertEq`-with-`Test262Error` style, `Object.is`/`1/x` for ±0).
- Positive: the four examples from the issue, expressed as `Math.pow(2, n)`
  (each `2^n − 1` rounds up to `2^n`): `0xffffffffffffffffffff === Math.pow(2, 80)`,
  70-ones binary `=== Math.pow(2, 70)`, `0o777777777777777777777777 === Math.pow(2, 72)`,
  `0777777777777777777777777 === Math.pow(2, 72)`, both as source literals and inside
  `eval`; exact-tie round-half-even cases from slice 2; separators;
  overflow to `Infinity` for 256 hex digits / 342 octal digits / 1024 binary digits;
  literal equals `Number("0x…")` for the same digits (consistency with
  `radix_digits_to_f64`; use `Number`, not `parseInt`, which takes prefix-less digits
  plus an explicit radix and treats `"0o17"` as `0`).
- Negative (via `assert.throws(SyntaxError, () => eval(...))`, `includes` header
  as in other `test262-extra` files): `0x`, `0b`, `0o` with no digits; wide legacy
  octal in strict mode (`"use strict"; 0777777777777777777777777`) is a SyntaxError —
  this is the one assertion whose mechanism changes (today the lexer rejects it; after
  the fix the parser must reject it via `sec-numeric-literals-early-errors`), so it
  proves `Token::LegacyOctalLiteral` is preserved and must live in this file, not only
  in a lexer unit test; wide separator misuse (`0x1__0…`, trailing `_`) remains a
  SyntaxError.
- BigInt regression: `0xffffffffffffffffffffn === 1208925819614629174706175n`.

## 6. Regression risk

- `test262-pass.txt`: expected to be unchanged or only gain; nothing currently passing
  should depend on a >64-bit literal being a SyntaxError. The baseline is read from
  `origin/main` and is **not** rewritten in this PR.
- Shared machinery leaned on: only `prevalidated_radix_digits_to_f64` (also used by
  `parseInt` in `builtins/mod.rs:1866` and `StringToNumber`). Behaviour for ≤64-bit
  digit runs is identical (`u64::from_str_radix` → `as f64` fast path inside the
  helper), so hot-path numerics are unchanged.
- Not touched: tree-walker `eval_expr`/`exec_statement`, property MOP, GC rooting,
  `ObjectKind`, bytecode compiler — they consume `Token::NumericLiteral(f64)` /
  `LegacyOctalLiteral(f64)` unchanged.
- Perf: the lexer conversion adds one function call per non-decimal literal (plus a
  debug-build-only `debug_assert`); negligible. A `BigUint` is built only for literals
  that exceed `u64::MAX` and are below the ≥1024-bit `+∞` short-circuit.
- Node-compat library harnesses (`scripts/run-library-tests.sh`): libraries with
  large hex constants previously failing to lex would newly parse; not part of the gate
  for this change, no library expected to regress.
- Debug-assert hazard: never pass empty digits to the helper (the reason the
  emptiness check stays in the lexer).

## 7. Out of scope

- Moving `prevalidated_radix_digits_to_f64` to a neutral numeric module to remove the
  `lexer → interpreter::helpers` dependency (precedent already exists at
  `lexer.rs:427`); a separate refactor.
- Any change to BigInt literal handling, decimal literal parsing, numeric-separator
  logic, or the "numeric literal immediately followed by IdentifierStart" check.
- Changing error message text or lexer error locations.
- Any `test262-pass.txt` baseline update; formatting or unrelated cleanups.

PR title (squash subject): `fix(lexer): round non-decimal numeric literals wider than 64 bits`
