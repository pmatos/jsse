## Codebase Patterns
- Lexer token types for numeric literals: `NumericLiteral`, `LegacyOctalLiteral`, `NonOctalDecimalLiteral`, `BigIntLiteral`
- Strict mode numeric literal rejection happens in parser (expressions.rs, declarations.rs), not lexer
- Test runner uses `test262-pass.txt` to track passing tests; 0 regressions means no previously-passing test now fails
- Use `-j 32` for test runs (machine has 128 cores, but 128 workers causes contention)
- `read_numeric_literal` receives the first character already consumed via `self.advance()` in `next_token`
- When `first == '.'` in `read_numeric_literal`, the dot is NOT detected by the `self.peek() == Some('.')` branch
- `eval_member_lhs_ref` returns a `DestructLRef` enum (Member or Private) with raw key values; `ToPropertyKey` is deferred to PutValue time per spec §13.15.5
- For destructuring, the order is: (1) evaluate target lref (base + key expression), (2) GetV/IteratorStep, (3) ToPropertyKey + PutValue
- `set_private_field` helper in eval.rs handles private field set for pre-evaluated base objects

---

## 2026-03-03 - US-001
- **What was implemented**: Fixed BigInt literal syntax validation in the parser/lexer
  - Added rejection of BigInt literals with exponent parts (`0e0n`) and decimal points (`2017.8n`, `.0000000001n`)
  - Added `NonOctalDecimalLiteral` token type to distinguish `08`/`09` from regular numeric literals, enabling strict mode rejection
  - Added post-numeric-literal check: reject when next char is an `IdentifierStart` (e.g., `3in []`)
- **Files changed**: `src/lexer.rs`, `src/parser/declarations.rs`, `src/parser/expressions.rs`, `README.md`, `test262-pass.txt`
- **Results**: 9 new passes, 0 regressions. 90,374/91,986 (98.25%)
  - BigInt literals: 118/118 (100%)
  - Numeric literals: 301/301 (100%)
- **Learnings for future iterations:**
  - The `read_numeric_literal` function has a subtle path where `first == '.'` means the dot was already consumed but the `has_dot` flag needs to be initialized to `true`
  - `NonOctalDecimalLiteral` must be handled in ALL pattern match sites where `LegacyOctalLiteral` appears (expressions.rs primary_expression, declarations.rs parse_property_name, parse_property_key_for_pattern, and object literal is_method/is_accessor checks)
  - The identifier-after-number check is best placed in `next_token` after `read_numeric_literal` returns, not inside the function itself, since `read_legacy_octal_or_decimal` is a separate path
---

## 2026-03-03 - US-002
- **What was implemented**: Fixed destructuring assignment evaluation order per spec §13.15.5
  - `eval_member_lhs_ref` now returns `DestructLRef` enum with raw key values (defers `ToPropertyKey` to PutValue time)
  - Iterator destructuring (§13.15.5.5): target lref evaluated BEFORE `IteratorStep` calls
  - Keyed destructuring (§13.15.5.6): target lref (base + key expression) evaluated BEFORE `GetV` on source
  - Private field support: `DestructLRef::Private` variant + `set_private_field` helper
  - Extracted `set_private_field` from inline code in `set_member_property_with_base`
  - `destructure_object_assignment` now uses `eval_member_lhs_ref` instead of ad-hoc base-only evaluation
- **Files changed**: `src/interpreter/eval.rs`, `README.md`, `test262-pass.txt`
- **Results**: 9 new passes, 0 regressions. 90,383/91,986 (98.26%)
  - Iterator destructuring eval order: 2/2 (both strict/sloppy)
  - Keyed destructuring eval order: 3/3 (both tests, all scenarios)
  - Private field set eval order: 2/2 (regression fix)
  - For-of destructuring: +2 new passes
- **Learnings for future iterations:**
  - Per spec, evaluating a member expression as a Reference stores the raw key value, NOT the result of `ToPropertyKey`. The conversion happens at PutValue/GetValue time.
  - When changing `eval_member_lhs_ref`, private member fields MUST still evaluate the base expression (e.g., `this.#field` needs `this` evaluation for TDZ checks)
  - The `destructure_object_assignment` had a separate hand-coded pre_base evaluation that only covered the base, not the key. Unifying via `eval_member_lhs_ref` was cleaner.
---
