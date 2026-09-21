# Plan — issue #689: `break`/`continue` out of a yielding `try` in a generator skips the finalizer

## 1. Problem restated

In sync and async **generators**, the transform lowers an explicit `break`/`continue` that leaves a transformed
(yielding) `try` into a bare `StateTerminator::Goto(target_state)` (`TransformContext::jump_terminator`,
`generator_transform.rs:432`, gated on `is_async && detect_for_await`, which only async *functions* satisfy). The
generator drivers then jump straight to the loop target: the pending `finally` never runs, and the `TryContextInfo`
entries pushed by `TryEnter` are never popped. Async functions already route the same edge through
`route_loop_control!` / `pending_loop_control` (`eval.rs:8519`).

Probes on the current HEAD (release build, compared with `node`):

| probe | jsse | node | meaning |
|---|---|---|---|
| issue repro (`try { yield i; if (i==1) break } finally { log.push(i) }`) | `[0,1,"0"]` | `[0,1,"0,1"]` | **#689**: finalizer skipped |
| `continue outer` out of a yielding `try`/`finally` in a nested loop | `["0:0","1:0","2:0",""]` | `[…,"f00,f10,f20"]` | **#689**: no finalizer at all |
| `finally { yield* ['f'] }` on the break path | `["a","end"]` | `["a","f","end"]` | **#689**, and needs the new state to survive a delegating suspension |
| `outer: while(1){ for (x of it) { yield x; break outer } }` — no `try` at all | iterator `return()` not called | called once | **same root cause**: `align_generator_for_of_stack` matches `target_state` against each loop's `after_state`/`head_state`; a labeled break's target is the labeled statement's `after_labeled`, which matches neither, so no loop closes. `LoopControlTarget.for_of_depth` already carries the exact answer and the `Goto` throws it away |
| `for(;;){ try { yield 1; break } catch(e){…} } throw new Error('x')` (generator) | later `throw` is caught by the dead `catch` | propagates | **same root cause**: the break leaves the `try` context on `try_stack` |
| same shape as an **async function** with `await 1` | **infinite loop** (dead `catch` re-enters the loop) | propagates | `route_loop_control!` never truncates `try_stack` when no finalizer intercepts (`eval.rs:8575-8580`) |
| async function `try { try { await null; break } catch{} } finally { log.push(..) }` | finalizer runs **twice** | once | same omission: with a finalizer at depth `i`, `try_stack` is not truncated to `i+1`, so `EnterFinally` marks the wrong (innermost) entry and `TryExit` pops the wrong one |

The last two rows contradict the issue's "async functions are correct". They are folded in because the generator
routing must not be born with the same omission (see §4, slice 1).

## 2. Spec basis

All clauses are in `spec/spec.html` (esids; section numbers are computed by ecmarkup and not present in the source).

- `sec-try-statement-runtime-semantics-evaluation` (§14.15.3 `TryStatement : try Block Finally` / `try Block Catch
  Finally`): the Finally clause is evaluated for **every** completion of the Block/Catch, including `break` and
  `continue` completions; if _F_ is normal the original completion is restored (`Set F to C`), otherwise the
  abrupt _F_ replaces it. This is the clause the issue cites and is the behavior under repair.
- `sec-break-statement-runtime-semantics-evaluation` / `sec-continue-statement-runtime-semantics-evaluation`:
  produce a `break`/`continue` completion carrying `[[Target]]`; nothing in them lets the completion skip an
  enclosing `try`.
- `sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset` (step "If LoopContinues(result,
  labelSet) is false … return ? IteratorClose(iteratorRecord, status)") together with `sec-iteratorclose` /
  `sec-asynciteratorclose` and `sec-loopcontinues`: an abrupt body completion that leaves the loop — including a
  labeled `break` targeting an *enclosing* statement — closes the iterator, and a `finally` lexically inside the loop
  body runs before that close. Basis of the `break outer` probe and of the ordering tests.
- `sec-runtime-semantics-loopevaluation` / `sec-runtime-semantics-labelledevaluation`: where a `break`/`continue`
  completion is consumed (loop head vs labeled statement), i.e. which `LoopControlTarget` each jump resolves to.
- `sec-generatoryield`, `sec-generatorresumeabrupt`, `sec-asyncgeneratoryield`, `sec-yield`: a `yield` inside a
  `finally` that is running on behalf of a pending `break`/`continue` suspends *with that completion still pending*;
  resuming with `.next()` completes the finalizer and then resumes the original completion, while resuming with
  `.return()`/`.throw()` replaces it (an abrupt completion of the Finally clause wins, per the try clause above).

## 3. Files to touch

Engine:
- `src/interpreter/eval.rs` — `route_loop_control!` (async-function driver): truncate `try_stack` (slice 1).
- `src/interpreter/generator_transform.rs` — `jump_terminator` (line 432; used by `record_inline_jumps` line 414 and
  the explicit `Statement::Break`/`Continue` arms at 1035/1045) emits `LoopControl` for generators; update the
  transform unit tests that assert `Goto` (`test_yield_free_try_break_in_switch_case_records_inline_jump`, ~line 3302);
  add unit tests for the new terminators.
- `src/interpreter/types.rs` — add `pending_loop_control: Option<LoopControlTarget>` to **both**
  `IteratorState::StateMachineGenerator` and `IteratorState::StateMachineAsyncGenerator` and to both
  `completed_state_machine_*` constructors (`None`). `LoopControlTarget` is `Copy` and holds only `usize`s: no GC
  rooting, no `gc.rs` change, no new `ObjectKind`/`IteratorState` variant (the exhaustive matches are unaffected).
- `src/interpreter/eval/generator_runtime.rs` — split the fused `Goto | LoopControl` arms (lines 1276 and 5163) into two
  arms; add the shared routing helper next to `route_generator_exception` (~6710); add the `pending_loop_control` check
  to `TryExit` in both drivers (1381 / 5269); thread the new field through every snapshot literal (~90 sites, compiler
  driven — see slice 2 for the per-site rule).
- Doc comments in those two arms ("Async-function transforms are currently the only machines that emit
  LoopControl") become false; rewrite them.

Not needed (state explicitly so the implementer does not over-port): `EnterScope`/`ExitScope` are never emitted for
generators (doc comment at `generator_transform.rs:142-150`), so the generator path has no `scope_stack`, no
`unwind_scopes_to!`, no `DisposeThen::ScopeCrossLoopControl`. Only `try_stack` + `for_of_stack` matter.

Tests (see §5): new files under `test262-extra/`.

Docs: `CONTEXT.md` line 62 says the `BlockExits` side table exists "so the generator executors stay untouched" — reword
once that is no longer true, and add a short "Loop Control" entry describing `LoopControlTarget` + the routing shared by
the async-function driver (`route_loop_control!`) and the generator drivers (`route_generator_loop_control`). No ADR:
this reuses an accepted mechanism; it is not a new architectural decision.

## 4. TDD slices

Every red step is written first, run to confirm it fails **for the stated reason**, then made green. Tests that could
hang before the fix (the dead-`catch` loop) must bound the loop with a counter and record `'WRONG catch'` rather than
spin, so the red run fails fast instead of eating the runner's 120 s limit. Commit per slice (`fix(generators): …` /
`refactor(generators): …`; the PR title is the squash subject, so it must be a Conventional Commit, e.g.
`fix(generators): run finalizers for break/continue leaving a yielding try`).

**Slice 1 — async-function driver pops the try context it leaves (prerequisite).**
- Test: `test262-extra/async-function-break-out-of-yielding-try-pops-try-context.js` — (a) `for(;;){ try { await 1; break }
  catch(e){ caught++ } } throw …` must reject with the thrown error and `caught === 0`; (b) `try { try { await null;
  break } catch {} } finally { log.push('f') }` logs `f` exactly once; (c) `continue` variant of (a).
- Fix: in `route_loop_control!`, after the two `unwind_scopes_to!`/`unwind_for_of!` steps, `try_stack.truncate(idx + 1)`
  when a finalizer at depth `idx` intercepts (mirrors the throw path, `eval.rs:8826-8831`), and
  `try_stack.truncate(target.try_depth)` in the no-interception branch. Truncate *after* the unwinds: the suspending
  `unwind_scopes_to!` re-enters the macro from the parked `ScopeCrossLoopControl`, so pre-suspension state must be
  unmodified.
- Green gate: `test262-extra/`, `language/statements/for-await-of`, `language/statements/try`, `language/expressions/await`,
  the async-function dirs.

**Slice 2 — plumbing only: `pending_loop_control` in the two snapshot variants (no behavior change).**
- Add the field (types.rs) and the constructors; let the compiler enumerate the literals. Per-site rule:
  - literals that today write `pending_return: None` and are rebuilt outside a suspension of the same body
    (`.return()`, `.throw()`, completed states, error-routing rebuilds, the source-level `return` arm) → `pending_loop_control: None`
    (an injected return/throw *replaces* a pending break; `generator_throw_state_machine`'s `stored_pending_return` pass-through at ~2441 gets `None` for the new field);
  - literals that write `pending_return: pending_return.take()` (yield / await suspension) →
    `pending_loop_control: pending_loop_control.take()`;
  - `yield*` delegation snapshots and delegation-resume snapshots (sync ~520-700 and ~900-1170; async `yield_star_*`
    helpers ~2632-3250 and the async driver's own delegation arms): forward the stored value while delegation
    continues or completes into `resume_state`; `None` when the delegate's failure is being routed as a throw.
  - locals: destructure `pending_loop_control: stored_pending_loop_control` next to `stored_pending_return` in both `*_next_state_machine_impl`, keep `let mut pending_loop_control`.
- Nothing reads the field yet. Green gate: `cargo test --release`, `test262-extra/`, a targeted generator directory run — must be
  byte-identical to before.

**Slice 3 — sync generator: route `LoopControl` through un-entered finalizers.**
- Red tests (`test262-extra/`): `generator-break-through-yielding-try-finally.js` (issue repro; `continue`; labeled
  `continue outer`; `break` out of `while`, `do-while`, `switch`, labeled block; nested finalizers run innermost-first
  and the outer loop still terminates); `generator-loop-control-closes-for-of-iterators.js` (the `break outer` probe
  plus a `try/finally` inside a for-of body — finalizer output must precede the iterator's `return()`);
  `generator-break-out-of-yielding-try-pops-try-context.js` (dead-`catch` probe, plus the catch-only-nested-in-finally
  shape).
- Production:
  1. `jump_terminator`: return `LoopControl(target)` when `!self.is_async || self.detect_for_await` (async generators
     keep today's `Goto` until slice 5 so this slice cannot regress them). Update
     `test_yield_free_try_break_in_switch_case_records_inline_jump` to expect
     `LoopControl(t) if t.target_state == after_switch`; add a unit test that a `break` inside a yielding `try` in a
     sync generator lowers to `LoopControl` with `try_depth` = the loop's depth.
  2. New helper `route_generator_loop_control(&mut self, generator_id, for_of_stack, try_stack, func_env, target) ->
     LoopControlRoute` (`Resume { state, pending: Option<LoopControlTarget> } | Throw(JsValue) | Exit(i32)`), next to
     `route_generator_exception`:
     - `idx` = innermost `i` in `try_stack[target.try_depth..]` with `!entered_finally && finally_state.is_some()`;
     - `keep_len` = for a finalizer: `for_of_stack.position(|l| l.try_depth > idx).unwrap_or(len).max(target.for_of_depth)`;
       otherwise `target.for_of_depth`. **Use `target.for_of_depth`, not `align_generator_for_of_stack`, for this arm.**
       `align_generator_for_of_stack` stays for the remaining bare-`Goto` arm (normal fallthrough, `Goto(head_state)`,
       `Goto(clause_completion_state)`), where its heuristic is right; the issue's "keep align" must not be read as
       "route `LoopControl` through align" — the `break outer` probe is the case that discriminates;
     - `unwind_generator_for_of_loops(.., keep_len, Completion::Empty)` (already truncates `try_stack` to each closed
       loop's `try_depth`, runs per-iteration disposal + IteratorClose); a `Throw` from a disposer/`return()` is a throw
       completion that replaces the jump (`Route::Throw`), `Exit` propagates;
     - then `try_stack.truncate(idx + 1)` and resume at `finally_state` with `pending = Some(target)`, or
       `try_stack.truncate(target.try_depth)` and resume at `target.target_state` with `pending = None`.
  3. The driver's `LoopControl` arm: clear `pending_exception` and `pending_return` (a jump produced inside a
     finalizer replaces the exception/return that entered it — the sync driver, unlike the async one, leaves
     `pending_exception` set across the finally body), set `pending_loop_control`, call the helper; on `Throw` reuse the
     snapshot-restore + `generator_throw_state_machine` pattern already used by the `Goto` arm's close-failure branch.
  4. `TryExit`: after the `pending_exception` and `pending_return` checks (same order as `eval.rs:9177`), a
     `pending_loop_control.take()` re-runs the helper (the entry was just popped, so the next outer finalizer is found
     or the jump completes).
  5. `route_exception!` (and the source-level `return` arm) set `pending_loop_control = None`.
- Green: the three test files, `cargo test --release`, `test262-extra/` (#690's five yield-free try/with files change
  terminator kind and are the cheapest regression signal), then the targeted test262 directories in §5.

**Slice 4 — sync generator: suspension inside the finalizer, replacement semantics, injected `.return()`/`.throw()`.**
  (This is the only slice that exercises the new `IteratorState` field; without it the field is dead.)
- Red tests: `generator-loop-control-through-suspending-finalizer.js` — `finally { yield 'f' }` and
  `finally { yield* ['f'] }` on the break path (node: `["a","f","end"]`; jsse today: `["a","end"]`); finalizer that
  `throw`s / `return`s / `break`s / `continue`s replaces the pending `break` (and a finalizer's own inner loop with
  its own `break` does not disturb it); `.return(v)` and `.throw(e)` delivered while suspended inside such a finalizer
  replace the break (`return` value observed, `throw` reaches the outer `catch`); nested `try/finally` inside
  `try/finally` both suspending.
- Production: whatever slice 2's threading missed (the `yield*` delegation paths are the likely gap); nothing new
  in the design.

**Slice 5 — async generator: same routing, and remove the gate.**
- Red tests: `async-generator-break-through-yielding-try-finally.js`, `async-generator-loop-control-closes-for-await-iterators.js`
  (break out of `for await` inside a yielding `try/finally`: finalizer runs, then `AsyncIteratorClose` awaits `return()`),
  `async-generator-break-out-of-yielding-try-pops-try-context.js`; drive with `for await`/`.next()` and assert the
  settled results, the observable order of finalizer vs iterator close, and that a later `throw` is not caught by a dead `catch`.
- Production: `jump_terminator` returns `LoopControl` unconditionally (delete the gate — `detect_for_await` remains in use
  elsewhere); split the async driver's fused arm, reuse `route_generator_loop_control`; `TryExit` re-route (note this driver
  handles `pending_return` by setting `check_abrupt_on_resume` and looping — the loop-control check is a direct re-route, not that
  mechanism); route a close failure through the driver's own `route_exception!` (not the reject-immediately shortcut in the `Goto` arm);
  clear `pending_loop_control` in `route_exception!` and where `pending_return` is set (source `return`, `.return()`).

**Slice 6 — async generator: `await`/`yield`/`yield*` inside the finalizer, replacement, injected completions.**
- Red tests: `async-generator-loop-control-through-suspending-finalizer.js` — `finally { await null; log }`,
  `finally { yield 'f' }`, `finally { yield* ['f'] }` on the break/continue path; replacement by throw/return/break;
  `.return()`/`.throw()` while parked inside the finalizer (mirrors slice 4); every result observed in order through `.next()` promises.
- Production: snapshot threading gaps only (`Await` terminator, `yield*`, `async_gen_await_resume`, `yield_star_*`).

**Slice 7 — cleanup.** Rewrite the two stale arm comments; `CONTEXT.md` (Loop Control entry, line-62 wording); make sure
no `#[allow(dead_code)]` was needed. Run `./scripts/lint.sh`.

## 5. Test surface

Targeted test262 (run after each engine slice; use the release binary, never rebuild mid-run):
- `test262/test/language/statements/{for-of,for-in,for-await-of,try,break,continue,labeled,switch,while,do-while,for,async-generator,generators}/`
- `test262/test/language/expressions/{generators,async-generator,yield,await}/`
- `test262/test/language/statements/async-function/`, `test262/test/language/expressions/async-function/`
- `test262/test/built-ins/GeneratorPrototype/`, `test262/test/built-ins/AsyncGeneratorPrototype/`
- `test262/test/staging/` generator/async-generator slices only if the baseline covers them (staging is run explicitly).
- Then the full run: `uv run python scripts/run-test262.py`. Fresh workspaces have empty submodules:
  `git submodule update --init --depth 1 test262` first (read-only; never modify `test262/` or `spec/`).

Not covered by test262 (test262 has no test that observes a finalizer *ordering* or a dead `catch` across a yielding
`break`), so new files under `test262-extra/` with test262 frontmatter (`esid`, `info:` quoting the clause,
`flags: [async]` / `features: [generators]` / `includes: [compareArray.js]` as needed), modeled on
`test262-extra/async-function-loop-control-through-finally.js`. Run with `uv run python scripts/run-test262.py test262-extra/`
(no dedicated runner; `run-custom-tests.py` does not collect it). Files, by slice:
`async-function-break-out-of-yielding-try-pops-try-context.js` (1); `generator-break-through-yielding-try-finally.js`,
`generator-loop-control-closes-for-of-iterators.js`, `generator-break-out-of-yielding-try-pops-try-context.js` (3);
`generator-loop-control-through-suspending-finalizer.js` (4); `async-generator-break-through-yielding-try-finally.js`,
`async-generator-loop-control-closes-for-await-iterators.js`, `async-generator-break-out-of-yielding-try-pops-try-context.js` (5);
`async-generator-loop-control-through-suspending-finalizer.js` (6). esids: `sec-try-statement-runtime-semantics-evaluation`
for finalizer tests; `sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset` for the
iterator-close tests.

Rust: the `generator_transform.rs` unit tests (slice 3 update + new) and `cargo test --release`. `cargo test` also runs
`tests/test262_smoke_oracle.rs` (0.5 % random sample); a failure there is a real signal, re-run to distinguish flake.

## 6. Regression risk

- **Baseline (`test262-pass.txt`)**: must not move. Expected result is no change; the fixed behaviors are not in test262.
  The riskiest directories are the generator/async-generator/for-of/for-await ones listed above.
- **Terminator kind flips for all generators**: after slice 5 every `break`/`continue` emitted by the transform in a generator
  body (explicit arms *and* the #690 `inline_jumps`) is `LoopControl`, not `Goto`. The five `test262-extra` files landed by
  #690 and the unit tests around `record_inline_jumps` are the direct guard. This is also a per-jump hot-ish path in
  generator-heavy benchmarks; the helper must stay allocation-free on the no-`try`/no-for-of path (indexing + two
  truncates). Check with `benchmarks/` generator microbenchmarks if any exist before merging.
- **`for_of_depth` equality assumption**: the new arm trusts that the runtime `for_of_stack.len()` equals the transform's
  `for_of_depth` at the jump. The async-function driver already relies on it. Add a `debug_assert!(keep_len <= for_of_stack.len())`
  as the async driver does. Watch the `for-in` shape (`ForOfInit` with `is_for_in`) and for-of inside `switch` cases.
- **Snapshot plumbing (~90 literals)**: a wrong `None` silently drops a pending break across a suspension (the `yield*`
  delegation snapshots are the likeliest); a wrong `take()` re-arms a stale break. Slices 4 and 6 exist to make the field
  observable; review the diff site-by-site against the rule in slice 2.
- **Shared machinery**: touches the generator drivers (`generator_runtime.rs`), `route_loop_control!` in the async-function
  driver, and `IteratorState`. It does **not** touch the property MOP, `gc_safepoint()` rooting (the new field holds no
  `JsValue`), the exhaustive `ObjectKind` matches, the bytecode fast path (feature-flagged off by default; confirm with
  `grep -n "Generator\|yield" src/interpreter/bytecode/` that it bails on generator bodies before relying on this), or the
  Node-compat library harnesses' shims. Run at least one generator/async-heavy library (`./scripts/run-library-tests.sh acorn`,
  `decimal.js` are the fast ones) as a smoke check before merging; `uglify-js`/`highlight.js` are too slow to gate on.
- **Slice 1 changes the async-function driver** that the issue says is correct; the change is a strict fix (a `try_stack` that
  is now exact) but `async-function-*` and `await-using-*` files in `test262-extra/` (the #683/#701 scope-state work) are the
  guard, particularly `DisposeThen::ScopeCrossLoopControl` re-entry.

## 7. Out of scope (follow-ups, not bundled)

- `pending_return` (and `pending_exception`) being **dropped across a delegating `yield*` inside a `finally`**
  (`function*g(){ try { return 1 } finally { yield* [2,3] } }` → jsse's final `{done:true}` has no `value`; node `1`). Independent
  of break/continue; probe kept in the run notes. Separate issue.
- `generator_throw_state_machine` re-storing `stored_pending_return` when a `throw()` is routed to a handler, and the sync
  driver leaving `pending_return` set when an exception thrown inside a finalizer is caught by an outer `catch` — same
  "replacement" class for returns; not touched here beyond `pending_loop_control`.
- `align_generator_for_of_stack`'s state-id heuristic for bare `Goto`s (left as is; only the `LoopControl` arm stops using it).
- Any `IteratorState` snapshot-literal deduplication (e.g. a `suspended_*` constructor); refactors of `generator_runtime.rs`;
  formatting-only changes; the await-using / #665 / #685 / #686 microtask-drain work.
- No `test262-pass.txt` update (never on a feature branch), no `spec/` or `test262/` edits, no new dependencies.

## Notes for the implementation stage

- PR title is the squash subject: `fix(generators): run finalizers for break/continue leaving a yielding try`. `git rm PLAN.md` before opening the PR.
- Build with `cargo build --release -j6` into `$TMPDIR` (`CARGO_TARGET_DIR=$TMPDIR/target`); a cold release build (all deps, `-j6`, no debug info) took ~1 m 05 s on this host. Never rebuild while a test262 run is in flight — snapshot the binary first.
- Leave a `gh issue comment 689` when opening the PR recording: probes r3/s1 folded in as same root cause, the `route_loop_control!`
  truncation pulled in because the generator path depends on it, and the deferred `yield*`-in-finalizer `pending_return` loss.
