# Plan: fix(gc) — RegExp.prototype[@@matchAll] stores its matcher as a plain number property

## 1. Problem restated

`RegExp.prototype[@@matchAll]` (`src/interpreter/builtins/regexp.rs:9769`) constructs a RegExp
String Iterator and, instead of putting the matcher object id into the iterator's internal
state, stashes it as an ordinary data property `"__matcher__"` holding `JsValue::number(matcher_id
as f64)` (line ~9787). `%RegExpStringIteratorPrototype%.next` (the `next` native at line ~9835)
later reads that property back with `get_property_on_id(o_id, "__matcher__")` to drive
`regexp_exec_abstract`. Because the id is encoded as a `JsValue::number`, it is invisible to the
GC: `IteratorState::RegExpStringIterator` (`src/interpreter/types.rs:1497`) has no field for it,
and `Interpreter::collect_iterator_state_roots` (`src/interpreter/gc.rs:1123`) has no arm that
roots it, so it falls into the final `_ => {}` catch-all. Once a program drops every other
reference to the matcher RegExp object, a GC cycle can collect it while the iterator itself
survives (it's still reachable, e.g. bound to a variable). The next `.next()` call reads back a
stale/reused object id and `regexp_exec_abstract` throws `TypeError: RegExp.prototype.exec
requires that 'this' be a RegExp object`. This is a hidden-root bug in the family the project
tracks under `JSSE_GC_STRESS` (issue #331): a GC-relevant reference stored somewhere the tracer
doesn't look.

## 2. Spec basis

- **§22.2.9.1 CreateRegExpStringIterator** (`spec/spec.html:39578`): step 1 creates the iterator
  with internal slots `[[IteratingRegExp]]`, `[[IteratedString]]`, `[[Global]]`, `[[Unicode]]`,
  `[[Done]]`; step 2 sets `[[IteratingRegExp]]` to the matcher object `R`. Internal slots are not
  ordinary properties — they must not be visible to `Object.keys`, `for-in`, `JSON.stringify`,
  etc., and script cannot read or overwrite them by name.
- **§22.2.9.2.1 %RegExpStringIteratorPrototype%.next ( )** (`spec/spec.html:39610`): step 5 reads
  `R` back from `O.[[IteratingRegExp]]` and step 9 calls `RegExpExec(R, S)` with it.
- **§22.2.9.3 Properties of RegExp String Iterator Instances** (`spec/spec.html:39645`), table at
  `#table-regexp-string-iterator-instance-slots`: documents `[[IteratingRegExp]]` as "an Object".
- **§22.2.5.8 RegExp.prototype [ @@matchAll ]**: step 5, `Construct(C, « R, flags »)`, always
  produces a fresh object (Construct never returns a non-object without throwing), so the matcher
  is never script-reachable through any path other than the iterator's own internal slot.

The current `"__matcher__"` property is doubly non-conformant: it is GC-unsafe (this issue), and
it is also visible/enumerable/configurable script-reachable state that §22.2.9.1 requires to be an
internal slot. Moving the matcher id into `IteratorState::RegExpStringIterator` — the engine's
existing representation for internal-slot-shaped iterator state (compare `ArrayIterator`,
`TypedArrayIterator`, `MapIterator`, which already store their backing object id as a plain `u64`
field rather than a property) — fixes the GC root and also removes the stray visible property,
because both come from the same underlying representation defect.

`"__full_unicode__"` (a boolean, next to `"__matcher__"` at line 9793) has the same visibility
defect but **no GC-safety defect** (booleans aren't heap references), and the issue does not
mention it. Left untouched — see Out of scope.

## 3. Files to touch

- `src/interpreter/types.rs` — add a `matcher_id: u64` field to the
  `IteratorState::RegExpStringIterator` variant (~line 1497).
- `src/interpreter/gc.rs` — add an explicit arm for `IteratorState::RegExpStringIterator` in
  `collect_iterator_state_roots` (~line 1123) that pushes `matcher_id` onto the worklist, instead
  of letting it fall through the trailing `_ => {}`.
- `src/interpreter/builtins/regexp.rs`:
  - `[@@matchAll]` native (~line 9769–9811): drop the `insert_value("__matcher__", ...)` call
    (~line 9782–9789); pass `matcher_id` directly into the `IteratorState::RegExpStringIterator { .. }`
    literal instead.
  - `%RegExpStringIteratorPrototype%.next` native (~line 9835 onward): drop the
    `get_property_on_id(o_id, "__matcher__")` read (line 9856); destructure `matcher_id` out of
    the matched `IteratorState::RegExpStringIterator` state alongside `source`/`flags`/`string`/
    `global`/`last_index`/`done` (~line 9860–9882) as a plain `u64` local, used in place of the old
    `matcher_id_val.as_number()` optional lookup (~line 9894). Thread `matcher_id` through the
    4 remaining `IteratorState::RegExpStringIterator { .. }` reconstructions inside what is
    currently the `if let Some(mid) = ...` body (~9904, 9916, 9931, 9979).
  - **Delete the now-provably-dead "Fallback: use raw regex (legacy path)" block** (currently
    ~line 9989 to the end of the closure, ~line 10087, including its own 4
    `IteratorState::RegExpStringIterator { .. }` reconstructions at ~9994, 10007, 10020, 10079).
    This is not optional cleanup: today the block is reachable only when
    `matcher_id_val.as_number()` returns `None`, but every construction site already always sets
    `"__matcher__"`, so the branch is already dead in practice. Once `matcher_id` becomes a plain
    (non-`Option`) `u64` field, the `if let Some(mid) = ...` guard around the live branch
    disappears, every remaining path through the (now unconditional) live branch returns, and the
    fallback code becomes *provably* unreachable to rustc — leaving it in place is a compile
    failure under this repo's `-D warnings` clippy gate (`unreachable_code` / `unused_variables`),
    not a style choice.
  - As a direct, verified consequence of deleting that block:
    - The `regex_string` local (~line 9891, `regex_input.as_str(true)`) loses its only two uses
      (~10017, ~10037/10042, both inside the deleted block) and must be deleted too. `regex_input`
      itself stays — it's also used at ~9896 and ~9969 in the live branch.
    - `fn build_regex` (`src/interpreter/builtins/regexp.rs:5822`) has exactly one call site
      (~9990, inside the deleted block, confirmed by grep) and becomes fully dead — delete the
      function.
    - `fn regex_captures` (`src/interpreter/builtins/regexp.rs:7492`) has exactly one call site
      (~10017, inside the deleted block, confirmed by grep) and becomes fully dead — delete the
      function. Do **not** confuse this with `regex_captures_at` (a distinct function, still used
      elsewhere) — leave that one alone.
    - `ensure_capture_slots` and `count_capture_groups` each have call sites elsewhere in
      `regexp.rs` (confirmed by grep: lines 8138/9014 and 5212/6082 respectively, outside the
      deleted block) — they stay untouched.
  - Leave `"__full_unicode__"` and its read at line 9857 as-is (out of scope, see below).
- No `docs/adr/` entry: this follows the established pattern already used by every other
  `IteratorState` variant that tracks a backing object (`array_id`, `typed_array_id`, `map_id`,
  `set_id`), so it is not a new architectural decision.
- No `CONTEXT.md` update: no new vocabulary.

## 4. TDD slices

1. **Red: add the dedicated GC-rooting regression test first, on today's code.** Add
   `test262-extra/RegExpStringIterator-matcher-gc-rooting.js`, following the naming and structure
   of the existing `*-gc-rooting.js` files (e.g. `test262-extra/Array-length-set-gc-rooting.js`),
   `esid: sec-%regexpstringiteratorprototype%.next`. Shape, directly from the issue's own repro:
   ```js
   var it = "abc".matchAll(/b/g);
   $262.gc();
   var r1 = it.next();
   // r1.value[0] === "b", r1.value.index === 1, r1.done === false
   $262.gc();
   var r2 = it.next();
   // r2.done === true — a null RegExpExec result still dereferences O.[[IteratingRegExp]],
   // so this second call exercises the same root on the "no more matches" path too.
   ```
   No separate reference to the matcher RegExp is held anywhere in the test, by construction: per
   §22.2.5.8 step 5 the matcher is a fresh `Construct` result never exposed to script, so the test
   doesn't need to do anything special to make it collectible. Confirm this fails on a plain
   `cargo build --release` (no `JSSE_GC_STRESS` needed) with the `RegExp.prototype.exec requires
   that 'this' be a RegExp object` TypeError (or an equivalent wrong-object failure) before
   touching production code.
2. **Green: implement the fix in one slice.** Add `matcher_id: u64` to
   `IteratorState::RegExpStringIterator` (`types.rs`); add the `collect_iterator_state_roots` arm
   in `gc.rs`; in `regexp.rs`, remove the `"__matcher__"` property write/read, thread `matcher_id`
   through the construction site and the 4 remaining reconstructions, and delete the now-dead
   fallback block plus `build_regex`/`regex_captures`/`regex_string` as described in §3. These
   pieces don't compile independently of each other (the field addition alone won't compile until
   every construction site is updated), so this is committed as a single slice rather than split
   further. Confirm the slice-1 test now passes on a plain release build.
3. **Confirm the stress-mode surface too.** Run the existing
   `test262-extra/RegExp-advance-string-index-supplementary-pua.js` (the file named in the issue)
   under `JSSE_GC_STRESS=1` and confirm it still passes — this is incidental confirmation that the
   fix also closes the originally reported stress failure, not the primary regression test (slice
   1 already covers the bug without needing stress mode at all).
4. **Full regression sweep.** Run the targeted and full test262 directories plus `cargo test
   --release` (§5) to confirm nothing else moved.

## 5. Test surface

- **New engine-internal regression (primary):** `test262-extra/RegExpStringIterator-matcher-gc-rooting.js`
  (slice 1/2 above) — this is exactly the "spec-correct but not in test262" GC-rooting check the
  project convention (`CLAUDE.md`, existing `test262-extra/*-gc-rooting.js` precedent) calls for.
  Must pass on a plain `cargo build --release` binary (no stress needed to reproduce this bug) and
  continue to pass under `JSSE_GC_STRESS`.
- **Stress-specific confirmation:** `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py test262-extra/RegExp-advance-string-index-supplementary-pua.js --timeout 300`
  — must go from failing to passing; this is the file the issue names as the stress-mode symptom.
- **Targeted test262 run:** `uv run python scripts/run-test262.py test262/test/built-ins/RegExp/prototype/Symbol.matchAll/`
  and `uv run python scripts/run-test262.py test262/test/built-ins/String/prototype/matchAll/`
  (25 files) — confirms no regression in matchAll's ordinary (non-GC) behavior, including the
  non-global/legacy-exec paths that used to depend on the now-deleted fallback block (they must
  still pass purely through the live `regexp_exec_abstract` path).
- **Full regression gate:** `uv run python scripts/run-test262.py` (default `language/`,
  `built-ins/`, `annexB/`, `intl402/`) to confirm the baseline doesn't move, plus
  `uv run python scripts/run-custom-tests.py` and `cargo test --release` (also exercises
  `./scripts/lint.sh` / clippy to confirm the dead-code deletions in §3 actually resolved the
  `-D warnings` pressure rather than just moving it).
- **Checked-root-discipline build:** build `cargo build --profile release-checked` and run the new
  `test262-extra` file plus the stress run again on that binary — `gc_assert_root_depth` and the
  LIFO `gc_temp_roots` debug-asserts are the cheapest way to catch any root-stack imbalance
  introduced by the refactor.

## 6. Regression risk

- **Mechanical risk, not behavioral:** the fix changes *where* the matcher id lives (struct field
  vs. property) and deletes a block that is already unreachable in practice, not any observable
  match semantics, lastIndex handling, or control flow on the live `regexp_exec_abstract` path.
- **Highest-risk spot:** the 4 `IteratorState::RegExpStringIterator { .. }` reconstruction sites
  that remain (inside the live branch of the `next` native closure). Missing `matcher_id` at any
  one of them is a compile error (good — Rust forces completeness), but accidentally threading the
  *wrong* captured value (e.g. reusing a stale local instead of the one just destructured) would
  silently corrupt the matcher reference without a compiler error. Mitigate by binding
  `matcher_id` once at the top of the closure (from the initial destructure) and reusing it
  verbatim at every reconstruction site, never re-deriving it.
- **Dead-code deletion is scoped and verified, not speculative:** §3 lists exact call-site counts
  (via grep) for every function being deleted (`build_regex`, `regex_captures`) and every function
  being kept despite losing a call site in this block (`ensure_capture_slots`,
  `count_capture_groups`, `regex_captures_at`). The implementer should re-verify these counts
  after the edit (a second grep or the clippy `dead_code` lint) rather than trust this plan's
  snapshot, in case an unrelated concurrent change on `main` added a new caller.
- **GC rooting correctness:** this leans directly on `gc::trace_object_fields` →
  `ObjectKind::Iterator(state)` → `collect_iterator_state_roots`, the same machinery already
  proven for `ArrayIterator`/`TypedArrayIterator`/`MapIterator`/`SetIterator`. Low risk of breaking
  other iterator kinds since this only adds one new match arm; the existing arms are untouched.
- **test262-pass.txt baseline:** no entries should move. The matchAll test262 directories exercise
  the live `regexp_exec_abstract` path already (it was already the normal path; the fallback was
  already dead before this fix), so removing dead code should be behaviorally invisible to them.
  If any matchAll test regresses, it most likely means the destructure/reconstruct threading
  dropped or misrouted `global`/`last_index`/`done` while editing those 4 sites, not `matcher_id`
  itself (which is new) — that would point to a mechanical slip in slice 2, not a design problem.
- **Bytecode fast path:** `matchAll`/the RegExp String Iterator are built-in/native-function driven
  (not user bytecode-compiled bodies), so the `bytecode/` VM fast path is not implicated.
- **Node-compat library harnesses:** none of the wired libraries (`decimal.js`, `acorn`, `zod`,
  `moment`, etc.) are known to exercise `matchAll` iteration under GC pressure in their test
  corpora; no expected interaction, and none are planned to be re-run specifically for this change
  beyond the standard full-suite gate.

## 7. Out of scope

- **`"__full_unicode__"` visibility.** Same internal-slot-should-not-be-a-property defect as
  `__matcher__`, but booleans carry no GC risk, and the issue doesn't ask for it. Leaving it alone
  keeps this PR a focused GC fix. Worth a follow-up issue for full §22.2.9.1 internal-slot fidelity
  if the project wants to close that gap, but it's a separate, non-GC concern.
- **Auditing other native iterator closures for the same hidden-property pattern.** This plan fixes
  the one instance named in the issue. A broader sweep for other `insert_value("__...__", ...)`-style
  hidden state elsewhere in `builtins/` is valuable (and is exactly the kind of thing
  `JSSE_GC_STRESS` sampling is designed to surface) but is independent follow-up work, not part of
  closing #797.
- **No baseline update.** `test262-pass.txt` is not touched; any pass-count movement is reported in
  the PR description, not committed to the baseline file.
