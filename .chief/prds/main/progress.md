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

## 2026-03-03 - US-004
- **What was implemented**: Fixed generator `.return()` and `.throw()` control flow for both sync and async generators
  1. **Execution state checks (§27.5.3.3, §27.5.3.4)**: Added `StateMachineExecutionState` matching — Executing→TypeError, Completed→immediate return/throw, SuspendedStart→mark completed + immediate
  2. **Try-stack walking**: Fixed to iterate from innermost to outermost (`for i in (0..try_stack.len()).rev()`) instead of only checking `.last()`. Both return and throw paths now correctly find catch/finally blocks.
  3. **TryExit pending_exception handling**: Both sync and async TryExit state terminators now check `pending_exception.take()` and re-throw through outer try blocks or reject promise.
  4. **Yield save point preservation**: `pending_exception` and `pending_return` now preserved across yield save points (was being reset to None).
  5. **Async generator return promise unwrapping**: New `async_generator_await_return` helper using `Promise.resolve(value).then(onFulfilled, onRejected)` chaining for proper async return value handling.
  6. **Async generator throw try_stack walking**: Was completely missing — async `generator_throw` state machine now walks try_stack like sync version.
  7. **PromiseResolve check for broken constructors**: Async generator return checks `.constructor` getter via `promise_resolve_with_constructor` and routes to throw path on error.
  8. **Function length fix**: Generator `.next()` and `.return()` functions changed from length 0 to length 1.
- **Files changed**: `src/interpreter/eval.rs`, `src/interpreter/types.rs`, `src/interpreter/builtins/iterators.rs`, `src/interpreter/builtins/promise.rs`, `test262-pass.txt`
- **Results**: 38 new passes, 0 regressions. 90,458/91,986 (98.34%)
  - GeneratorPrototype: 49/61 → 122/122 (100%)
  - AsyncGeneratorPrototype: 64/96 → 84/96 (88%)
  - Remaining 12 async failures are all request-queue-* tests (different feature: async generator request queue)
- **Learnings for future iterations:**
  - State machine generators walk `try_stack` from end (innermost) to beginning (outermost) — `.last()` only checks the innermost block but misses nested try-finally
  - `async_generator_await_return` needs promise chaining because `await_value` is synchronous and can't handle deferred promise resolution
  - `promise_then` and `promise_resolve_with_constructor` in promise.rs needed `pub(crate)` visibility for use from eval.rs
  - After PromiseResolve, check promise state: Fulfilled→use value, Rejected→throw, Pending→use deferred chaining
---

## 2026-03-03 - US-005
- **What was implemented**: Fixed yield* delegation error routing, AsyncFromSyncIterator rewrite, for-await-of close fix
  1. **yield* delegation error routing (§14.4.14)**: Errors from `IteratorComplete`, `IteratorValue`, and `Call(iterator.throw/return)` in `generator_return_state_machine` and `generator_throw_state_machine` now route through the generator's `try_stack` via recursive `generator_throw_state_machine` call instead of `Completion::Throw` directly.
  2. **AsyncFromSyncIterator rewrite (§27.1.2)**: New `async_from_sync_continuation` helper implements §27.1.2.4 with proper PromiseResolve (via `promise_resolve_with_constructor`), onFulfilled/onRejected `.then()` chaining, `closeOnRejection` parameter (true for next, false for return/throw), absent-value handling for return(), and throw() method undefined/null handling (IteratorClose + TypeError rejection).
  3. **for-await-of IteratorClose fix (§14.7.5.7 step c)**: Removed spurious `iterator_close` call when `Await(nextResult)` rejects — per spec, `? Await(nextResult)` propagates the error without AsyncIteratorClose; the onRejected callback handles closing.
- **Files changed**: `src/interpreter/eval.rs`, `src/interpreter/builtins/mod.rs`, `src/interpreter/exec.rs`, `README.md`, `PLAN.md`, `test262-pass.txt`
- **Results**: 48 new passes, 0 regressions. 90,506/91,986 (98.39%)
  - yield expressions: 93→115/123 (93.5%)
  - AsyncFromSyncIteratorPrototype: 44→76/76 (100%)
- **Learnings for future iterations:**
  - yield* delegation errors must go through try_stack to allow finally blocks to execute — calling `generator_throw_state_machine` recursively with the error routes it properly
  - AsyncFromSyncIteratorContinuation §27.1.2.4 requires `PromiseResolve(%Promise%, value)` via `promise_resolve_with_constructor` which checks the `.constructor` getter
  - `closeOnRejection` true for next(), false for return()/throw() — the onRejected callback should close the sync iterator only when appropriate
  - for-await-of's `? Await(nextResult)` at §14.7.5.7 step c does NOT include IteratorClose — removing the spurious close fixed the double-close regression
---
