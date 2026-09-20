# Plan: issue #643 — optional-chain `delete` skips exotic `[[Delete]]` special cases

## 1. Problem restated

`delete ta[0]` runs the full `[[Delete]]` dispatch (proxy → module-namespace exotic → TypedArray exotic → String exotic → ordinary configurable check) inline in `src/interpreter/eval.rs:824-942`. `delete obj?.ta[0]` / `delete ta?.[0]` instead goes `eval_delete_optional_chain` → `eval_delete_oc_tail` → `eval_delete_on_object` (`src/interpreter/eval/access.rs:443-507`), a second copy that has only the proxy branch and the ordinary configurable check. So a valid TypedArray index, a String-wrapper index / `length`, and a module-namespace export all "delete" successfully (`true`, no strict-mode `TypeError`) when reached through `?.`. Fix by making the optional-chain and plain paths call **one** `[[Delete]]`-dispatch function so they cannot drift again.

## 2. Spec basis

- `delete` operator Runtime Semantics: Evaluation, `UnaryExpression : delete UnaryExpression` (`sec-delete-operator-runtime-semantics-evaluation`, `spec/spec.html` ~20034): property-reference branch = `ToObject(ref.[[Base]])`, `ToPropertyKey` if needed, `? baseObj.[[Delete]](key)`, then "If deleteStatus is false and ref.[[Strict]] is true, throw a TypeError", else return deleteStatus. This is the single operation both syntactic forms must perform; `?.` only changes how the Reference Record is produced (OptionalExpression / ChainEvaluation yield a normal property Reference), never the `delete` semantics.
- TypedArray `[[Delete]]` (`sec-typedarray-delete`, ~14779): if P is a String and CanonicalNumericIndexString(P) is not undefined, return `true` iff IsValidIntegerIndex is false, else `false`; otherwise `OrdinaryDelete`.
- String exotic objects (`sec-string-exotic-objects`): own index properties and `length` are `{[[Configurable]]: false}` (via StringGetOwnProperty / the `length` data property), so `OrdinaryDelete` → `false` → strict `TypeError`.
- Module namespace exotic objects `[[Delete]]` (`sec-module-namespace-exotic-objects-delete-p`, ~15285): Symbol → `OrdinaryDelete`; export name → `false`; otherwise `true`. Deferred namespaces (`import defer`, the import-defer proposal already implemented on the plain path) additionally trigger evaluation for non-symbol-like keys before that check — carried over unchanged.
- `OrdinaryDelete` (`sec-ordinarydelete`), Proxy `[[Delete]]` and arguments-exotic `[[Delete]]` (`sec-arguments-exotic-objects-delete-p`) — unchanged; already handled by the existing shared tail.

## 3. Files to touch

- `src/interpreter/eval/access.rs` — `eval_delete_on_object` becomes the single implementation of the `delete` operator's property-reference tail (`sec-delete-operator-runtime-semantics-evaluation`): ToObject (with the existing non-object fallthrough → `true`), proxy / revoked-proxy, module namespace (incl. deferred evaluation), TypedArray, String exotic, ordinary configurable check + `parameter_map` / dense-array-element cleanup. Body is a move of `eval.rs:824-942`, keeping the branch order verbatim. Keep the name and `PropertyKeyLike` generic signature so the two existing oc callers (lines 360, 406) stay untouched.
- `src/interpreter/eval.rs` — in the `Expression::Delete` → `Expression::Member` arm, keep the super/ReferenceError handling, base + key evaluation, private-name TypeError and the nullish-base "Cannot delete property … of null/undefined" TypeError as-is; replace lines ~806-943 (ToObject + inline dispatch) with `return self.eval_delete_on_object(&obj_val, &key, env)`. Remove now-unused imports (`canonical_numeric_index_string`, `is_valid_integer_index` if no longer referenced in `eval.rs`; `cargo clippy -D warnings` is enforced by the edit hook).
- Borrow note for the implementer: `get_object_cell` returns a `&ObjectHandle` tied to `&self`; `ensure_deferred_namespace_evaluation` needs `&mut self`. Copy out what is needed (`obj.borrow().module_namespace()` info, id) and re-fetch the cell after the `&mut self` call rather than holding the reference across it.
- New tests (see §4/§5): `test262-extra/delete-optional-chain-typedarray.js`, `…-string-exotic.js`, `…-module-namespace.js` (+ `…-module-namespace_FIXTURE.mjs`), `…-ordinary-parity.js`.
- No `docs/` / `CONTEXT.md` / ADR change: no new vocabulary, no architectural decision (one-function dedup inside existing modules). `docs/perf` untouched.

## 4. TDD slices

Build first: `cargo build --release -j2` (long compile → explicit timeout); init submodules if missing (`git submodule update --init --depth 1 test262 spec`). Run each new test with `uv run python scripts/run-test262.py test262-extra/<file>`.

0. **Baseline safety net (green before and after).** `test262-extra/delete-optional-chain-ordinary-parity.js` (`flags: [noStrict]`, strict cases inside `"use strict"` IIFEs). For `delete o.k`, `delete o?.k`, `delete o?.a.k`, `delete o?.[k]`: configurable own prop → `true` and removed; non-configurable → `false` (sloppy) / `TypeError` (strict); Proxy `deleteProperty` trap called exactly once, returning `false` → `false`/`TypeError`, returning `true` → `true`; mapped `arguments` `delete arguments?.[0]` unmaps (later `arguments[0]` / param decoupled); dense array `delete arr?.[0]` leaves a hole (`0 in arr === false`); primitive base (`delete "s"?.x`, `delete (1)?.x`) → `true`; nullish base short-circuits to `true`, `delete o?.a.b` with `o.a === undefined` → `TypeError`. Confirm it passes on the unmodified binary — it pins the behavior the refactor must preserve.
1. **RED — TypedArray.** `test262-extra/delete-optional-chain-typedarray.js` (`esid: sec-typedarray-delete`, `sec-delete-operator-runtime-semantics-evaluation`; `flags: [noStrict]`; `includes: [testTypedArray.js, detachArrayBuffer.js]`). Matrix over `testWithTypedArrayConstructors` × forms {`delete ta[k]`, `delete ta?.[k]`, `delete o?.ta[k]`, `delete o?.ta?.[k]`} × keys: valid `0` → `false` sloppy / `TypeError` strict; out-of-range `"1"`/`"-1"`, non-integer `"1.5"`, `"-0"`, `"Infinity"` → `true` (canonical numeric but invalid index); non-canonical `"01"` and non-numeric `"foo"` → falls to `OrdinaryDelete` (`true` when absent, a defined configurable own `foo` gets removed); after `$DETACHBUFFER`, index `0` → `true`. Must fail on current HEAD (valid-index cases return `true` via `?.`).
2. **RED — String exotic.** `test262-extra/delete-optional-chain-string-exotic.js` (`esid: sec-string-exotic-objects`, `sec-delete-operator-runtime-semantics-evaluation`). `new String("ab")` and primitive `"ab"` (ToObject path): `delete s?.[0]`, `delete s?.[1]`, `delete s?.length` → `false` / strict `TypeError`; `delete s?.[2]` and `delete s?.["01"]` → `true`; own added `x` on the wrapper is removed → `true`. Each form compared against the plain `delete s[k]`.
3. **RED — module namespace.** `test262-extra/delete-optional-chain-module-namespace.js` + `delete-optional-chain-module-namespace_FIXTURE.mjs` (`flags: [module]`, `esid: sec-module-namespace-exotic-objects-delete-p`; follow `OrdinarySet-module-namespace-receiver.js` / `_FIXTURE.mjs` for fixture wiring). Module code is strict: `delete ns?.exported` and `delete ns?.[exportedName]` → `TypeError`, binding intact; `delete ns?.nonExported` → `true`; `delete ns?.[Symbol.toStringTag]` → `TypeError` (`OrdinaryDelete` on non-configurable @@toStringTag), `delete ns?.[Symbol.iterator]` → `true`. Second small block with `import defer * as ns` (`features: [import-defer]`): `delete dns?.exported` triggers evaluation (counter in fixture side effect) and throws `TypeError`; `delete dns?.then`-style symbol-like key does not trigger evaluation — mirror the assertions already on the plain path in `language/import/import-defer/evaluation-triggers/*delete*`.
4. **GREEN — production change.** Move the full dispatch from `eval.rs` into `eval_delete_on_object` (access.rs), then make the plain `delete obj.prop` arm call it. Slices 0–3 all pass; slice 0 proves no ordinary/proxy/arguments/array regression. Commit tests + fix together (`fix(delete): apply exotic [[Delete]] semantics to optional-chain delete`).
5. **REFACTOR / verify.** Confirm no remaining duplicate dispatch (`grep -n "is_valid_integer_index\|string_exotic_index\|module_namespace" src/interpreter/eval.rs` around the delete arm), `./scripts/lint.sh`, then the runs in §5. Run quality gates as separate commands (lint, `cargo clippy`, `cargo test --release`), never `&&`-chained.

## 5. Test surface

Targeted test262 (run before/after; results must be identical or better):
- `test262/test/language/expressions/delete/`
- `test262/test/language/expressions/optional-chaining/`
- `test262/test/built-ins/TypedArrayConstructors/internals/Delete/`
- `test262/test/built-ins/Proxy/deleteProperty/`, `test262/test/built-ins/Reflect/deleteProperty/` (must not move; they do not use this path)
- `test262/test/language/module-code/namespace/internals/` (delete-* files)
- `test262/test/language/import/import-defer/evaluation-triggers/` (delete-related)
- `test262/test/language/arguments-object/` (mapped-arguments delete cleanup)
- `test262/test/built-ins/String/`

Then the full default suite (`uv run python scripts/run-test262.py`), diffed against the `origin/main` baseline (do not pass `--update-baseline`), and `cargo test --release` + `uv run python scripts/run-custom-tests.py`.

Not covered by test262 (upstream has no optional-chain-delete-of-exotic tests) → the four `test262-extra/` files above, each header naming its `esid` and quoting the spec steps in an `info:` block, following existing `test262-extra` conventions. No `tests/` addition needed: all asserted outcomes are ECMAScript-observable values/throws.

## 6. Regression risk

- `test262-pass.txt` baseline: should only gain, never lose; the plain-path move is behavior-preserving by construction (verbatim branch order), guarded by slice 0.
- Shared machinery touched: the tree-walker `Expression::Delete` arm in `eval_expr` (cold path; no hot-loop cost), `[[Delete]]` special cases that live outside `property.rs` (this change does not consolidate them into `property.rs`; see out of scope), `ensure_deferred_namespace_evaluation` (now reachable from the oc path — same key filter as plain path), GC: no new allocation/rooting except `to_object` on primitive bases, already used by both paths. Watch for the `RefCell` borrow held across `&mut self` calls (panics at runtime if `borrow_mut()` overlaps `to_object`/namespace evaluation — keep the borrow scopes of the moved body intact).
- No `ObjectKind` match changes; bytecode compiler bails on `expression:Delete` (`bytecode/compiler.rs:817`), so no VM parity issue.
- Library harnesses (`acorn`, `zod`, `luxon`, …): `delete` on ordinary objects only; no run needed beyond a smoke of one library if time permits.

## 7. Out of scope

- The plain path evaluates `ToPropertyKey` before the nullish-base check (spec 13.5.1.2 does `ToObject(base)` first) and keeps its custom "Cannot delete property … of null" message; the oc path's differing "Cannot read properties of …" message likewise. Order/message harmonization is a separate issue.
- Moving `[[Delete]]` exotic dispatch into `property.rs` as a proper MOP operation (alongside `[[Get]]`/`[[Set]]`/`[[DefineOwnProperty]]`) so `Reflect.deleteProperty`, `delete`, and builtin `DeletePropertyOrThrow` share it — worthwhile follow-up (backlog candidate), not bundled.
- Issue #642 (private-field-access gap in the same access.rs tail evaluator) — separate PR.
- Formatting, unrelated dedup between `eval.rs` and `access.rs`, and any `test262-pass.txt` change.
