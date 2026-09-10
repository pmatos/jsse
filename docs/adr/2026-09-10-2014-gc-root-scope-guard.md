# GC temp-root scoping: a closure combinator, not an RAII guard

Issue #331 asked whether the GC temp-root frame idiom — `let f =
self.gc_root_frame(); … self.gc_unroot_frame(f);`, paired and placed by hand
on every exit path — should become an RAII `Drop`-guard, so a forgotten
teardown (premature collection or a root leak) can no longer happen by
omission. #290 first reconciled the one-line `gc_root_value(&x)` idiom; this
records the decision for the frame-scoping half of that question, now that
`with_gc_root_scope` has a second call site beyond its `array.rs` origin.

## Decision

`Interpreter::with_gc_root_scope(|interp| …)` — a closure combinator that
captures the current temp-root depth, runs `body`, and bulk-unroots on every
exit path the closure takes (tail, early `return`, `?`) — is the seam for
whole-body, single-frame native temp-root scoping. It is not a Drop-guard:
`gc_temp_roots` stays a plain `Vec<u64>`, so there is no interior-mutability
borrow tax (`RefCell`) on the GC hot path (`gc_root_value`, called from every
allocation-adjacent site in the interpreter). The raw `gc_root_frame`/
`gc_unroot_frame` primitive is retained for frames that genuinely nest or
interleave across early exits — the one case the combinator structurally
cannot express, since it owns exactly one depth marker.

An RAII `Drop`-guard is added only if a genuinely interleaved case with two
or more real adapters later surfaces, decided where the variation is
observed rather than pre-committed here. PR #595's design-it-twice review
(`.architecture/reviews/2026-09-04-gc-root-scope-guard.md`) scored this
against an RAII guard (Design B, rejected: forces `Rc<RefCell<Vec<u64>>>`,
touches `gc.rs` and ~15 functions across six files for a capability
`array.rs` never used) and a value-taking `with_gc_rooted(&[&JsValue], …)`
variant (Design C, runner-up: its slice seals a ≥2-root variation that had
zero real adapters in `array.rs`).

## Applied so far

- **`array.rs`** (PR #595): 8 whole-body natives — `concat`, `slice`, `map`,
  `filter`, `splice`, `flat`, `flatMap`, `Array.from`'s array-like path —
  collapsing 71 hand-threaded `gc_unroot_frame` teardowns to 9. The 9
  remaining are `Array.from`'s nested iterator frames (`gc_frame` +
  `gc_frame_next` held simultaneously), which correctly keep the raw
  primitive.
- **`eval.rs`'s `yield*` delegation** (this change): the concrete first slice
  of the `gc-root-scope-guard-eval` follow-up PR #595 proposed, and the
  control-flow-heavy path #331 itself named as the prototype target — 10
  `gc_unroot_frame` call sites across the iterator-protocol loop, one per
  abrupt exit (`IteratorNext` throwing, an awaited rejection, `done`/`value`
  getters throwing, and each of `next`/`return`/`throw` resume kinds)
  collapse to plain `return`s inside `with_gc_root_scope`. The manual
  `if let Some(o) = iterator.as_object_id() { self.gc_temp_roots.push(o.id) }`
  bypass at this site is replaced with `gc_root_value(&iterator)`, retiring
  one of the two seam bypasses PR #595 flagged (`eval.rs:1066`/`:4324`).

  **Reachability caveat, found during review of this ADR.** #331 described
  this loop as *the* control-flow-heavy `yield*` path when it was filed.
  Today, ordinary `yield*` execution goes through `generator_runtime.rs`'s
  state-machine `StateTerminator::Yield { is_delegate: true, .. }` arm
  instead — a separate, independently-suspending implementation (spec
  §14.4.14) that stores `delegated_iterator` on the generator object rather
  than calling back into `eval_expr`. This `eval.rs` loop is reached only
  through the documented `InlineYield` fallback
  (`generator_runtime.rs:4301-4304`: "any `Completion::Yield` from
  `exec_statements` ... came from a loop body or complex control flow that
  isn't decomposed by the state machine transformer"). Direct instrumentation
  of this exact branch found zero hits across a bare `yield*`, `while`/
  `do-while`/nested-`while` loops, `try`/`finally`, `switch`, `for-in`,
  labeled `continue`, `yield*` as a binary/call/ternary/array-literal operand,
  and an async generator — so no construction found so far exercises it. The
  migration is still correct and harmless (identical behavior on every exit
  path, full test262 green), but its practical value is unconfirmed rather
  than the "control-flow-heavy path" framing alone would suggest; see
  jsse#625 for the open question of whether/when this fallback fires and
  whether the legacy `self.generator_context`-driven branch is still load-
  bearing.

## Deliberately not migrated

- **`eval.rs`'s `destructure_array_assignment`** (the other flagged bypass,
  `:4324`) **and `exec.rs`'s `bind_pattern`** array-destructuring path root
  their iterator the same way, but unroot it with `gc_unroot_value` — a
  per-value identity removal, not a blanket frame truncation. Array pattern
  binding can hold other, unrelated temp roots live across its own body (a
  bound value that itself triggers rooting), so collapsing it into
  `with_gc_root_scope`'s single bulk-truncating frame risks unrooting a
  value a sibling branch still needs. These stay on the raw primitive until
  that interaction is worked out on its own, not bundled into this slice.
- **`array.rs`'s `from_async_gc_root`/`from_async_gc_unroot`** (`Array.fromAsync`)
  pin a `FromAsyncState`'s object fields for the life of a multi-tick async
  continuation, which spans suspend points `with_gc_root_scope`'s single
  synchronous closure cannot cover. `pin_native_root`/`gc_native_roots` (the
  anchor-object pinning PR #473 introduced, and `RootedPair` in
  `iterators.rs` already builds on) is the right target mechanism for this
  case — but migrating `from_async_gc_root` onto it is #331's item 3 (split
  frame-roots from independently-registered roots) applied to a specific
  call site, not this ADR's concern. Left as follow-up.
- **`exec.rs`'s destructuring-pattern iterator root**, **`array.rs`'s
  `from_async_gc_root`** loop (see above), and **`regexp.rs`'s global-match
  result loop** still use the raw `gc_temp_roots.push(id)` / manual-frame
  idiom #290 and #331 catalogued as remaining mechanical-sweep sites. None
  are touched here; they are unrelated to the `with_gc_root_scope` seam
  decision and remain future opportunistic cleanup.

## Consequences

- `gc-root-scope-guard-eval`'s remaining scope — the rest of `eval.rs` and
  ~9 other files PR #595 estimated — is unchanged by this slice; each
  future site is evaluated individually against the same criterion applied
  here (single frame, no cross-branch identity removal, no continuation
  spanning multiple ticks).
- #331's item 4 (root-stack balance assertions at evaluation boundaries) is
  **not** unblocked by this change. `gc_temp_roots` still conflates
  frame-scoped roots (what this ADR's combinator manages) with
  independently-registered ones — the exact hazard #465 found and fixed for
  promise-resolver roots sharing the same truncatable stack as an enclosing
  `eval_call` frame. Until #331's item 3 separates the two root kinds,
  neither `==` nor `>=` holds at a call boundary while `Array.fromAsync` or
  an `Atomics.waitAsync`-style continuation is pending, so balance
  assertions would false-positive. Revisit once item 3 lands.
