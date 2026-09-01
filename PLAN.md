# Plan: Implement `Iterator.prototype.join` (issue #550)

## 1. Problem restated

`Iterator.prototype.join` does not exist on jsse's `%IteratorPrototype%`
(`typeof Iterator.prototype.join === "undefined"`), causing test262 failures
across the 18 files under `test262/test/built-ins/Iterator/prototype/join/`
(test262 bump to `7710052`, #547; the issue's "32 failing scenarios across 16
files" is stale against the currently-checked-out `7710052` tree, which has 18
files — none carry `noStrict`/`onlyStrict`/`module`/`raw` flags, so the runner's
dual-mode execution makes the actual target 36 scenarios). The method is an
eager (non-lazy) terminal consumer, analogous to
`Array.prototype.join`: it walks a plain iterator to completion, coercing each
non-nullish value to a string and concatenating them with a separator
(default `","`, or `ToString(separator)` if one is supplied), closing the
receiver only in the narrow cases where the *separator* or a *yielded value*
fails to coerce to a string — not when the iterator's own `next()`/`next`
getter throws, returns a protocol-violating result, or exhausts normally.

## 2. Spec basis

`sec-iterator.prototype.join` does **not** exist in `spec/spec.html` (submodule
pinned at `270a490b`) — grepped for `id="sec-iterator.prototype.join"` and for
any `join` clause under the `%IteratorPrototype%` section (`sec-iterator.prototype`
through `sec-iterator.prototype.toarray`, plus the `%IteratorHelperPrototype%`
clauses): no match. `Iterator.prototype.join` is a separate, still-unmerged TC39
proposal (copyright header "Kevin Gibbons" on every test file), the same
situation as `Iterator.prototype.chunks`/`windows` (#548) and
`Iterator.prototype.includes` (#549) before it — see those PLAN.md precedents
(`git show 02eb8ce:PLAN.md`, `git show 6ecc882:PLAN.md`) for the same
"proposal not yet merged, reconstruct from test262" pattern.

Unlike `chunks`/`windows`/`includes`, the `join` test files under
`test262/test/built-ins/Iterator/prototype/join/` carry **no `info:` frontmatter
algorithm excerpt** — every file's frontmatter is just `esid` + `description` +
`features`. The algorithm below is therefore reconstructed entirely from the
observable behavior each test file pins (file names cited per step); every step
is independently corroborated by at least one assertion and no two files
contradict each other:

**`Iterator.prototype.join ( separator )`**
1. Let `O` be the this value. If `O` is not an Object, throw a **TypeError**
   (`receiver-not-object.js`: `undefined`, `null`, `false`, `0`, `0n`, `""`,
   `Symbol()` all throw; no attempt to close anything, since there is nothing
   to close).
2. If `separator` is `undefined`, let `sep` be `","` (`results-no-separator.js`).
   Otherwise, let `sepCompletion` be `Completion(ToString(separator))`.
   - If `sepCompletion` is an abrupt completion, close `O` directly (call `O`'s
     `return` method) and re-throw the **original** `ToString` error —
     **regardless of what the close itself returns or throws**
     (`closes-on-separator-coercion-exception.js`: the object's `next`
     **getter** is never invoked — `gotNext` stays `false` — proving this step
     precedes fetching `next`; `calledReturn` is `true`, proving `O.return` is
     still invoked even though `next` was never read). Concretely: in that test
     `O.return` returns `undefined`, which is not an object, so a naive
     "propagate the close's own result" implementation would throw
     `TypeError: Iterator result is not an object` instead of the original
     `Test262Error` — the close's outcome (success, its own throw, or a
     non-object return value) must always be discarded in favor of the
     original error. Use `iterator_close_with_completion(interp, this,
     Err(err.clone()))` (the idiom `chunks`/`windows` already use at
     `iterators.rs:2345`/`2486` for this exact pre-`GetIteratorDirect`
     validation-close case) and re-throw `err`, not whatever
     `iterator_close_with_completion` returns.
   - `ToString` is called **exactly once** on the separator
     (`separator-tostring.js`). `null` is *not* treated as "no separator" — it
     coerces to the string `"null"` (`separator-tostring.js`:
     `['one','two','three'].values().join(null) === 'onenulltwonullthree'`) —
     only `undefined` gets the `","` default.
3. Let `sep` be the resulting string.
4. Let `iterated` be `? GetIteratorDirect(O)` — this is where `next` is first
   read, and only **after** the separator has been fully coerced
   (`next-lookup-after-separator-tostring.js`: `effects === ['toString', 'get
   next']`). If reading `O.next` throws (getter throws) or `O.next` is later
   found non-callable, the resulting abrupt completion is **not** followed by a
   close (`does-not-close-on-next-getter-error.js`: `gotReturn` stays `false`).
5. Let `R` be the empty String. Let `first` be `true`.
6. Repeat:
   a. Let `value` be `? IteratorStepValue(iterated)` (calls `next()`, reads
      `.done`/`.value`). If this step itself throws — because `next()` throws
      (`does-not-close-on-iterator-error.js`) or returns a non-object result
      (`does-not-close-on-iterator-protocol-violation.js`) — propagate the
      error **without** closing (`gotReturn` stays `false` in both files; this
      matches `IteratorStepValue`'s own internal closing semantics, which
      already mark the record done on failure, so no further `IteratorClose`
      call is needed — the same behavior jsse's `reduce`/`some`/`every`/`find`
      already rely on via `iterator_step_value_getter`/`iterator_step_direct`).
   b. If `value` is `~done~`, return `R` (`does-not-close-on-iterator-exhaustion.js`:
      natural exhaustion returns the joined string with **no** close call).
   c. If `first` is `true`, set `first` to `false`. Else, set `R` to the
      string-concatenation of `R` and `sep`.
   d. If `value` is neither `undefined` nor `null`:
      - Let `strCompletion` be `Completion(ToString(value))`.
      - If `strCompletion` is an abrupt completion, close `iterated` and
        re-throw the **original** `ToString` error, again discarding whatever
        the close itself returns or throws — same invariant as step 2
        (`closes-on-contents-coercion-exception.js`: `next()` is called
        exactly once — `calledNextCount === 1` — before the throwing
        `toString`, and `calledReturn` is `true`; the test's `return` method
        returns `undefined`, so the same "close result must be discarded"
        requirement applies). Idiom: `let _ = iterator_close_with_completion(interp,
        &iter, Err(e.clone())); return Completion::Throw(e);` — exactly what
        `some`/`every` already do at `iterators.rs:1538-1541` when their
        predicate throws mid-loop.
      - `ToString` is called exactly once per value that needs it
        (`contents-tostring.js`).
      - Set `R` to the string-concatenation of `R` and `strCompletion.[[Value]]`.
   e. Else (nullish `value`), contribute nothing further to `R` — the separator
      already added in step 6c makes the gap visible
      (`contents-nullish.js`: `['one', null, 'two', undefined].values().join()
      === 'one,,two,'`).

Property shape: `Iterator.prototype.join` is a normal (non-generator, non-lazy)
method — `length` 1, `name` "join", `{writable: true, enumerable: false,
configurable: true}` (`length.js`, `name.js`, `descriptor.js`), not a
constructor (`not-a-constructor.js`).

This reuses already-merged `ecma262` abstract operations jsse already
implements and uses for `toArray`/`forEach`/`some`/`every`/`find`/`reduce`:
- `GetIteratorDirect` — `spec/spec.html`, `emu-clause id="sec-getiteratordirect"`
  (jsse: `get_iterator_direct_getter` in `src/interpreter/builtins/iterators.rs:302`).
- `IteratorStepValue` — `spec/spec.html`, `emu-clause id="sec-iteratorstepvalue"`
  (jsse: `iterator_step_value_getter`, `iterators.rs:482`).
- `IteratorClose` — `spec/spec.html`, `emu-clause id="sec-iteratorclose"`
  (jsse: `iterator_close_with_completion`, `iterators.rs:515`, used for both
  close sites in this plan since both must discard the close's own outcome
  and preserve the original error — see steps 2 and 6.d above).
- `ToString` — `spec/spec.html`, `emu-clause id="sec-tostring"` (jsse:
  `Interpreter::to_string_value`, `src/interpreter/eval.rs:1697`).

No new JavaScript syntax, no new `ObjectKind` variant, no lazy `%IteratorHelperPrototype%`
machinery is needed — `join` returns a plain string synchronously, exactly like
`reduce`.

## 3. Files to touch

- `src/interpreter/builtins/iterators.rs` — add `join` as one new
  `self.define_method(iter_proto_id, "join", 1, ...)` block, inserted
  immediately after the existing `reduce` block (ends `iterators.rs:1719`) and
  before the `// Lazy helpers: map, filter, take, drop, flatMap` comment
  (`iterators.rs:1721`) — grouping it with the other eager terminal-consumer
  methods (`toArray`, `forEach`, `some`, `every`, `find`, `reduce`), matching
  this file's existing convention of keeping eager methods together and lazy
  `%IteratorHelperPrototype%`-based methods (`map`/`filter`/`take`/`drop`/
  `flatMap`/`chunks`/`windows`) in `setup_iterator_lazy_helpers`.
- `README.md` — update the pass count/percentage on line 9 (currently
  `99,758 / 99,907 (99.85%)`, calling out `includes`/`join` by name as the two
  unimplemented `Iterator.prototype` proposals) after the final test262 run.
  **Not part of the TDD slices** — last step before opening the PR. Note: a
  sibling worktree branch (`sym/jsse/549-implement-iterator-prototype-includes-...`)
  is implementing `includes` in parallel and is not merged to `main` as of this
  plan; this PR's README wording should only claim `join` is resolved, not
  `includes`, unless `main` has picked up `includes` by the time this lands.
- No parser/lexer/AST changes — `join` is an ordinary method call, not new
  syntax.
- No `docs/adr/` entry — this is a straightforward addition using an
  already-shipped implementation pattern (`get_iterator_direct_getter` /
  `iterator_step_value_getter` / `iterator_close_with_completion`, the same
  primitives `reduce`/`some`/`every`/`find`/`chunks`/`windows` already use);
  it introduces no new architectural mechanism.
- `test262-extra/built-ins/Iterator/prototype/join/` — one new file,
  `separator-placement-empty-values.js` (name TBD by the implementer, follow
  existing `test262-extra` file-naming/frontmatter conventions), covering the
  gap identified in §5.

## 4. TDD slices

Each slice: `cargo build --release`, then
`uv run python scripts/run-test262.py test262/test/built-ins/Iterator/prototype/join/`
(targeted directory), then `cargo test --bin jsse` for the Rust-side unit
tests (per the fmt/clippy hook noted in memory — every `.rs` edit triggers it
automatically). No half-finished/stubbed implementation per CLAUDE.md, so each
slice adds a real, complete increment of behavior rather than a placeholder.

1. **Shape and this-validation.** Implement `this`-is-object check (TypeError,
   no close attempted) and register the method via `define_method` so
   `typeof`/`length`/`name`/property-descriptor/non-constructor checks pass.
   Green target: `descriptor.js`, `length.js`, `name.js`, `not-a-constructor.js`,
   `receiver-not-object.js`.
2. **Separator handling and ordering.** Implement the default-`","`/`ToString(separator)`
   step, its close-and-rethrow-on-failure behavior (`iterator_close_with_completion(interp,
   this, Err(err.clone()))` then re-throw `err` — discarding the close's own
   outcome per the invariant in §2 step 2 — since `GetIteratorDirect` hasn't
   run yet, `this` is the object to close directly), and the ordering
   guarantee that separator coercion happens strictly before `next` is read.
   Green target: `separator-tostring.js`,
   `closes-on-separator-coercion-exception.js`,
   `next-lookup-after-separator-tostring.js`.
3. **Core iteration and join semantics.** Implement `GetIteratorDirect` +
   the `IteratorStepValue` loop, nullish-vs-coercible value handling, and
   separator placement (no leading separator, one separator between each
   pair of elements, no close on natural exhaustion). Green target:
   `results-no-separator.js`, `results-empty-separator.js`,
   `results-nonempty-separator.js`, `contents-nullish.js`,
   `contents-tostring.js`, `does-not-close-on-iterator-exhaustion.js`.
4. **Non-closing failure paths from the iterator itself.** Confirm (no
   production code changes expected beyond slice 3, since
   `iterator_step_value_getter`/`get_iterator_direct_getter` already propagate
   these without closing) that `next()` throwing, `next()` returning a
   non-object, and the `next` getter throwing all propagate without an
   `IteratorClose` call. Green target: `does-not-close-on-iterator-error.js`,
   `does-not-close-on-iterator-protocol-violation.js`,
   `does-not-close-on-next-getter-error.js`.
5. **Closing on value-coercion failure.** Implement the
   `iterator_close_with_completion`-preserving-original-error path for when
   `ToString` on a yielded value throws. Green target:
   `closes-on-contents-coercion-exception.js`. Full
   `test262/test/built-ins/Iterator/prototype/join/` directory should be green
   at the end of this slice (18 files, 36 scenarios per §1).
6. **`test262-extra` gap coverage.** Add the new file identified in §5,
   covering: (a) an empty-string value followed by a non-empty separator
   (`['', 'x'].values().join('-') === '-x'`) — the case that discriminates a
   correct `first: bool` flag from the tempting-but-wrong `if !R.is_empty()`
   shortcut, which would wrongly produce `'x'`; (b) a leading nullish value
   followed by a separator (`[null, 'x'].values().join('-') === '-x'`) — the
   case that catches an implementation that `continue`s past nullish values
   *before* placing the separator; (c) the separator is still coerced exactly
   once, and its coercion is still ordered before `GetIteratorDirect`, even
   when the underlying iterator is immediately exhausted
   (`[].values().join(coercible)` calls `toString` once and returns `''`).
   No production code change is expected in this slice if slices 2–3 already
   implement steps 2 and 6.c correctly per §2 — this slice exists to prove it.

## 5. Test surface

- Targeted run during development:
  `uv run python scripts/run-test262.py test262/test/built-ins/Iterator/prototype/join/`
  (18 files, 36 scenarios — see §1 for the count reconciliation against the
  issue's stale "32").
- Full-suite run before opening the PR:
  `uv run python scripts/run-test262.py` (per CLAUDE.md: run after any
  implementation work; do not pass `--update-baseline` — that is a
  `main`-only operation).
- `test262-extra/built-ins/Iterator/prototype/join/` (new): the 18 upstream
  files cover this-validation, separator default/coercion/ordering/close,
  per-value coercion/nullish/close, non-closing error propagation from the
  iterator itself, property shape, and non-constructor — but leave the
  separator-*placement* interaction with empty-string/nullish values
  under-specified (`results-empty-separator.js` only tests an empty
  *separator*, never an empty-string *value*; `contents-nullish.js` only
  places nullish values mid-sequence/trailing, never leading). Add one file
  there, following existing `test262-extra` naming/frontmatter conventions
  and citing `esid: sec-iterator.prototype.join`, asserting the three cases
  in TDD slice 6 (§4).
- `cargo test --bin jsse` after the Rust edit (crate is bin-only, per memory).

## 6. Regression risk

- **Blast radius is narrow.** The change is additive — one new
  `define_method` call on `%IteratorPrototype%`. It cannot affect existing
  `toArray`/`forEach`/`some`/`every`/`find`/`reduce`/lazy-helper behavior,
  since it reuses their exact helper functions
  (`get_iterator_direct_getter`, `iterator_step_value_getter`,
  `iterator_close_getter`, `iterator_close_with_completion`) rather than
  modifying them.
- **GC rooting:** `join`'s loop holds only local Rust `JsValue`s (`iter`,
  `next_method`, the accumulating `String`) across nested calls into JS
  (`next()`, `toString()`), exactly the same shape as the already-shipped
  `reduce`/`some`/`every`/`find` loops immediately above the insertion point.
  No new `ObjectKind`, no persistent object needs `set_helper_gc_roots` or
  `pin_native_root` — there is no lazy generator-backed helper object here to
  root in the first place.
- **Baseline movement:** could only move `test262-pass.txt` by turning the 36
  currently-failing `join` scenarios green (expected, desired) — no existing
  passing test exercises `Iterator.prototype.join` today (it's currently
  `undefined`), so there's no plausible path to a regression elsewhere from
  this change. The one soft risk is the sibling `includes` work landing on
  `main` first or after this PR — purely a README-wording coordination issue
  (§3), not a functional one, since the two methods share no code.
- **Tree-walker / property MOP / bytecode fast path:** untouched — `join` is
  a plain native builtin method call, no new AST node, no new opcode, nothing
  for `eval_expr`/`exec_statement`/`property.rs`/the bytecode compiler to
  special-case.

## 7. Out of scope

- Implementing `Iterator.prototype.includes` (#549) — already in progress on
  a separate branch/worktree; not this issue.
- Any refactor of `reduce`/`some`/`every`/`find`/`toArray`/`forEach` to share
  more code with `join` (e.g. extracting a common "eager consume loop"
  helper) — three-plus call sites already duplicate this skeleton today with
  no shared abstraction; introducing one is a separate, deliberate refactor
  with its own review surface, not bundled into a single-method feature PR.
- Any change to `scripts/run-test262.py` or CI — the runner has no
  feature-flag allowlist to update (confirmed: no `Iterator.prototype.chunks`/
  `windows`-style gating exists in the script today), so `join` needs no
  runner changes, matching the #548/#549 precedent.
- Formatting or unrelated cleanup elsewhere in `iterators.rs`.
