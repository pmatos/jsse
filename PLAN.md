# Plan: issue #609 — promote the Rooted Slot to `gc.rs`, migrate `promise.rs`'s `value_anchor` sites

## 1. Problem restated

A `JsFunction::native` closure's captures are invisible to the GC tracer, and
`pin_native_root` is append-only, so a capture that is *written after
construction* needs an arena-backed container that an anchor pins **once** and
the owner mutates in place (a **Rooted Slot**, `CONTEXT.md`). That container
exists only as the file-private `RootedPair` in `builtins/iterators.rs` (two
fixed slots, two consumers: `Iterator.concat`, `Iterator.prototype.flatMap`).
`builtins/promise.rs` re-invented a weaker form five times — `promise_all`,
`promise_all_settled`, `promise_all_keyed`, `promise_all_settled_keyed`,
`promise_any` each allocate a JS-unreachable `value_anchor` object, keep the
accumulated values in a Rust-side `Rc<RefCell<Vec<JsValue>>>`, and re-pin every
settled value onto the anchor from inside the element closure
(`pin_native_root(&anchor, &val)`). Fix: one generalised, growable
`RootedSlots` in `gc.rs`; `RootedPair` becomes an adapter over it; the five
promise sites store their accumulator *in* the slots, so the per-value pins
disappear.

## 2. Spec basis

N/A: no JavaScript behavior change — this is an engine-internal GC-rooting
refactor; every observable ECMAScript value, ordering and throw stays as-is.

Clauses whose behavior the refactor must **preserve** (regression oracles, not
things being changed): `sec-promise.all` + `sec-promise.all-resolve-element-functions`
(`[[Values]]` list, `[[RemainingElements]]`, `[[AlreadyCalled]]`),
`sec-promise.allsettled` + `sec-promise.allsettled-resolve-element-functions` /
`-reject-element-functions`, `sec-promise.any` + `sec-promise.any-reject-element-functions`
(`[[Errors]]` → `AggregateError`), `sec-iterator.concat`,
`sec-iterator.prototype.flatmap` (the persistent inner `IteratorRecord`).
`Promise.allKeyed` / `Promise.allSettledKeyed` are the await-dictionary
proposal (`esid: sec-promise.allkeyed`, as used by the existing
`test262-extra/Promise-allKeyed-*` files) — not in `spec/`; behavior preserved
verbatim. (`spec/` and `test262/` are empty submodules in this workspace; see
§5 for the init command.)

## 3. Files to touch

- `src/interpreter/gc.rs` — add `RootedSlots` next to `pin_native_root`;
  update `pin_native_root`'s doc comment (currently points at `RootedPair`);
  add unit tests in the existing `mod tests`.
- `src/interpreter/builtins/iterators.rs` — reduce `RootedPair` to a thin
  private adapter over `RootedSlots` (keeps the iterator-specific
  "slot 0 alone decides occupancy; slot 1 stored verbatim, `undefined`
  included" contract and its doc); move the arena-backing / write-barrier prose
  to `gc.rs`. Call sites (`RootedPair::new/root/get/set/clear` at ~2910–3081 and
  ~3277–3414) should not need to change. Update the `set_helper_gc_roots` doc
  reference if it names the old type.
- `src/interpreter/builtins/promise.rs` — the five sites (`value_anchor` at
  ~1274, 1414, 1613, 1774, 2067) and their `results` / `values` / `errors`
  `Rc<RefCell<Vec<JsValue>>>` accumulators.
- `src/interpreter/tests.rs` — characterization tests for
  `Promise.allKeyed` / `Promise.allSettledKeyed` across major GC (siblings of
  `promise_all_settles_across_major_gc_between_element_settlements`, ~1231).
- `test262-extra/Promise-allKeyed-combinator-gc-rooting.js`,
  `test262-extra/Promise-allSettledKeyed-combinator-gc-rooting.js` — new,
  modelled on `Promise-all-combinator-gc-rooting.js` (`flags: [async]`,
  `features: [await-dictionary, host-gc-required]`, IIFE-scoped inputs,
  `$262.gc()` between settlements).
- `CONTEXT.md` — fold `RootedSlots` into the existing **Rooted Slot** entry
  (same entry, not a second glossary term): name the type and its location,
  list the consumers (`Iterator.concat`, `Iterator.prototype.flatMap`, the five
  Promise combinators), state that it is growable/indexable, and adjust
  **Pinned Native Root**'s last sentence to point at Rooted Slot for values that
  are written after the pin. No new ADR file: this promotes an existing pattern and the vocabulary lives in
  `CONTEXT.md`. The sibling registered-roots-vs-frame-roots decision record is
  `docs/adr/2026-09-10-2014-gc-root-scope-guard.md`; the PR body states this
  "why no ADR" and links it (and, if the reviewer wants one, a short sibling ADR
  is a doc-only follow-up).
- Nothing under `spec/`, `test262/`, `test262-pass.txt`.

## 4. Design

`RootedSlots` — `#[derive(Clone)] pub(crate) struct RootedSlots(JsValue)`:

- **Backing stays an arena object** (issue item 1, load-bearing): writes go
  through `ObjectHandle::borrow_mut`, which runs `remember_if_old()`
  (`object_arena.rs:73-75`), so an old anchor taking a young value is
  remembered. Use `Interpreter::with_array_elements` / `with_array_elements_mut`
  (already barriered, already what `RootedPair` uses); the tracer visits
  `array_elements` (`gc.rs::trace_object_fields`).
- **Construction**: `create_array(vec![UNDEFINED; len])`, exactly what
  `RootedPair::new` does today and already proven by `iterator-concat-*` /
  `iterator-flatMap-*`. The issue asks to *keep* the arena backing, not to
  slim it; no hand-built `JsObjectData`. `length` and the index properties on
  the backing go stale after `push` — harmless because the object is never
  JS-reachable; say so in the doc comment.
- **Single-store invariant (the whole correctness argument).**
  `trace_object_fields` visits both `properties` values and `array_elements`
  (`gc.rs:816-830`). `set`/`push` must write `array_elements` **only** (via
  `with_array_elements_mut`); never mirror into an index property. The initial
  `undefined` property copies from `create_array` are inert, but a mirrored
  write would retain every superseded value and silently degrade the slot to
  accumulate-only pinning — the bug this issue removes. State this in the
  `RootedSlots` doc comment, and slice 1(b) asserts it directly.
- **API — add only methods with a consumer** (the PostToolUse hook runs
  `clippy -D warnings`; dead code blocks the edit): `new(interp, len)`,
  `pin_on(&self, interp, anchor)` (= `pin_native_root(anchor, &self.0)`; the
  discoverable spelling of "pin the slot, not the value"), `root(&self) -> JsValue`
  (for `gc_root_value` / `set_helper_gc_roots`), `len`, `push`, `get(i) -> JsValue`,
  `set`, `snapshot -> Vec<JsValue>`. `get` is occupancy-agnostic: it returns
  whatever is stored. `snapshot` returns an owned `Vec` and drops its
  `with_array_elements` borrow before returning — the final-resolve path is
  `snapshot` → `create_array`, and `create_array` → `alloc_object` mutates the
  arena, so a borrow held across it is a `RefCell` panic.
- **Occupancy stays in the `RootedPair` adapter, not in `RootedSlots`.**
  `RootedPair::get` keeps its `Option<(iterator, next)>` return: slot 0 alone
  decides occupancy, slot 1 is returned verbatim (`undefined` included) so the
  non-callable-`next` *TypeError* stays owed to the first call. Leaking this
  into the generic type would move that throw earlier
  (`iterator-concat-non-callable-next-method.js`,
  `iterator-flatMap-non-callable-next-method.js` guard it).
- **Promise sites**: `value_anchor` → `let slots = RootedSlots::new(self, 0);
  self.gc_root_value(&slots.root());` (keeps the per-invocation, JS-unreachable,
  loop-duration temp root — the anchor *must* still be rooted while
  `iterator_step` / `promise_resolve` run user code). Accumulator
  `results.borrow_mut().push(UNDEFINED)` → `slots.push(self, UNDEFINED)`; element
  closure captures `slots.clone()` and does `slots.set(interp, i, val)` (no
  `pin_native_root(&anchor, &val)`); final resolve uses
  `interp.create_array(slots.snapshot(interp))` — the *result* array stays a
  fresh, separate object; the slots object is never handed to JS.
  `self.pin_native_root(&on_x, &value_anchor)` → `slots.pin_on(self, &on_x)`
  (one pin per element function, as today). `keys: Rc<RefCell<Vec<JsPropertyKey>>>`
  in the keyed variants stays a Rust `Vec` (property keys are not arena object
  ids). Port the index/push order **literally**, do not normalise: the keyed variants
  read `let i = values.borrow().len()` *before* pushing → `let i = slots.len(self)`
  then `slots.push(self, UNDEFINED)`; `all`/`allSettled`/`any` keep their
  separate `index` counter and push before building the closure. allSettled(+Keyed)
  create the `{status, value|reason}` record then `slots.set(interp, i, record)`;
  no safepoint between creation and store, as today.
- The anchor property the existing comment documents is preserved: slots are
  pinned on the *element functions*, never on `cap.resolve` / `cap.reject`, so a
  constructor that reuses one resolving function across capabilities still
  accumulates zero pins on it
  (`combinator_pins_do_not_accumulate_on_a_reused_capability_function`).
- Left alone deliberately: the fixed-set `pin_native_root` calls in
  `promise.rs` (~273, 318, 334–337, 838–839, and the `&cap.resolve/&cap.reject`
  pins on element functions) — they pin a value established once, which is the
  correct use.

## 5. TDD slices

Run each slice's tests before and after; commit per slice (conventional
commits, squash-merged). Refactor slices are green→green by nature, so each one
is preceded by characterization coverage that fails if the rooting regresses.

1. **`RootedSlots` red→green** (`src/interpreter/gc.rs` `mod tests`; add via
   `run_step`-style interpreter construction, `interp.gc.request();
   interp.gc_safepoint();` for major GC). Tests: (a) slot values survive a
   major GC when only the slots object is pinned on a live anchor; (b) `set`
   replaces in place and the superseded object is collected by the next major GC
   — the test that fails if `set` ever mirrors into `properties` (single-store
   invariant), and the property `pin_native_root` cannot give; (c) `push`/`len`/`snapshot`
   round-trip and growth after a GC; (d) write barrier: `promote()` the backing
   (`object_arena.rs:125`), `set` a young object, assert the backing is in the
   remembered set (`remembered_len()`), run `gc_collect_minor`, young value
   still live. Production: `RootedSlots` in `gc.rs` + doc updates to
   `pin_native_root`.
2. **Migrate `RootedPair` → adapter** (`iterators.rs`). Guards (already green,
   must stay green): `test262-extra/iterator-concat-*`,
   `iterator-flatMap-*`, `test262/test/built-ins/Iterator/`. Production:
   `RootedPair(RootedSlots)`, `new` = `RootedSlots::new(interp, 2)`, `get`/`set`
   delegate. Keep the occupancy comment; delete the duplicated barrier prose.
3. **Characterization for the two uncovered combinators**: add
   `promise_all_keyed_settles_across_major_gc_between_element_settlements` and the
   `allSettledKeyed` twin in `src/interpreter/tests.rs`, plus the two new
   `test262-extra/Promise-*Keyed-combinator-gc-rooting.js`. These pass on the
   current code (verify that first — the values must be reachable only through
   the accumulator) and are the safety net for slices 5–6.
4. **`promise_all`** → `RootedSlots` (guards: `promise_all_settles_across_major_gc_…`,
   `test262-extra/Promise-all-*gc-rooting.js`, the pin-accumulation test).
5. **`promise_all_settled`**, then **`promise_any`** (same guards, `Promise-allSettled-*`,
   `Promise-any-*`).
6. **`promise_all_keyed`** and **`promise_all_settled_keyed`** (slice-3 tests).
7. **Docs**: `CONTEXT.md` Rooted Slot / Pinned Native Root edits; final grep that
   `RootedPair` appears only as the adapter and that no `value_anchor` and no
   `pin_native_root(&anchor…, &val|&record)` remain in `promise.rs`.

Slice order note: 1 must land before 2–6 (they call the new API); 2 and 4–6 are
independent of each other and can be separate commits in any order.

## 6. Test surface

Setup (fresh workspace): `git submodule update --init --depth 1 test262`
(spec is not needed for a run). Build with capped parallelism:
`cargo build --release -j4`, `TMPDIR` for scratch. Run gates as separate
commands; never rebuild while a suite run is in flight.

- Unit: `cargo test --release` (new `gc.rs` tests; `tests.rs` major-GC combinator
  tests; `combinator_pins_do_not_accumulate_on_a_reused_capability_function`).
- test262 targeted: `test262/test/built-ins/Promise/` (all, allSettled, any,
  race, resolve, `allKeyed`/`allSettledKeyed` if present), `built-ins/Iterator/`
  (concat, flatMap), `built-ins/AsyncFromSyncIteratorPrototype/`,
  `language/expressions/await/`, `language/statements/for-await-of/` (heavy
  Promise consumers).
- Not covered by test262 (GC-only, host-gc-required): `test262-extra/` —
  existing `Promise-{all,allSettled,any}-*gc-rooting.js`, `iterator-concat-*`,
  `iterator-flatMap-*`; new `Promise-allKeyed-combinator-gc-rooting.js` and
  `Promise-allSettledKeyed-combinator-gc-rooting.js`. Run with
  `uv run python scripts/run-test262.py test262-extra/`.
- Full: `uv run python scripts/run-test262.py` (no `--update-baseline`),
  `uv run python scripts/run-custom-tests.py`, `./scripts/lint.sh`
  (rustfmt + clippy `-D warnings`).

## 7. Regression risk

- `test262-pass.txt`: no movement expected; any diff is a rooting bug. The
  baseline is read from `origin/main` — do not roll it forward.
- GC rooting is the whole risk surface: (i) forgetting the loop-duration
  `gc_root_value(&slots.root())` (allocation-free but `iterator_step` /
  `promise_resolve` / `then` run JS that can safepoint); (ii) pinning the slots
  on only some element functions (allSettled needs both `on_fulfilled` and
  `on_rejected`; `promise_any` only `on_rejected`); (iii) holding a
  `with_array_elements` / `_mut` borrow across a call that allocates —
  including the *read* path (`snapshot` must own its `Vec` before
  `create_array`), and never calling back into JS inside a slot closure
  (`create_array` / `create_object_id` mutate the arena; build the record
  *before* the borrow, as the closures do today).
- Shared machinery leaned on: `gc_safepoint()` / `trace_object_fields`
  (Array elements), `ObjectHandle::borrow_mut` write barrier and the
  remembered set, the exhaustive `ObjectKind` match (no new variant added —
  reusing `Array`), the microtask/promise-reaction lists that keep element
  functions (and therefore the pinned slots) alive.
- Not touched: tree-walker hot paths (`eval_expr`/`exec_statement`), property
  MOP, bytecode fast path, Node-compat library harnesses. Perf: one arena
  object per combinator call, same count as the old anchor, and lighter than
  `create_array`; per-element `pin_native_root` `Vec::push` disappears.
  Promise-heavy library suites (`zod`, `moment`, `luxon`) are the only
  realistic second-order signal — spot-check one if time allows.

## 8. Out of scope

- Any change to `RootedPair`'s observable behaviour (occupancy rule, `clear`
  on completion/return) — the adapter is behaviour-identical.
- The remaining fixed-set `pin_native_root` sites in `promise.rs`
  (`finally` thunks, resolving functions, `cap.*` pins), which are correct.
- `promise_race` (no accumulator) and `Promise.try`/`withResolvers`.
- #331's other items (RAII frame guard, single mutation boundary, migrating the
  frame-roots half); this PR only delivers the registered-roots half and will
  say so in the PR body (`Closes #609`, `Refs #331`).
- The dropped/proposed `proxy-blind-callable-check`, `iterator-helper-close-policy`
  backlog items, formatting sweeps, and any change to `create_array`.
- `.architecture/backlog.md` bookkeeping.
