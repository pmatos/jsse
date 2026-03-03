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

## 2026-03-03 - US-003
- **What was implemented**: Fixed `with` statement scope semantics — four categories of fixes
  1. **PutValue unresolvable refs in sloppy mode (§6.2.5.6)**: Added `set_global_implicit` helper that sets on `self.realm().global_env` instead of `Environment::find_var_scope()` (which returned nearest function scope, not global). Fixed both `IdentifierRef::SpecificEnv` and `IdentifierRef::Binding` Unresolvable branches.
  2. **With statement completion value (§14.11.2)**: Added UpdateEmpty(C, undefined) after executing with body — converts `Completion::Empty` to `Completion::Normal(JsValue::Undefined)`.
  3. **Parser declaration restrictions (§14.11.1)**: Added early error checks rejecting `function` and `class` keywords in with body position, plus `IsLabelledFunction` post-parse check.
  4. **Reflect.set proxy trap propagation (§10.1.9.2)**: When OrdinarySetWithOwnDescriptor receiver is a proxy, use `proxy_get_own_property_descriptor` and `proxy_define_own_property` instead of direct object access.
  5. **Symbol.unscopables cleanup**: Removed spurious fallback `"[Symbol.unscopables]"` lookup in `check_unscopables_dynamic`.
- **Files changed**: `src/interpreter/eval.rs`, `src/interpreter/exec.rs`, `src/parser/statements.rs`, `src/interpreter/builtins/mod.rs`, `test262-pass.txt`
- **Results**: 37 new passes, 0 regressions. 90,420/91,986 (98.30%)
  - With statement tests: 182/182 (100%)
  - Additional passes from `set_global_implicit` fix affecting sloppy-mode implicit global creation
- **Learnings for future iterations:**
  - `Environment::find_var_scope()` returns the nearest function scope, NOT the global scope. For PutValue on unresolvable references per §6.2.5.6, must use `self.realm().global_env` directly.
  - `check_unscopables_dynamic` had a fallback lookup for `"[Symbol.unscopables]"` that caused extra proxy trap invocations — only the `"Symbol(Symbol.unscopables)"` key should be used.
  - `Reflect.set` must propagate through proxy traps when receiver is a proxy — `proxy_get_own_property_descriptor` returns `Result<JsValue, JsValue>` (not `Option<PropertyDescriptor>`), and `proxy_define_own_property` takes `(u64, String, &JsValue)`.
---
