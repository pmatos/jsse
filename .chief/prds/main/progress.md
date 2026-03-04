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

## 2026-03-04 - US-006
- **What was implemented**: Fixed `super` expression evaluation order per spec §13.3.7
  1. **`__home_object__` scope pollution fix**: Object literal method creation was setting `__home_object__` directly on the method's captured closure (parent scope), so nested object literals like `var k = { toString() {...} }` inside a method would overwrite the outer method's `__home_object__`. Fixed by wrapping the closure in a new intermediate scope.
  2. **GetSuperBase before ToPropertyKey (§13.3.7.1 + §6.2.5.5/§6.2.5.6)**: Restructured `eval_member`, `eval_assign`, and `eval_update` for super[expr] to capture the super base (HomeObject.__proto__) BEFORE calling `to_property_key` on the key expression.
  3. **This TDZ check before key expression**: `GetThisBinding()` now checked BEFORE evaluating the key expression in super property access, so `super[super()]` in an uninitialized constructor throws ReferenceError without evaluating the inner `super()`.
  4. **Super property [[Set]] via OrdinarySet**: `super[key] = val` now calls `[[Set]]` on the super base with `this` as receiver (invoking setters correctly), instead of directly setting on `this`.
  5. **Receiver extensibility check**: When creating a new property on a frozen/non-extensible receiver via super assignment, the operation correctly fails (TypeError in strict mode).
- **Files changed**: `src/interpreter/eval.rs`, `README.md`, `test262-pass.txt`
- **Results**: 22 new passes, 0 regressions. 90,528/91,986 (98.41%)
  - Super expressions: 166→184/184 (100%)
- **Learnings for future iterations:**
  - Object literal method creation must wrap the closure in a NEW scope for `__home_object__` — setting it directly on the existing closure pollutes the parent scope when nested object literals have methods
  - `set_property_value` on JsObjectData does NOT check extensibility — must check explicitly when creating new properties on a frozen/non-extensible receiver
  - Per spec, super property [[Set]] starts at the super base (HomeObject.__proto__) with receiver = this; setters found in the prototype chain are called with receiver as `this`
---

## 2026-03-04 - US-007
- **What was implemented**: Fixed optional chaining edge cases per spec §13.5.1
  1. **Parser: tagged template after optional chain**: `OptionalChain TemplateLiteral` production now correctly reports SyntaxError. Previously the parser allowed template literals in the optional chain tail loop.
  2. **Super base in optional chain**: `OptionalChain(Member(Super, ...), ...)` now uses `get_super_base_id()` to resolve `HomeObject.__proto__` instead of `eval_expr(Super)` which returned the constructor. Fixes `super.a?.name` and `super.method?.()`.
  3. **`this` preservation**: Added `eval_oc_base()` and `eval_optional_chain_with_ref()` helpers that return `(value, this_context)`. Nested `OptionalChain` base evaluates recursively to preserve reference context. `eval_call` now handles `OptionalChain` callee to preserve `this` for patterns like `(a?.b)()`, `a?.b?.()`, `(a?.b)?.()`.
- **Files changed**: `src/parser/expressions.rs`, `src/interpreter/eval.rs`, `README.md`, `test262-pass.txt`
- **Results**: 14 new passes, 0 regressions. 90,542/91,986 (98.43%)
  - Optional chaining: 62→76/76 (100%)
- **Learnings for future iterations:**
  - OptionalChain returns a Reference Record per spec — the `this` context (reference base) must be preserved through nested chains and when used as callee
  - Super property access in optional chain must use the same `get_super_base_id()` resolution as `eval_member` — `eval_expr(Super)` returns `__super__` (the constructor), not the super base
---

## 2026-03-04 - US-008
- **What was implemented**: Fixed `using`/`await using` remaining edge cases across parser and interpreter
  1. **exec_try dispose_resources**: Added `dispose_resources` call after try block execution for `using` declarations in try bodies (§14.2.2).
  2. **exec_for init failure dispose**: Init failure in for-loops with using declarations now disposes for-scope resources before returning abrupt completion.
  3. **generator_next dispose_resources**: Added `dispose_resources` calls in Return, Normal/Empty, and Throw completion branches of replay-based generators (§14.4.8). Yield branch correctly skips disposal.
  4. **is_await_using_declaration rewrite**: Used lexer `save_state`/`restore_state` for proper two-token lookahead past `await using` to check binding identifier.
  5. **Module top-level using/await using**: Added `self.is_module` checks so using/await using are allowed at module top-level (not just block/function scope).
  6. **for-using-of disambiguation**: Special-case lookahead in `parse_for_statement` to recognize `of` as binding identifier in `for (using of = init;...)` and `for (await using of of [])`, separate from `is_using_declaration()` which excludes `Keyword::Of` to avoid `for (using of iterable)` ambiguity.
  7. **Sloppy-mode `let` disambiguation**: `parse_statement_or_declaration` now checks next token before routing `let` to lexical declaration — uses `current_identifier_name()` plus explicit `Keyword::Yield`/`Keyword::Await`/`IdentifierWithEscape` checks. Handles `using\nlet = ...` (two expression statements via ASI) and `let yield`/`let await` in generator/async context (no ASI per §14.3.1).
- **Files changed**: `src/parser/statements.rs`, `src/interpreter/exec.rs`, `src/interpreter/eval.rs`, `src/lexer.rs`, `README.md`, `test262-pass.txt`
- **Results**: 18 new passes, 0 regressions. 90,560/91,986 (98.45%)
  - using/await-using: 300→318/336 (94.6%)
  - future-reserved-words: preserved 100% pass rate
  - let/syntax: preserved 100% pass rate
- **Learnings for future iterations:**
  - `for (using of ...)` is ALWAYS a for-of with `using` as identifier per spec. `of` as binding name requires `=` after it: `for (using of = init; ...)`. Need multi-token lookahead.
  - Lexer `save_state`/`restore_state` enables multi-token lookahead without pushback slot limitations
  - `current_identifier_name()` returns None for `yield`/`await` in generator/async context, but per spec §14.3.1 they are still grammatically valid BindingIdentifiers — ASI must not apply between `let` and these tokens
---
