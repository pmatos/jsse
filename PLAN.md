# Plan: issue #711 — delete the dead legacy `IteratorState::AsyncGenerator` path

## 1. Problem restated

`IteratorState::AsyncGenerator { body, func_env, is_strict, execution_state }`
(`src/interpreter/types.rs:1471`) is the pre-state-machine representation of an
async generator: it re-runs the whole function body via `exec_body` and
fast-forwards past already-fired yields (`GeneratorContext`), settling its
promise with blocking `await_value` + inline `drain_microtasks()`. Every async
generator is now built as `IteratorState::StateMachineAsyncGenerator`
(`eval.rs:5529`), driven by the request queue. Nothing constructs the legacy
variant except the legacy code itself, which only ever *rewrites an existing*
`AsyncGenerator` state, so it is unreachable. Delete the variant and everything
that exists only to serve it (behaviour-preserving refactor, follow-up of #687).

Verified by grep (`IteratorState::AsyncGenerator`): the only occurrences are
`types.rs:1471` (variant), `gc.rs:1097` (root arm, shared with `Generator`),
and `generator_runtime.rs` in three legacy bodies below. No hit in
`builtins/`, `bytecode/`, `eval.rs`, `exec.rs`, `scheduler.rs`, `tests.rs`, or
`tests/`.

Dead code to delete, all in `src/interpreter/eval/generator_runtime.rs`
(line numbers at `2acae0e3`):

| Item | Lines | Notes |
| --- | --- | --- |
| legacy tail of `async_generator_next` (from `let Some(IteratorState::AsyncGenerator …) = state else`) | 5889–6067 | 5 × `drain_microtasks()`, 2 × `await_value`, `exec_body`, `generator_context = Some(GeneratorContext { is_async: true, … })` |
| fallthrough in `async_generator_return` → `async_generator_return_legacy` | 6150–6151 | |
| `async_generator_return_legacy` | 6348–6416 | 1 × `drain_microtasks()` |
| legacy tail of `async_generator_throw` | 6441–6472 | 1 × `drain_microtasks()` |

Measured total: 8 `drain_microtasks()` + 2 blocking `await_value` (issue says
"~10 more"; the 2 in `async_generator_await_return` are **kept** — see below).

Must **not** be deleted (still live):

- `async_generator_await_return` (6074): called from the state-machine paths at
  `:5723` and `:6210`.
- `GeneratorExecutionState`, `GeneratorContext`, `GeneratorResumeKind`,
  `exec_body`, `await_value`, `create_iter_result_object`,
  `reject_with_type_error`: still used by the sync `IteratorState::Generator`
  path (`:43–460`) and the state-machine inline-yield fallback (`:3816`).
- `IteratorState::Generator` and the `Generator` half of the `gc.rs` arm.

## 2. Spec basis

`N/A: no JavaScript behavior change` — the deleted code is unreachable from
script, so no syntax or semantics move.

The surviving entry points must keep their current observable behaviour,
governed by (all in `spec/spec.html`):

- `%AsyncGeneratorPrototype%.next`, `.return`, `.throw`
  (`sec-asyncgenerator-prototype-{next,return,throw}`, ES2025 §27.6.1.2–4):
  step 2 creates the capability; step 3–4 `AsyncGeneratorValidate` +
  `IfAbruptRejectPromise` — a non-generator `this` yields a **rejected promise
  with a TypeError, never a synchronous throw**.
- `AsyncGeneratorValidate` (`sec-asyncgeneratorvalidate`): requires
  `[[AsyncGeneratorContext]]`, `[[AsyncGeneratorState]]`, `[[AsyncGeneratorQueue]]`
  slots. In jsse those slots ≙ `IteratorState::StateMachineAsyncGenerator`.
- `AsyncGeneratorEnqueue` (`sec-asyncgeneratorenqueue`): what
  `async_gen_enqueue` already implements.

## 3. Files to touch

- `src/interpreter/eval/generator_runtime.rs` — the deletions above; reduce
  `async_generator_next` / `async_generator_return` / `async_generator_throw`
  to *validate → enqueue*. Exact post-deletion shape, identical for all three
  (`X` ∈ `next` / `return` / `throw`): `this` not an object, or object id not
  resolvable → `reject_with_type_error("AsyncGenerator.prototype.X called on non-object")`;
  object whose state is not `StateMachineAsyncGenerator` →
  `reject_with_type_error("not an async generator object")` (what the legacy
  bodies' re-check produced today — `return`/`throw` must **not** fall back to
  the "called on non-object" text); otherwise
  `async_gen_enqueue(this, value, AsyncGenRequestKind::{Next,Return,Throw})`. Replace
  `let state = obj_rc.borrow().iterator_state().cloned();` +
  `if let Some(StateMachineAsyncGenerator{..}) = &state` with a
  `matches!(obj_rc.borrow().iterator_state(), Some(IteratorState::StateMachineAsyncGenerator { .. }))`
  bound to a `bool` first (the whole-state clone existed only to feed the
  legacy destructure; the temporary borrow must drop before `async_gen_enqueue`).
  Also drop the stale `// Non-state-machine … legacy` / `// NOTE: The old state
  machine path…` comments.
- `src/interpreter/types.rs` — remove the `AsyncGenerator { … }` variant (`:1471–1476`).
- `src/interpreter/gc.rs` — `collect_iterator_state_roots` (`:1092–1108`): the
  `Generator | AsyncGenerator` or-pattern collapses to `Generator` alone
  (sync legacy state must still root `func_env` and `prev_sent`).
- `test262-extra/async-generator-prototype-methods-reject-non-async-generator-receivers.js` — new characterization test (slice 1).
- No `docs/adr/` or `CONTEXT.md` change: no new decision or vocabulary.
  `docs/adr/2026-09-21-2300-yield-star-delegated-step-suspension.md:46` mentions
  the legacy path under "Known boundaries"; it is a dated historical record —
  leave it, and say so in the PR body.
- `PLAN.md` — `git rm` before opening the PR (stage-handoff artefact).

## 4. TDD slices

This is a deletion; "red" does not exist for the removal itself. The discipline
is characterization-first (green→green), so the only non-vacuous risk — the
`else` arm of the dispatchers, which is the *only* place legacy and live code
meet — is pinned before it is touched.

Create a task list (TaskCreate) from these slices before executing; slice 2
blocks slice 3.

1. **Pin the receiver-validation contract (green on current binary).**
   `test262-extra/async-generator-prototype-methods-reject-non-async-generator-receivers.js`,
   test262 frontmatter, `esid: sec-asyncgeneratorvalidate`, `flags: [async]`,
   `features: [async-iteration]`. Deliberately small: primitives, plain objects
   and functions are already covered by the baselined test262
   `this-val-not-object.js` / `this-val-not-async-generator.js`, so they are
   *not* repeated. For each of `next`, `return`, `throw` (taken from
   `Object.getPrototypeOf(async function*(){}).prototype`) and each receiver —
   a **sync generator object** (state `StateMachineGenerator`: the only
   constructible `IteratorState` that reaches the dispatcher `else` arm and the
   one a "widen the discriminant to any generator-ish state" slip would
   wrongly accept), `Object.create(asyncGenObj)`, `new Proxy(asyncGenObj, {})` —
   assert the call **returns a Promise without throwing** and rejects with
   `TypeError` (`assert.sameValue(e.constructor, TypeError)`). Then assert a
   *real* async generator still works through the same three entry points
   (`next` → `{value, done:false}`, `return(v)` → `{value:v, done:true}`,
   `throw(e)` → rejects `e`) so the test proves the discriminating branch, not
   just the rejection. One sequential `$DONE` chain. Run with
   `uv run python scripts/run-test262.py test262-extra/async-generator-prototype-methods-reject-non-async-generator-receivers.js`
   against the **pre-change** binary; it must be green. Only keep assertions
   that are green pre-change — if a receiver exposes a pre-existing bug, drop
   it from this file and note it as a follow-up in the PR; do not fix it here.
2. **Delete the dead path (one atomic change).** Apply all edits in §3 to
   `generator_runtime.rs`, `types.rs`, `gc.rs`. The fmt/clippy hook exits 2 on
   intermediate states (variant never constructed, unused fn) but the file is
   still written — only the final state must be warning-free. Order that keeps
   intermediates readable: (a) reduce the three dispatchers and delete
   `async_generator_return_legacy`; (b) delete the `types.rs` variant;
   (c) fix the `gc.rs` arm. Then `./scripts/lint.sh`
   (`cargo fmt --check` + `clippy -D warnings`, `--all-targets`).
3. **Confirm nothing regressed / nothing dead was left behind.**
   - `grep -rn "AsyncGenerator {" src | grep -v StateMachine` and
     `grep -rn "IteratorState::AsyncGenerator\b\|async_generator_return_legacy" src`
     both empty.
   - `grep -c "drain_microtasks\|await_value" src/interpreter/eval/generator_runtime.rs`
     drops by 8 and 2 respectively (report before/after in the PR).
   - `cargo build --release --features perf-counters` still compiles
     (attribution code names generator bodies; grep shows no dependency, this
     proves it).
   - Slice 1 test still green on the post-change binary.

## 5. Test surface

Targeted test262 (run with `uv run python scripts/run-test262.py <dir>`; init
the submodule first: `git submodule update --init --depth 1 test262`):

- `test262/test/built-ins/AsyncGeneratorPrototype/` (all of `next`, `return`,
  `throw`, esp. `this-val-not-object.js`, `this-val-not-async-generator.js`,
  `request-queue-*`, `return-suspended*`, `return-state-completed*` — all in the
  baseline)
- `test262/test/built-ins/AsyncGeneratorFunction/`
- `test262/test/built-ins/AsyncFromSyncIteratorPrototype/`
- `test262/test/language/statements/async-generator/`,
  `test262/test/language/expressions/async-generator/`
- `test262/test/language/statements/for-await-of/`,
  `test262/test/language/expressions/yield/`,
  `test262/test/language/expressions/await/`
- `test262/test/language/statements/class/` and `expressions/class/`
  (`async-gen-method*`, `elements/*async-gen*`), `expressions/object/method-definition/`
  (`async-gen-*`)
- `test262-extra/` (whole dir — every `async-generator-*` / `AsyncGenerator-*`
  file there exercises the state-machine driver the dispatchers feed).

Not in test262 → new `test262-extra` file (slice 1): AsyncGeneratorValidate
rejection for *sync generator*, iterator-helper, proxy, and prototype-inheriting
receivers, plus the live-path sanity. test262's `this-val-not-async-generator`
only uses a plain object; the sync-generator receiver is the one that reaches
the same `else` branch through a *different* `IteratorState` variant.

Other gates: `cargo test --release` (includes `gc.rs` unit tests
`pending_async_generator_request_keeps_generator_and_promise_alive`,
`freed_async_generator_drops_its_request_queue`, and the async-generator
`host_exit_*` tests in `interpreter/tests.rs`), `uv run python scripts/run-custom-tests.py`,
`./scripts/lint.sh`, then the **full** `uv run python scripts/run-test262.py`
last (default baseline = `origin/main:test262-pass.txt`; compare, do not
update). Cap build parallelism (`cargo build --release -j4`); do not rebuild the
binary while the full run is in flight.

## 6. Regression risk

Low: the removed variant is never constructed, so no runtime path can reach the
removed code. Expected `test262-pass.txt` movement: **none** (no scenario should
change status either direction). Residual risks:

- **Dispatcher `else` arm**: the only live code touched. A slip (e.g. accepting
  `StateMachineGenerator`, or throwing instead of rejecting) would flip
  `this-val-not-*` scenarios and slice 1's test.
- **GC rooting**: `gc.rs` `collect_iterator_state_roots` must still root
  `func_env` + `prev_sent` for `IteratorState::Generator`; merging the or-pattern
  wrongly would drop them. `gc_safepoint()`/`trace_object_fields` are otherwise
  untouched (the exhaustive `ObjectKind` match sits above `IteratorState`).
- **Exhaustive `IteratorState` matches**: removing a variant fails to compile
  anywhere it is matched without `_`; grep found only `gc.rs`, compiler will
  confirm.
- **Async-generator drivers**: `async_generator_next_state_machine_impl`,
  `async_gen_enqueue`, `async_gen_process_queue`, the scheduler queue — read-only
  for this change. Tree-walker hot paths (`eval_expr`/`exec_statement`),
  `property.rs` MOP, bytecode fast path, and the Node-compat library harnesses
  are not touched; no library run needed beyond an optional `acorn` sanity run
  (async-generator syntax is parsed but not executed there).
- **Timing**: deleting inline `drain_microtasks()` in unreachable code cannot
  change microtask ordering; `request-queue-*` and `test262-extra/async-generator-*`
  tick-order tests are the guard.

## 7. Out of scope

- Unifying the three now-similar dispatchers behind one helper (`async_generator_next`/`return`/`throw`). Not needed to close #711; keep them as three short functions with the messages above.
- The **sync** `IteratorState::Generator` legacy path (`generator_next` /
  `generator_return` / `generator_throw`, `generator_runtime.rs:~40–460`, plus
  `GeneratorExecutionState` and the `#[allow(dead_code)] SuspendedStart`): also
  appears never constructed outside its own functions (grep: only
  `generator_runtime.rs` and the `gc.rs` root arm). It is the natural sibling
  cleanup; file it as a follow-up issue (or mention in the PR body), do not
  bundle it here.
- Remaining blocking `await_value` / `drain_microtasks` callers in
  `eval.rs`, `exec.rs`, `dispose.rs`, and `async_generator_await_return`
  (#687's other items).
- Removing `GeneratorContext` / the `InlineYield` fallback (#625).
- Editing the dated ADR that mentions the legacy path.
- Any formatting-only or unrelated cleanup in `generator_runtime.rs`; baseline
  updates (`--update-baseline`).

## PR

Branch `sym/jsse/711-refactor-delete-the-dead-legacy-iteratorstate-asyncgenerator-path`.
Title (squash subject): `refactor(generators): delete the dead legacy IteratorState::AsyncGenerator path`.
Checked against `commitlint.config.cjs` (`@commitlint/config-conventional`):
subject starts lowercase (`subject-case` forbids sentence/start/pascal/upper),
no trailing full stop, header is 79 chars, 86 with the ` (#NNN)` squash suffix
(limit 100); `lint-pr-title.yml` lints both forms.
Body: `Closes #711`; summarize deleted items and the before/after
`drain_microtasks`/`await_value` counts; note "no behaviour change, baseline
unchanged"; list the sync-`Generator` follow-up. `git rm PLAN.md` first.
