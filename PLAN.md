# Plan: issue #642 — optional-chain private field access on non-object silently returns `undefined`

## 1. Problem restated

`src/interpreter/eval/access.rs` has three call sites that re-implement the private-element read
(`private_fields.get(&branded)`) inline instead of delegating to `Interpreter::private_get`
(`src/interpreter/eval.rs:3940`), the function whose own doc comment claims to be "the single read
operation that every `o.#x` reference form routes through." Two of the three inline copies are
missing arms that `private_get` already has, so specific inputs silently produce `undefined`
instead of throwing `TypeError`:

- `eval_oc_tail_with_this_ctx` (a private access that is a *non-optional link inside* an optional
  chain, e.g. `o?.value.#x`) — its non-object fallback at line 306-308 swallows the error.
- `eval_oc_base`'s non-super `Expression::Member` handling (a private access that is the *base* of
  an optional chain, e.g. `o.#x?.y`) has the same non-object fallback at line 146-148, **and** a
  second, distinct gap: its accessor arm at line 123-138 returns `undefined` instead of throwing
  when the accessor is set-only (`get` is `None`) — the corresponding arm in
  `eval_oc_tail_with_this_ctx` (line 296-300) already throws correctly.

Confirmed empirically against the built binary vs. Node v26.9.0 (repro below); all three throw
`TypeError` on Node, all three return `undefined` on jsse today:

```js
class C { #x = 1; chained(o) { return o?.value.#x; } }
new C().chained({value: 5});                          // non-object base, tail-of-chain

class D { #x = 1; baseCase(o) { return o.#x?.y; } }
new D().baseCase(5);                                   // non-object base, base-of-chain

class E { set #x(v) {} f(o) { return o.#x?.y; } }
new E().f(new E());                                    // set-only accessor, base-of-chain
```

## 2. Spec basis

- **§13.3.7.2 Runtime Semantics: ChainEvaluation**, `OptionalChain : OptionalChain . PrivateIdentifier`
  — step 4: `Return MakePrivateReference(newValue, fieldNameString)`. This step performs **no**
  object check; it just builds a Reference Record whose `[[Base]]` is `newValue`.
- **§6.2.5.5 GetValue ( V )** — step 3 (`IsPropertyReference(V)` is true): 3.a `Let baseObj be
  ? ToObject(V.[[Base]])`; 3.b `If IsPrivateReference(V) is true, then Return ? PrivateGet(baseObj,
  V.[[ReferencedName]])`. This is where the private reference produced above actually gets
  resolved — at the point the surrounding expression (here, the `return`) dereferences it.
- **§7.3.28 PrivateGet ( O, P )** — step 1 `Let entry be PrivateElementFind(O, P)`; step 2 `If entry
  is ~empty~, throw a TypeError exception`; step 5 `If entry.[[Get]] is undefined, throw a TypeError
  exception` (accessor case).

Putting these together: `ToObject` on a non-nullish primitive never throws — it allocates a fresh
wrapper object (Number/String/Boolean/Symbol/BigInt) — but that fresh wrapper can never carry the
class's `[[PrivateElements]]`, so `PrivateElementFind` on it is always `~empty~`, and `PrivateGet`
always reaches its step-2 throw. `ToObject` on `null`/`undefined` throws directly. Either way the
*only* spec-compliant outcome for a non-object private-access base is a thrown `TypeError`; there is
no code path that yields a normal `undefined`. `private_get`'s existing shortcut — throwing directly
on `obj_val.as_object_id().is_none()` without materializing the wrapper — is therefore observably
equivalent to the spec's ToObject-then-PrivateGet detour (same abrupt completion, immaterial message
text), and is the behavior the other two call sites must match. **§7.3.28** step 5 is the basis for
the set-only-accessor throw.

The nullish short-circuit that already sits above the `match` in both functions (`OptionalExpression`
evaluation step: `If baseValue is either undefined or null, then return undefined`) is unrelated to
and unaffected by this fix — it is the correct implementation of a *different* spec step and must
keep firing first, before any private-element resolution is attempted.

## 3. Files to touch

- `src/interpreter/eval/access.rs`
  - `eval_oc_tail_with_this_ctx`'s `MemberProperty::Private(name)` arm (~line 277-309): replace the
    inlined `private_fields.get` lookup with a call to `self.private_get(&inner_val, name, env)`.
  - `eval_oc_base`'s non-super `MemberProperty::Private(name)` arm (~line 107-149): replace the
    inlined lookup with a call to `self.private_get(&obj_val, name, env)`, keeping the
    nullish-short-circuit-then-`eval_oc_tail_with_this` continuation that only this call site needs
    (see slice 3).
- `src/interpreter/eval.rs`: no signature change to `private_get` — it is already callable from
  `access.rs` (three existing call sites, e.g. line 677, 727) since `access.rs` is a child module of
  `eval.rs`'s `impl Interpreter` block. Its doc comment already asserts single-routing; once this fix
  lands the assertion becomes true and needs no edit.
- `test262-extra/optional-chain-private-field-non-object-base.js` (new): the red/green test for this
  issue (see slice 1-3 and Test surface below).
- No `docs/adr/` entry: this is a bug fix restoring existing documented behavior to spec, not an
  architectural decision.

## 4. TDD slices

Each slice is red-green: write/extend the assertion first, confirm it fails against the current
binary, then make the minimal production change.

1. **Tail-of-chain, non-object base** (`o?.value.#x`).
   - Test: in the new `test262-extra` file, a class with `#x` whose method does
     `return o?.value.#x;`, asserting `TypeError` for `o = {value: 5}` (primitive `.value`) and for
     `o = {value: "str"}`, plus a passing control for `o = {value: new ThatClass()}` (already works)
     and `o = null` / `o = undefined` (short-circuits to `undefined`, unaffected).
   - Production: `eval_oc_tail_with_this_ctx`'s Private arm becomes
     `match self.private_get(&inner_val, name, env) { Completion::Normal(v) => Ok((v, inner_val)), other => Err(other) }`,
     dropping the now-unused inline `branded`/`as_object_id`/`get_object_cell` lookup and its
     `else { Ok((JsValue::UNDEFINED, inner_val)) }` fallback.
2. **Base-of-chain, non-object base** (`o.#x?.y`).
   - Test: same file, a class with `#x` whose method does `return o.#x?.y;`, asserting `TypeError`
     for a primitive `o` (e.g. `5`), plus passing controls for `o` = an instance with `#x` set to
     `null`/`undefined` (chain short-circuits to `undefined`) and `o` = an instance with `#x` set to
     an object that has `y` (chain continues and returns the value).
   - Production: `eval_oc_base`'s Private arm's outer `if let Some(o) = obj_val.as_object_id()...
     else { Ok((UNDEFINED, UNDEFINED)) }` is replaced by matching on
     `self.private_get(&obj_val, name, env)`: `Completion::Normal(v)` re-runs the existing
     nullish-check-then-`eval_oc_tail_with_this(&v, chain, env)` continuation that the old
     `Field`/`Method`/`Accessor-with-getter` arms each duplicated; any other completion becomes
     `Err(other)`. This collapses the four-way inline match (`Field`/`Method`/`Accessor`/`None`) down
     to one `private_get` call plus the chain continuation.
3. **Base-of-chain, set-only accessor** (`o.#x?.y` where `#x` is set-only).
   - Test: same file, a class with `set #x(v) {}` only, method `f(o) { return o.#x?.y; }`, asserting
     `TypeError` when called with an instance of the same class.
   - Production: falls out of slice 2's fix automatically — `private_get`'s accessor arm already
     throws `"...which has no getter"` when `get` is `None` (`eval.rs:3960`), so no separate code
     change is needed once slice 2's `eval_oc_base` arm routes through `private_get`. This slice
     exists to pin the behavior with its own assertion, not to add new production code.

Run `cargo build --release` once after slice 2 (which is the substantive rewrite); slices 1 and 3 are
one-line/zero-line production deltas layered on the same build. Run
`uv run python scripts/run-test262.py test262-extra/optional-chain-private-field-non-object-base.js`
after each slice to confirm red→green.

## 5. Test surface

- **New test262-extra file**: `test262-extra/optional-chain-private-field-non-object-base.js`,
  modeled on the existing `test262-extra/Optional-chaining-primitive-prototype-accessors.js` and
  `test262-extra/private-accessor-get-set-mop.js` (header style, `/*--- ... ---*/` frontmatter with
  `esid: sec-privateget`, `info:` quoting GetValue §6.2.5.5 steps 3.a/3.b and PrivateGet §7.3.28
  steps 1-2 and 5, `features: [class, class-fields-private, class-methods-private, optional-chaining]`).
  Covers the three red cases above plus their passing controls, using `assert.throws(TypeError, ...)`
  and `assert.sameValue(..., undefined, ...)` in the same style as the two existing files.
- **Existing test262 regression guards** (already pass today; run targeted to confirm no
  regression, not to find new failures):
  - `test262/test/language/expressions/class/elements/private-field-after-optional-chain.js`
  - `test262/test/language/expressions/class/elements/grammar-private-field-optional-chaining.js`
  - `test262/test/language/statements/class/elements/private-field-after-optional-chain.js`
  - `test262/test/language/statements/class/elements/grammar-private-field-optional-chaining.js`
  - broader sweep: `test262/test/language/expressions/optional-chaining/`,
    `test262/test/language/expressions/class/elements/`, `test262/test/language/statements/class/elements/`
  - Command: `uv run python scripts/run-test262.py test262/test/language/expressions/optional-chaining/`
    and the two `class/elements` directories above.
- **Full suite**: `uv run python scripts/run-test262.py` before opening the PR, to confirm
  `test262-pass.txt` (read from `origin/main`) shows no regressions. Do not pass
  `--update-baseline`.
- **Custom tests**: `uv run python scripts/run-custom-tests.py` and `cargo test --release` (the
  crate's Rust unit/integration tests) as the standard gate; no existing `tests/` file targets this
  code path, so no changes expected there.
- **Not applicable**: this is a pure interpreter/engine change, so the Node-compat shim and library
  harness gates (`scripts/run-node-shim-selftest.sh`, `scripts/run-shim-fixtures.sh`,
  `scripts/run-library-tests.sh <lib>`) are not part of this change's test surface. No real-world
  library in the wired harnesses performs private-field access through an optional chain on a
  primitive base, so this is near-zero regression risk for that machinery and is not worth a full
  harness run.

## 6. Regression risk

- **Baseline movement**: the change turns a silent `undefined` into a thrown `TypeError` for inputs
  that are already spec-incorrect today. No test262 test can legitimately depend on the old
  (wrong) `undefined` result, since the spec mandates the throw; if the full-suite run surfaces a
  newly-failing test, treat that test's expectation as suspect and re-check it against `spec/`
  before assuming the fix is wrong. Expect zero baseline movement.
- **Shared machinery leaned on**: both edited arms sit in the tree-walker's optional-chain
  evaluation path (`eval_expr` → `eval_optional_chain_with_ref` → `eval_oc_base` /
  `eval_oc_tail_with_this_ctx`), which is on the hot path for every `?.` expression, not just private
  ones — but the edits are confined to the `MemberProperty::Private` match arms, so non-private
  optional-chain links (`?.prop`, `?.[expr]`, `?.()`) are untouched. `private_get` itself is
  unchanged; only its caller count grows from 3 to 5. No GC rooting, `ObjectKind` match, or bytecode
  fast-path code is touched — private-field reads in this engine are tree-walker-only (the private
  arms use `obj.borrow().private_fields` directly), so the bytecode compiler's fast path is
  unaffected either way.
- **Behavior-preserving refactor risk**: slice 2's rewrite of `eval_oc_base`'s Private arm is the
  largest structural change (collapsing a 4-way match into a `private_get` call plus a 2-way
  nullish-check-and-continue). The risk is losing the `eval_oc_tail_with_this(&v, chain, env)`
  continuation for the `Field`/`Method`/`Accessor`-success cases — the TDD slice's passing controls
  (object base, non-nullish `#x`, chain continues to `.y`) exist specifically to catch that
  regression before it reaches test262.

## 7. Out of scope

- **`super.#x` in an optional-chain base** (`eval_oc_base`'s `Expression::Super` handling,
  `access.rs` ~line 43-70): `super . PrivateIdentifier` has no grammar production in `spec/spec.html`
  (§sec-property-accessors-runtime-semantics-evaluation lists only `MemberExpression :
  MemberExpression . PrivateIdentifier`, not a `super`-prefixed form) — it should be a
  **SyntaxError** in the parser, not a runtime TypeError-vs-silent-undefined question. jsse currently
  throws a runtime `TypeError` for this in `eval.rs:3908` and returns a value straight off `this` in
  the `access.rs` optional-chain-base copy — inconsistent with each other, but a parser-layer bug in
  a different family from this issue. Will file a separate issue rather than fold it into this PR.
- **`delete obj?.ta[0]` and other exotic-object `[[Delete]]` special cases through optional chains**
  — already tracked as issue #643 ("delete via optional chain skips exotic-object [[Delete]] special
  cases"); not touched here.
- **Renaming/refactoring the other non-private arms** of `eval_oc_base` /
  `eval_oc_tail_with_this_ctx` (`Dot`, `Computed`) — out of scope; this PR only touches the
  `Private` arms that are the subject of the issue.
- **Changing `private_get`'s error message text** ("from a non-object" vs. Node's "from an object
  whose class did not declare it") — not spec-observable (`ECMA-262` does not mandate message
  wording), so not changed. The two now-newly-routed call sites will surface this existing message
  text at two more places; that is a side effect of correctness, not a message redesign.
- **Rolling `test262-pass.txt` forward** — a `main`-branch-only operation per project convention; not
  planned here regardless of full-suite outcome.
