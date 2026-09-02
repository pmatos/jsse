# Plan: issue #538 — `Op::GetElement` lacks the tree-walker's numeric-index fast path

## 1. Problem restated

`eval_member` (tree-walker, `src/interpreter/eval/access.rs:745-782`) recognizes a
computed member read whose raw key is a JS `Number` on a typed array or a plain
array object, and reads the element directly (`typed_array_get_index` /
`elems[idx]`) before `ToPropertyKey` ever runs — no allocation.
`member_get_computed` (bytecode VM, `src/interpreter/bytecode/vm.rs:110-133`),
the equivalent read path for the compiled `Op::GetElement` opcode, has no such
check: it always calls `to_property_key`, which for an integer index formats
`(trunc as u32).to_string()` and wraps it in a `JsPropertyKey` (two heap
allocations per element read), and then `get_object_property` re-derives the
same integer from that string via `canonical_numeric_index_string`. The fix is
to port the tree-walker's pre-`ToPropertyKey` numeric-index check into the
bytecode read path via one shared helper, eliminating the allocation on every
compiled-body typed-array/array element read. `Op::SetElement` has the same
shape but is explicitly out of scope (see §7) because the tree-walker pays the
same round trip on writes, so it is not a VM-only asymmetry.

## 2. Spec basis

This issue changes no observable JavaScript behavior — `member_get_computed`'s
existing slow path (`to_property_key` → `get_object_property`) is already
spec-correct; the fast path is a provably-equivalent shortcut for one input
shape (object base, `Number`-typed raw key), verified against test262 in both
execution modes. The clauses below are what make the shortcut provably
equivalent, not a behavior change to implement:

- **§13.3.7.3 EvaluatePropertyAccessWithExpressionKey**
  (`sec-evaluate-property-access-with-expression-key`, spec.html:19284): step 2
  evaluates the key expression to a raw value; the NOTE at step 3 confirms
  `ToPropertyKey` is *not* part of building the Reference Record — it happens
  later, inside `GetValue`. This is the basis for reading the raw `Number` key
  before any coercion, exactly mirroring what `eval_member` already does and
  citing at `access.rs:703`.
- **§6.2.5.5 GetValue** (`sec-getvalue`, spec.html:4434): step 3.c.i performs
  `ToPropertyKey` only when actually fetching the property — the fast path is
  a shortcut of this step for the one case where the outcome is already known
  without coercion.
- **§10.4.5.11 TypedArray `[[Get]]`** (`sec-typedarray-get`,
  spec.html:14735): for a string key, computes
  `CanonicalNumericIndexString(P)` and, if not `undefined`, returns
  `TypedArrayGetElement(O, numericIndex)` — it does **not** fall through to
  `OrdinaryGet`/the prototype chain for any canonical numeric index, valid or
  not.
- **§10.4.5.12 TypedArrayGetElement** (`sec-typedarraygetelement`, formerly
  `IntegerIndexedElementGet`, spec.html:15017): step 1 — `if
  IsValidIntegerIndex(O, index) is false, return undefined`. This is the exact
  justification for the tree-walker's (and the ported) "any canonical numeric
  index that's out of range returns `undefined`, no prototype walk" branch.
- **§10.4.5.14 IsValidIntegerIndex** (`sec-isvalidintegerindex`,
  spec.html:14995): step 3 explicitly rejects `-0` (`If index is -0𝔽 or index
  < -0𝔽, return false`). Combined with...
- **§7.1.21 CanonicalNumericIndexString** (`sec-canonicalnumericindexstring`,
  spec.html:5711): step 1 — the *string* `"-0"` canonicalizes to the Number
  `-0`, but `ToString` of the Number `-0` is `"0"` (positive), a *different*
  canonical numeric index. This is why `arr[-0]` and `arr[0]` name the same
  property (`ToPropertyKey(-0)` → `"0"` → canonical index `+0`, in range) while
  a literal `"-0"` string key does not. The fast path must defer to the slow
  `ToPropertyKey` path for a raw `-0` key rather than short-circuit it to
  `undefined` — this is already correctly handled by the tree-walker's
  `!index.is_sign_negative()` guard and must survive the port verbatim.
- **§10.1.8 OrdinaryGet** (`sec-ordinaryget`, spec.html:12858) plus §10.4.6
  (Array exotic objects, spec.html:14126): Array only overrides
  `[[DefineOwnProperty]]`; `[[Get]]` is the ordinary one. Reading `elems[idx]`
  directly for an in-bounds index that is not shadowed by an own property
  (checked via `properties.contains_key`) and is not itself `undefined` (a
  potential hole) is a valid shortcut of `OrdinaryGet`/`OrdinaryGetOwnProperty`
  for that one case; every other case (holes, shadowed indices, out-of-range)
  must fall through unchanged to the general property lookup.

## 3. Files to touch

- `src/interpreter/property.rs` — new `impl Interpreter` method
  `numeric_index_fast_get(&self, obj_val: &JsValue, index: f64) -> Option<JsValue>`,
  containing the block currently inlined at `access.rs:745-782` verbatim
  (typed-array branch + plain-array branch, including the existing
  `key_str` shadow-check allocation in the array branch — that allocation is
  pre-existing tree-walker behavior and out of scope here, see §7).
- `src/interpreter/eval/access.rs` — `eval_member`: replace the inlined block
  (lines 745-782) with a call to the new helper. Pure refactor, no behavior
  change.
- `src/interpreter/bytecode/vm.rs`:
  - `Op::GetElement` dispatch arm (currently lines 330-342): peek (not pop)
    the top two stack slots, try `numeric_index_fast_get` before deciding
    whether to call `root_operand_stack` / `member_get_computed`. See §4 for
    the two-slice breakdown.
  - `member_get_computed` (lines 110-133): left unchanged. See design note
    below.
  - Doc comment on `root_operand_stack` (lines 26-31): extend to note the
    `Op::GetElement` fast-path carve-out and why it is safe to skip.
- `src/interpreter/bytecode/tests.rs` — new unit tests (see §4).
- No `docs/adr/` entry (this is a local fast path, not an architectural
  decision) and no `test262-extra/` additions (no new spec-observable
  behavior — see §2 and §5).

**Design note — why the fast path is *not* embedded inside `member_get_computed`,
despite the issue text saying "port into `member_get_computed`":** the only
caller of `member_get_computed` is the `Op::GetElement` arm. Skipping
`root_operand_stack` on a fast-path hit (§4, slice 2) requires knowing whether
the fast path applies *before* deciding to root — i.e., the check has to live
at the call site regardless. Embedding a second copy of the same check inside
`member_get_computed` as well would either (a) be dead code once the call site
already filters hits out, or (b) run the (cheap, allocation-free) predicate
twice on every hit. Putting the one call in the `Op::GetElement` arm and
leaving `member_get_computed` as a pure slow-path helper satisfies the issue's
actual requirement — one shared predicate, ported from `eval_member`, no
second copy — without dead code. `numeric_index_fast_get` is the shared
helper the issue asks for; where exactly it's invoked from inside `vm.rs` is
an implementation detail.

## 4. TDD slices

This is a performance fix with no intended behavior change, so most steps are
not "red" in the sense of a pre-existing bug — `member_get_computed`'s current
slow path is already spec-correct. Tests here are characterization/regression
guards proving the fast path is exactly equivalent to the slow path across the
edge cases the issue calls out, plus one genuine regression guard for the
rooting-skip in slice 3 (which *can* be red if that slice introduces a
use-after-free).

1. **Extract the shared helper (pure refactor).**
   - Move `access.rs:745-782` into
     `Interpreter::numeric_index_fast_get` in `property.rs`; call it from
     `eval_member`. No new test — gate is the existing tree-walker suite:
     `cargo test --release` plus a targeted test262 run over
     `test262/test/built-ins/TypedArrayConstructors/internals/Get/` and
     `test262/test/language/expressions/property-accessors/` must produce
     byte-identical pass/fail sets to `origin/main` (this step must not move
     the baseline in either direction).

2. **Wire the helper into `Op::GetElement` (the actual fix).**
   - In the `Op::GetElement` arm, after popping `key_val`/`base` (rooting via
     `root_operand_stack` unconditionally still runs first, exactly as
     today — this slice changes *what gets computed*, not *when rooting
     happens*): try `key_val.as_number().and_then(|i|
     interp.numeric_index_fast_get(&base, i))`; on `Some(v)` push `v` directly
     without calling `member_get_computed`; on `None` fall through to today's
     `member_get_computed` call unchanged.
   - Add to `src/interpreter/bytecode/tests.rs`, each asserting tree-walker
     and bytecode parity (extend `eval_with_mode`/`assert_script_completion_*`
     style already in the file):
     - `end_to_end_computed_read_typed_array_out_of_bounds_matches_tree_walker`
       — `ta[i]` with `i >= ta.length` → `undefined` in both modes.
     - `end_to_end_computed_read_typed_array_negative_index_matches_tree_walker`
       — `ta[-1]` → `undefined` in both modes.
     - `end_to_end_computed_read_typed_array_fractional_index_matches_tree_walker`
       — `ta[0.5]` → `undefined` in both modes.
     - `end_to_end_computed_read_typed_array_minus_zero_index_matches_tree_walker`
       — `ta[-0]` → **the element at index 0**, not `undefined`, in both
       modes. This is the case most likely to regress if someone "simplifies"
       the `!index.is_sign_negative()` guard away; it must be its own test,
       not folded into the negative-index case.
     - `end_to_end_computed_read_typed_array_detached_buffer_matches_tree_walker`
       — construct a view, `buffer.transfer()` to detach it, read `ta[0]` →
       `undefined` in both modes.
     - `end_to_end_computed_read_typed_array_through_proxy_matches_tree_walker`
       — `new Proxy(ta, { get(t, p, r) { return 'trapped'; } })[0]` →
       `"trapped"` in both modes, proving the fast path's `typed_array_info()`
       check correctly never fires for a Proxy (guaranteed by `ObjectKind`'s
       exclusivity, but worth a regression test since this is exactly the
       kind of case a careless fast-path predicate could break).
     - `end_to_end_computed_read_array_out_of_bounds_matches_tree_walker`,
       `..._negative_index...`, `..._fractional_index...` — same shape for a
       plain array.
     - `end_to_end_computed_read_array_minus_zero_index_matches_tree_walker` —
       `a[-0]` → the element at index 0 (in IEEE, `-0.0 >= 0.0` is true, so
       the array branch's own `index >= 0.0` guard already takes the direct
       `elems[0]` read — a *different* code path from the typed-array
       branch's `-0` handling, so it needs its own pinned test rather than
       relying on the typed-array case above to cover it).
     - `end_to_end_computed_read_array_shadowed_index_matches_tree_walker` —
       `Object.defineProperty(a, '1', { get() { return 99; } })`-style
       accessor shadow on an array index must not read the stale
       `elems[idx]`; must still hit whatever `member_get_computed`'s slow
       path already does. (Not a `{value, get}` mixed descriptor — that's an
       invalid data/accessor combination and `defineProperty` throws
       `TypeError` on it before the test ever reaches the read under test.)
   - Run the full existing `end_to_end_computed_read_on_plain_array_takes_bytecode_path`
     / `..._on_typed_array_takes_bytecode_path` tests unchanged as a baseline
     sanity check (in-bounds case, already passing).

3. **Skip `root_operand_stack` on a fast-path hit.**
   - Restructure the `Op::GetElement` arm to *peek* (index, not pop) the top
     two stack slots to evaluate `numeric_index_fast_get` before deciding
     whether to call `root_operand_stack` at all. On a hit: pop, unroot via
     `unroot_stack_value` (matching `push_value`'s per-value rooting, which
     already independently protects every value currently on the operand
     stack via `gc_bytecode_roots` — see the correctness argument below),
     push the result, done — `root_operand_stack`/`gc_temp_roots` never
     touched. On a miss: identical to slice 2's `None` branch, including
     `root_operand_stack` seeing the *full* stack (including the still-present
     base/key) exactly as it does today, to keep the slow path's rooting
     behavior byte-for-byte unchanged.
   - **Correctness argument to write into the code comment** (per the issue's
     explicit ask — do not skip rooting silently): `numeric_index_fast_get`
     only borrows an existing object's already-allocated typed-array/array
     storage and clones a `JsValue` out of it (`Rc`/`Arc` clone, or a raw
     numeric read into a fresh `JsValue::number`/`JsValue::BigInt`) — it never
     calls a getter, a proxy trap, `to_property_key`'s `to_primitive`/
     `to_string_value` path, or anything else that can execute user code or
     allocate a new *GC-tracked object* mid-flight. `root_operand_stack`
     exists to protect the rest of the operand stack against a **nested**
     `gc_safepoint()` reachable from exactly those code paths (see its own doc
     comment and the regression it fixed, `end_to_end_member_chain_base_survives_gc_during_rhs_evaluation`).
     None of them are reachable from the fast path, so there is no nested
     safepoint to protect against. Independently, every value currently on
     the VM operand stack is *already* rooted via `gc_bytecode_roots`
     (`push_value` roots on push; only the matching `pop_value`/
     `unroot_stack_value` removes it — `Op::GetElement`'s bare `stack.pop()`
     does not), so even in the hypothetical case of a hidden reachable
     safepoint, the rest of the stack stays protected independently of
     `root_operand_stack`.
   - Add `end_to_end_getelement_fast_path_survives_gc_with_pending_sibling_operand`
     to `src/interpreter/bytecode/tests.rs`, modeled directly on the existing
     `end_to_end_member_chain_base_survives_gc_during_rhs_evaluation` /
     `end_to_end_getprop_call_argument_survives_gc_during_sibling_arg_evaluation`
     shape: push a freshly-allocated, otherwise-unreferenced object onto the
     operand stack via an earlier opcode, then take the `Op::GetElement`
     fast path (a numeric-index typed-array/array read) as a *sibling*
     operand whose own evaluation is trivial, then force `$262.gc()` in a
     later sibling before the first object is consumed, asserting the first
     object's value survives. If this cannot be made to fail without the
     fast path present (i.e., it can't actually exercise the skipped-rooting
     branch meaningfully) that is itself useful information — record it in
     the PR description rather than keep the test as a no-op assertion of the
     obvious.
   - **Grounding step before writing the comment:** the analysis above implies
     `gc_bytecode_roots` (via universal `push_value`) already protects the
     whole operand stack independently of `root_operand_stack`, which would
     make `root_operand_stack` look redundant everywhere, not just on this
     fast path. Before relying on that: run `git log -S root_operand_stack
     --oneline` and read the PR that introduced it. If `push_value`'s
     universal per-push rooting already existed at that point, the
     introducing PR's own description should explain what extra case
     `root_operand_stack` covers (or confirm it's belt-and-suspenders) —
     fold whichever is true into the code comment instead of re-deriving it
     from scratch. Do not widen this into removing `root_operand_stack` from
     other opcodes (`GetProp`, `SetProp`, `SetElement`) — that is separate
     scope.
   - **If, once written, the correctness argument above does not hold up**
     (e.g., the git-log grounding step surfaces a case the fast path *can*
     reach, or a `JsValue::BigInt` allocation for `BigInt64Array`/
     `BigUint64Array` turns out to reach a `gc_safepoint()` after all),
     **defer this slice to a follow-up issue** rather than land it
     speculatively — slice 2 alone already closes the issue's core ask (the
     allocation elimination) and does not depend on this slice.

## 5. Test surface

- **Targeted test262** (must show zero regressions against
  `origin/main:test262-pass.txt`, run in both default and `--bytecode` mode
  via `uv run python scripts/run-test262.py test262/test/built-ins/TypedArrayConstructors/internals/Get/`
  and again with `--bytecode`; same for
  `test262/test/language/expressions/property-accessors/`,
  `test262/test/built-ins/TypedArray/`, and `test262/test/built-ins/Array/`):
  these are the directories that exercise `[[Get]]` on typed arrays/arrays
  through bracket notation, including the existing `key-is-not-minus-zero.js`,
  `key-is-out-of-bounds.js`, `detached-buffer.js`,
  `detached-buffer-key-is-not-numeric-index.js`, and
  `key-is-not-canonical-index.js` cases under `internals/Get/`.
- **Full test262 suite**, both modes (`uv run python scripts/run-test262.py`
  and `uv run python scripts/run-test262.py --bytecode`), as the final gate —
  required by the issue's Validation section, no regression against the
  `origin/main` baseline permitted.
- **New unit tests** in `src/interpreter/bytecode/tests.rs` (§4) are the
  primary coverage for the edge-case matrix (negative/fractional/-0/detached/
  proxy/shadowed-index) since test262's existing coverage of these cases
  already passes today through the slow path and won't itself distinguish
  "fast path is wired correctly" from "fast path is dead code" — only a
  bytecode-mode-specific unit test with `bc_count >= 1` assertions does that.
  No `test262-extra/` addition: there is no spec-correct behavior here that
  test262 doesn't already cover (see §2) — the new coverage need is
  *bytecode-path-specific*, which is exactly what `bytecode/tests.rs` is for.
- **Regression gate for the whole change:** `cargo test --release` (full
  suite, not just `bytecode::tests`), since `property.rs` and `access.rs` are
  shared, high-traffic modules.
- **Performance validation** (informational, not a pass/fail gate per the
  issue's own wording — "if it is unmeasurable, that is the honest outcome to
  record"): re-run `benchmarks/scripts/bench_opmix.js`'s `elem` and `arith`
  variants before/after and record whether `elem`'s absolute
  default-vs-`--bytecode` saving now exceeds `arith`'s, per the table in the
  issue body. Record the result (including "no measurable change") in the PR
  description; do not add a new `docs/perf/` file for this unless the
  implementer judges the result substantial enough to be worth a durable
  record — a one-paragraph PR note is sufficient by default.

## 6. Regression risk

- **`property.rs` / `get_object_property`**: unchanged by this issue — the
  fast path only ever *bypasses* it on a hit; every miss falls through to the
  exact same call as today. Risk is contained to the fast-path predicate
  itself producing a wrong `Some(v)` for some input it shouldn't — the edge
  case matrix in slice 2 is designed to catch exactly that class of bug
  (especially the `-0` case, which is the one subtle enough to plausibly slip
  past casual review).
- **GC rooting / `gc_safepoint()`** (slice 3 only): the change with the most
  potential to move `test262-pass.txt` in the wrong direction (a
  use-after-free would surface as sporadic, hard-to-reproduce test262
  failures, not a clean local failure) if the "no nested safepoint reachable"
  argument turns out to be wrong for some `TypedArrayKind` (BigInt64/
  BigUint64 producing a `JsValue::BigInt` is the one path worth double-checking
  during implementation — confirm `JsBigInt` construction never calls
  `gc_safepoint()` before treating the argument as settled). Mitigated by
  making slice 3 independently revertible/deferrable (§4) without affecting
  slices 1-2's win.
- **Exhaustive `ObjectKind` match / typed-array vs. array vs. Proxy
  exclusivity**: the fast path leans on `ObjectKind`'s type-enforced
  disjunction (an object can't simultaneously be a Proxy and a TypedArray) to
  guarantee `typed_array_info()`/`array_elements()` correctly return `None`
  for a Proxy base. This is existing, load-bearing behavior this change
  doesn't modify, but the Proxy unit test in slice 2 pins it down explicitly
  for this code path.
- **Bytecode compile eligibility / `BAIL` table**: untouched — `Op::GetElement`
  already compiles today for computed member reads; this issue changes only
  what the VM does at runtime when that opcode executes, not compiler
  eligibility. No `compiler.rs` changes, so no new `BAIL` reasons and no
  change to `body_dispatch_compiled`/`body_dispatch_ast` counts.
- **Node-compat library harnesses**: low risk — none of the wired libraries
  (`decimal.js`, `big.js`, `acorn`, etc.) depend on this being *slow*, and the
  fast path is behavior-preserving; a full-suite `cargo test --release` run
  plus the targeted test262 directories is sufficient signal without
  re-running every library harness for this change.

## 7. Out of scope

- **`Op::SetElement` / `member_set_computed`'s equivalent fast path.** The
  issue explicitly deprioritizes this ("still worth a fast path, lower
  priority than the read") since the tree-walker pays the same round trip on
  writes today — this is a shared cost, not a VM-only asymmetry, and is a
  clean follow-up issue on its own once this read-side change has landed and
  proven the pattern.
- **The plain-array branch's own `key_str` allocation** (the
  `(idx as u32).to_string()` shadow-check inside the array half of the fast
  path, carried over verbatim from the existing tree-walker code). This is a
  pre-existing tree-walker cost this issue doesn't touch; removing it is a
  separate optimization with its own correctness surface (the shadow-check
  needs *some* way to test "is there an own property named this index" —
  changing that mechanism is out of scope here).
- **Bytecode inline caching (IC) for computed member access.** `eval_member`'s
  own IC probing (`access.rs:794-806`) is explicitly dot-access-only ("v1
  scope... computed access goes straight to the slow path"); extending IC to
  computed/numeric access is unrelated to this issue's allocation-elimination
  goal and a materially larger change.
- **`#524`'s bytecode eligibility expansion** and **`#539`'s per-entry cost**
  — both explicitly called out in the issue as independent, larger, and not
  to be sequenced behind this change.
- **Rolling `test262-pass.txt` forward** — read from `origin/main`, not
  rewritten by this branch (per repository convention).
