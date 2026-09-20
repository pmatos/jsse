# Plan: issue #650 — JetStream `lazy-collections` fails with "TypeError: Generator is already running"

## 1. Problem restated

`generators/lazy-collections.js` dies at start-up with `TypeError: Generator is already running`
(guard at `src/interpreter/eval/generator_runtime.rs:1901`/`:2228`). The guard is not misfiring on a
legitimate re-entry; something *causes* a real re-entry. Root cause (reproduced and confirmed with a
throw-away prototype, see below): `Interpreter::pending_iter_close` (`src/interpreter/mod.rs:248`) is a
single interpreter-wide `Vec<JsValue>` holding the iterators of every generator `for-of` that is
currently open. Every generator/async-generator driver drains it with
`std::mem::take(&mut self.pending_iter_close)` when it yields
(`generator_runtime.rs:783`, `:1164`, `:4294`, `:4639`, `:4764`) and parks the result in
`generator_inline_iters[<that generator's id>]`. So when generator **A** is mid-`for-of` and its body
runs generator **B** to a `yield`, **B's yield steals A's open iterators**. Later `B.return()` (an
early `return`/`break` out of a `for-of` over B, which is exactly what `slice`/`take`/`every` in
`lazy-collections.js` do) walks `generator_inline_iters[B]` and calls `.return()` on A's iterators
— iterators B never owned. Two visible outcomes:

- The stolen iterator is A's own for-of source, still `Executing` → `TypeError: Generator is already running`
  (the #650 failure: `chunk`/`take`/`windows` pipelines over nested generators).
- The stolen iterator is a suspended generator → it is silently closed early, so A's loop
  terminates prematurely and produces wrong results with no error (e.g.
  `pipe(naturalNumbers(), filter(isPrime), take(3), toArray())()` returns `[]`, node gives `[3,5,7]`).

The same defect exists for async generators (`async_generator_next_state_machine_impl`).

### Evidence gathered in this stage

- Repro without JetStream: concatenating `generators/lazy-collections.js` with
  `new Benchmark().runIteration()` fails in 0.02 s with the exact error.
- Minimal, JetStream-free repros (all cross-checked against node; jsse output wrong without the fix):
  ```js
  // sync: nested generator .return() closes the *outer* for-of iterator
  function* r(a,b){ for(let i=a;i<=b;i++) yield i; }
  function* it(){ const g=r(1,4);
    const w={[Symbol.iterator](){return this}, next(v){return g.next(v)},
             return(v){ print("w.return called"); return g.return(v)}};
    for (let d of w){ const h=r(1,3); h.next(); h.return(); yield d; } }
  // jsse: "w.return called" then outer ends after 1 item; node: no call, yields 1,2,...

  // TypeError repro (the lazy-collections shape)
  function* range(a,b){ for(let i=a;i<=b;i++) yield i; }
  const map   = {*[Symbol.iterator](){ for (let d of range(1,Infinity)) yield d*2; }};
  const chunk = {*[Symbol.iterator](){ let c=[]; for (let d of map){ c.push(d); if (c.length===2){ yield c; c=[]; } } }};
  const take  = {*[Symbol.iterator](){ let n=3; for (let d of chunk){ yield d; if (--n<0) return; } }};
  [...take]  // jsse: TypeError: Generator is already running; node: [[2,4],[6,8],[10,12],[14,16]]
  ```
  An `async function*` analogue (`for await` + nested `await h.next(); await h.return()`) is broken the same way.
- Prototype (scratch worktree, **not** part of this branch): give every generator activation a scope base into
  `pending_iter_close` (`iter_close_base`), replace the five `mem::take` sites with
  `split_off(base)`, truncate back to the base when the activation returns, wrap
  `generator_next_state_machine` and `async_generator_next_state_machine_with_promise`. Result: the
  repros above match node, the async variant matches node, and the full workload prints
  `totalLength 635` (the value `Benchmark.validate` requires) in ~1.9 s. No other blocker was found
  behind this one.
- Measured on the prototype binary vs. the unfixed binary (built from this branch at `16d0d6e`):
  - `--bytecode`: unfixed still throws on the chain repro; prototype prints the node-correct result and
    the full workload prints `ok 635`. So the fix is not bytecode-specific and the new `test262-extra`
    tests are expected to pass in CI's `--bytecode` pass (`ci.yml:84`).
  - Identical, all-pass results on both binaries for `language/statements/{for-of,generators,for-await-of,async-generator,class/elements}`,
    `language/expressions/{generators,async-generator,yield}`, `built-ins/{GeneratorPrototype,AsyncGeneratorPrototype,Iterator}`
    (e.g. for-of 1442/1442, for-await-of 2431/2431, class/elements 3054/3054, Iterator 1308/1308), and
    `test262-extra/` 328/328 in both default and `--bytecode` modes on the prototype. So the
    `truncate`-on-exit behavior showed no loss of a required IteratorClose in the suites that exercise it.
- `IteratorState::Generator` (the replay-based legacy generator, `generator_next` at
  `generator_runtime.rs:9`) has no construction site outside its own runtime file; it appears dead
  and is out of scope (see §7).

## 2. Spec basis

- `sec-generatorvalidate` (GeneratorValidate): a generator throws TypeError only if **that generator's
  own** `[[GeneratorState]]` is `executing`. A nested, distinct generator being resumed must never
  put an unrelated generator's iterators through this check.
- `sec-generatorresume` / `sec-generatorresumeabrupt` (GeneratorResume / GeneratorResumeAbrupt): `B.return(v)`
  resumes *B's* execution context with a return completion; only B's own suspended evaluation
  unwinds (closing the iterators B's `for-of` loops hold). Nothing belonging to another generator's
  execution context is touched.
- `sec-generator.prototype.return`, `sec-generator.prototype.next`.
- `sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`
  (ForIn/OfBodyEvaluation): an abrupt completion of the loop body performs `IteratorClose` on the
  loop's own iterator record; a `yield` inside the body merely suspends, and the loop's iterator is
  closed when the *suspended generator itself* is returned/thrown into, not by unrelated generators.
- `sec-iteratorclose` (IteratorClose), `sec-generatorstart`, `sec-generatoryield`.
- Async analogues: `sec-asyncgeneratorstart`, `sec-asyncgeneratorresumenext`-family, and the
  `for await` branch of ForIn/OfBodyEvaluation (`AsyncIteratorClose`).

No JavaScript syntax changes; this is a conformance bug in the engine's bookkeeping of open iterators
per generator activation.

## 3. Files to touch

- `src/interpreter/mod.rs` — add a field (working name `iter_close_base: usize`) next to
  `pending_iter_close` (`:248`, initialised `:593`) with a doc comment stating the invariant:
  entries in `pending_iter_close[iter_close_base..]` belong to the running generator activation;
  everything below belongs to enclosing activations and must not be touched.
- `src/interpreter/eval/generator_runtime.rs`
  - Add a small helper pair (private to this module or `impl Interpreter` in the same file):
    - `take_pending_iter_close(&mut self) -> Vec<JsValue>` — returns the entries at `iter_close_base..`
      **without allocating when that slice is empty** (do not use `Vec::split_off(0)`: it allocates a
      replacement with the old capacity on the per-`yield` hot path; use a `len() == base` short-circuit
      or `drain(base..).collect()`).
    - A scope enter/leave around an activation: save `iter_close_base`, set it to `pending_iter_close.len()`,
      run the activation, `truncate(iter_close_base)` (drops the activation's leftovers on every exit path,
      including throws, so nothing leaks into the caller's scope), restore the saved base.
  - Replace the five `std::mem::take(&mut self.pending_iter_close)` sites (`:783`, `:1164`, `:4294`,
    `:4639`, `:4764`) with the helper.
  - Wrap `generator_next_state_machine` (`:451`, already a wrapper around `_impl`) and
    `async_generator_next_state_machine_with_promise` (`:3273`, already a wrapper around `_impl`) in the
    scope. These two wrappers are the only entry points into the drivers that contain the `take` sites.
    `generator_return_state_machine` (`:1870`) and `generator_throw_state_machine` (`:2196`) are called
    directly from the `Generator.prototype.return/throw` builtins and run `unwind_generator_for_of_loops`
    (→ user `return()` callbacks) outside the scope. **Implementation check, not an assumption:** confirm
    nothing pushes to `pending_iter_close` during that unwinding (it only reaches the driver again via
    `generator_next_state_machine`, which is scoped); if anything can, wrap these two entry points (and the
    async `_return`/`_throw` `_with_promise` counterparts) in the same scope. The prototype, which did not
    wrap them, passed every return-heavy repro and the suites above. The async `_with_promise` wrapper recurses into itself
    only from the delegated-`yield*` prelude, before this activation pushes anything, so a nested scope
    there hands nothing over; re-verify that while implementing.
  - The `retain`/`any` uses of `pending_iter_close` in the for-of head/exit code (`:1661`, `:1791`,
    `:5634`, `:5782`) and `unroot_for_of_iterator` (`eval.rs:9438`) operate on the whole vec. Leave them
    unchanged in the first slice; see §7 for the optional scoped variant.
- `src/interpreter/eval.rs` — no change expected (`:4560`, `:9174` are pushes that are always followed by the
  driver's take inside the same activation); only touch if the tests show otherwise.
- `src/interpreter/gc.rs:377` — no change: entries of enclosing activations stay in `pending_iter_close`
  (not copied into a Rust local), so they stay GC-rooted. This is why the plan uses a base index and not
  `mem::take` + restore.
- Tests: see §4/§5 (`test262-extra/*.js`, optional unit test in `src/interpreter/tests.rs`).
- `docs/`: no ADR and no `CONTEXT.md` change (internal invariant, documented on the field).

## 4. TDD slices

Build once with `cargo build --release -j 8` before slice 1 (never rebuild while a test262 run is in
flight; snapshot the binary first). Use `uv run python scripts/run-test262.py --jsse ./target/release/jsse --test262 ./test262 <path>`;
initialise the submodule first if empty: `git submodule update --init --depth 1 test262`
(the `spec/` submodule is also empty in a fresh workspace: `git submodule update --init --depth 1 spec`).
`run-test262.py` did not find tests when given several directories at once in this workspace: run one
directory per invocation.

1. **Sync: nested generator `return()` must not close an enclosing generator's `for-of` iterator.**
   Red: new `test262-extra/generator-nested-activation-does-not-steal-for-of-iterators.js` — the
   `w.return called` repro above with a tracking iterator: assert the spy's `return` is never called,
   that the outer generator keeps producing 1, 2, 3, …, and that `h.return()` returns
   `{value: undefined, done: true}`. Green: add `iter_close_base`, the helper, the scope wrapper on
   `generator_next_state_machine`, and replace the sites in the sync driver (`:783`, `:1164`).
2. **Sync: no spurious "already running" (the #650 shape).** Red: same file (or a sibling
   `generator-nested-iterator-chain-early-return.js`): the `map`/`chunk`/`take` object chain from §1
   compared with `compareArray`/`JSON.stringify` against `[[2,4],[6,8],[10,12],[14,16]]`, plus a
   `range`/`filter`/`isPrime`-style variant where a helper function `break`s out of a `for-of` over a
   nested generator inside an outer generator. Expected to go green with slice 1; if not, the leftover
   case is in the `return()`/`throw()` paths — fix there before moving on.
3. **Sync guard (already green before the fix, must stay green): outer `return()` closes its own open
   `for-of` iterator exactly once after a nested activation ran between yields.** Checked on the
   unfixed binary: `['close:tracked']` already, because the loop-state stack (`generator_for_of_stacks`)
   closes it; the guard pins that exactly-once behavior (no double close from the inline copy) once
   the steal is gone. Same file: outer generator opens `for (x of tracked)`,
   runs a nested generator to a `yield` (and to completion) inside the body, yields; then `outer.return()`
   ⇒ tracked `return` called exactly once (log `['close:tracked']`). 
4. **Async generator parity.** Red: `test262-extra/async-generator-nested-activation-does-not-steal-for-await-iterators.js`
   (`flags: [async]`, `includes: [compareArray.js]`, finish with `$DONE`, as in the existing
   `async-function-for-of-*` extras) with the `for await` + `await h.next(); await h.return()` repro.
   Green: scope wrapper on `async_generator_next_state_machine_with_promise` and the three async `take`
   sites (`:4294`, `:4639`, `:4764`).
5. **Invariant unit test (optional, cheap).** In `src/interpreter/tests.rs` next to the existing
   `generator_inline_iters`/`generator_for_of_stacks` cleanup assertions (`:3956`, `:4012`): after running a
   script that finishes/aborts several nested generators, `interp.pending_iter_close.is_empty()` and
   `interp.iter_close_base == 0`. Catches leaks on throw exits, which the JS-level tests cannot see.
6. **Workload check (not a committed test).** With JetStream checked out at the pinned revision as
   described in the issue: `uv run python scripts/run-jetstream.py --test lazy-collections --iterations 1 --timeout 120 --engine target/release/jsse --jetstream <dir>`
   must pass (`totalLength === 635`), also with `--bytecode`. Quote the result in the PR description.
   Refactor step: read the diff for leftovers (dead locals, stale comments at the five sites).

Each slice is one commit-sized change; the implementer squashes into a clean history before the PR
(PR title, Conventional Commits, becomes the squash subject:
`fix(generators): scope pending iterator-close list to each generator activation`, body `Fixes #650`).
The implementation stage `git rm`s `PLAN.md` before opening the PR.

## 5. Test surface

Targeted test262 runs (all must not regress; the fix should only add passes if anything):

- `test262/test/language/statements/for-of/`
- `test262/test/language/statements/for-await-of/`
- `test262/test/language/statements/generators/`, `test262/test/language/expressions/generators/`
- `test262/test/language/statements/async-generator/`, `test262/test/language/expressions/async-generator/`
- `test262/test/language/expressions/yield/`
- `test262/test/language/statements/class/elements/` (generator/async-generator methods — many `yield`+`for-of` shapes)
- `test262/test/built-ins/GeneratorPrototype/`, `test262/test/built-ins/AsyncGeneratorPrototype/`
- `test262/test/built-ins/Iterator/` (iterator helpers use generator-like bookkeeping)
- then the full default run (`uv run python scripts/run-test262.py`) once, per CLAUDE.md, and `test262-extra/` both plain and with `--bytecode` (CI runs both, `.github/workflows/ci.yml:80-84`).

Not covered by test262 (spec-correct behavior, needs `test262-extra/`, frontmatter with `esid:` and an
`info:` block citing the clauses in §2, same shape as `generator-for-of-abrupt-exit-closes-iterators.js`):
nested generator activations inside another generator's `for-of` body — sync (slices 1–3) and async
(slice 4). test262 has no test where a *different* generator is `return()`ed/`next()`ed inside an open
`for-of` of the outer generator. Other gates: `cargo test --release` (unit test, slice 5) and
`uv run python scripts/run-custom-tests.py`.

Also run the neighbouring generator JetStream workloads once for regression/perf sanity
(`js-tokens`, `sync-file-system`, `async-file-system`, plus `lazy-collections` itself); no timing
comparison is required for correctness, but the per-`yield` path must stay allocation-free when
`pending_iter_close` is empty.

## 6. Regression risk

- `test262-pass.txt` baseline: expected unchanged or improved. Do **not** plan `--update-baseline`; the
  runner reads the baseline from `origin/main`.
- Hot path: generator `yield` in both drivers runs the take helper every time; the helper must not
  allocate when nothing is pending (see §3). The wrapper adds two scalar writes and a `truncate` per
  `next()`.
- Semantics the change relies on: (a) entries pushed by a for-of *head* in an activation are re-added on
  resume by the existing `already_pending` logic (`:1785-1805`, `:5775-5800`), so removing the "steal"
  must not change what a resumed activation sees; (b) truncating leftovers at activation exit
  (throw/complete paths) changes behavior only by *not* leaking iterators into whichever generator
  yields next — verify the completion/`Exit` paths still clean `generator_inline_iters`/
  `generator_for_of_stacks` as they do today (existing tests at `src/interpreter/tests.rs:3956,4012`).
- GC: enclosing activations' entries must stay in `pending_iter_close` (rooted at `gc.rs:377`); do not
  move them to an unrooted local.
- Exhaustive `ObjectKind` matches: untouched. `eval_expr`/`exec_statement`: untouched. Property MOP:
  untouched. Bytecode VM: generators reach the same drivers (issue reports identical failure with
  `--bytecode`), so run `test262-extra` with `--bytecode` too.
- Node-compat library harnesses: generators are used by acorn/uglify-js/highlight.js/luxon; the cheap
  ones (`./scripts/run-library-tests.sh acorn`, `prismjs`) are a reasonable smoke run, the multi-minute
  ones (`uglify-js`, `highlight.js`) are optional.
- If the full workload surfaces a *second*, independent defect after this fix, do not bundle it: open a
  follow-up issue and report it in the PR (the prototype run found none).

## 7. Out of scope

- Any refactor of the ~6.8k-line `generator_runtime.rs` (sync/async driver duplication, macro-izing the
  `IteratorState::StateMachineGenerator { .. }` reconstruction blocks).
- Restricting the whole-vec `retain`/`any` operations on `pending_iter_close` (for-of head/exit,
  `unroot_for_of_iterator`) to the current scope. Only observable if an enclosing and a nested activation
  iterate the *same* iterator object; separate follow-up if a failing case is constructed.
- Removing the dead-looking legacy replay generator (`IteratorState::Generator`, `generator_next`,
  `generator_return`, `generator_throw`) — confirm with a grep that it is unreachable and file a cleanup
  issue rather than expanding this PR.
- Performance tuning of `lazy-collections` beyond making it run (the prototype completes in ~1.9 s).
- Formatting-only changes, `test262-pass.txt`/baseline updates, JetStream harness or `docs/perf` changes.
