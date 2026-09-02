# Plan: #539 — cut the bytecode VM's per-entry cost below the ~16-opcode break-even

## 1. Problem restated

`run_chunk_inner` (`src/interpreter/bytecode/vm.rs:201`) allocates two fresh
`Vec`s on every compiled-body entry — `Vec::with_capacity(chunk.max_stack)` for
the operand stack and `Vec::with_capacity(chunk.max_refs)` for the reference
stack — and re-runs a `var_names` declaration prologue. The #526 audit
(`docs/perf/2026-08-26/mandreel-bytecode-work-share.md`) fit this fixed
per-entry cost at **b ≈ 230–505 ns**, against a **a ≈ 20–24 ns** saving per
opcode dispatched, putting break-even at **~16 opcodes/body**. mandreel's
compiled bodies average 15.87 opcodes, so its opcode saving (~4.5 s) and its
entry cost (~4.5 s) cancel almost exactly, and `--bytecode` shows no
measurable end-to-end gain despite compiling 96.5% of invocations. The fix is
to remove the unambiguously VM-*added* cost (the two allocations) without
changing any observable behavior, then re-measure to see how much of `b` that
recovers.

## 2. Spec basis

`N/A: no JavaScript behavior change`. This is an internal buffer-reuse change
to the bytecode VM's execution machinery (`src/interpreter/bytecode/vm.rs`,
`src/interpreter/mod.rs`). No opcode's semantics, no `Completion` value, and no
observable timing-independent behavior changes — every existing bytecode test
in `src/interpreter/bytecode/tests.rs` must still pass unmodified, and
`test262-pass.txt` must not move in either direction.

## 3. Files to touch

- `src/interpreter/bytecode/vm.rs` — extract the opcode-dispatch loop out of
  `run_chunk_inner` into a new private `run_chunk_loop`, and change
  `run_chunk_inner` to acquire pooled operand/reference stacks before calling
  it and release them unconditionally after.
- `src/interpreter/mod.rs` — add the pool fields (`vm_operand_stack_pool`,
  `vm_ref_stack_pool`) beside `function_env_pool`/`gc_bytecode_roots`, their
  cap constants beside `MAX_POOLED_FUNCTION_ENVIRONMENTS`, and
  `acquire_vm_operand_stack`/`release_vm_operand_stack` +
  `acquire_vm_ref_stack`/`release_vm_ref_stack` methods mirroring
  `acquire_function_environment`/`recycle_function_environment`.
- `src/interpreter/bytecode/tests.rs` — new pool-correctness tests (slice 2a).
- `src/interpreter/perf_counters.rs` — new entry-cost timing spans, feature-gated
  (slice 1), *only if* the measurement is judged worth shipping — see slice 1's
  exit condition below.
- `docs/perf/2026-09-02/` — new directory for this issue's measurement
  artifacts (raw counter/timing dumps + a short `entry-cost-breakdown.md`),
  following the `docs/perf/2026-08-26/` precedent.
- No changes to `src/parser/`, `src/lexer.rs`, `src/ast.rs`, or any
  `builtins/*` — this issue is confined to the bytecode VM's frame-entry path.

## 4. Design decision: free-list pool, not a shared base-offset stack

The issue's own "Suggested approach" section proposes a single growable
operand/ref stack shared across all frames, indexed from a saved base offset
per frame (like a real bytecode VM's value stack). That design requires **every
one of `run_chunk_inner`'s ~15 early-return sites** (one per opcode arm that
can produce an abrupt `Completion`, plus the `TailCall` trampoline exit) to
explicitly truncate the shared stack back to its frame's base before
returning — miss one, and a throw leaves that frame's leftover operands sitting
above the base, where the *next unrelated frame* (not a nested callee, a later
sibling) will silently treat them as its own bottom-of-stack values. That's a
correctness hazard, not just a missed optimization, and it re-opens on every
future opcode added to the match.

This plan instead pools **whole, independent `Vec`s**, exactly like the
existing `function_env_pool` (`src/interpreter/mod.rs:267`,
`acquire_function_environment`/`recycle_function_environment` at
`src/interpreter/mod.rs:1977-2001`): a frame checks out a `Vec<JsValue>` and a
`Vec<IdentifierRef>` from a free list (or allocates fresh if the list is
empty), uses them privately for its entire execution, and returns them
(cleared) to the free list when it's done. Two properties fall out of this
directly:

- **No cross-frame leak is possible.** Each frame's `Vec` is never visible to
  any other frame. If a frame fails to release its `Vec` (e.g. because a future
  refactor adds a new early-return arm that forgets to release), that `Vec` is
  simply dropped instead of recycled — the frame's *next* entry pays one more
  allocation than it needed to. It can never observe another frame's leftover
  values, because there is no shared indexed buffer to observe.
- **No per-arm changes are needed.** Every one of the ~15 return points in the
  dispatch loop stays exactly as it is today. The pool acquire/release is
  hoisted to the two lines immediately around a single call to the (extracted)
  dispatch loop, mirroring the pattern `run_chunk_with_var_prologue` already
  uses for `gc_bytecode_roots`:

  ```rust
  fn run_chunk_with_var_prologue(...) -> Completion {
      let gc_frame = interp.gc_bytecode_roots.len();
      let result = run_chunk_inner(interp, chunk, env, this_value, declare_chunk_vars);
      interp.gc_bytecode_roots.truncate(gc_frame);   // unconditional, whatever `result` is
      result
  }
  ```

  `run_chunk_inner` gets the identical shape: call the dispatch loop exactly
  once, release the pooled `Vec`s exactly once afterward, regardless of which
  arm produced the `Completion`. A normal (non-panicking) `Completion::Throw`
  from deep recursion propagates by ordinary `return` up through every nested
  `run_chunk_inner`, so every level's release runs as that level's call frame
  returns — the pool drains correctly even under a 5000-deep recursive throw
  (`CALL_DEPTH_HARD_LIMIT`, `src/interpreter/mod.rs:401`) without any special
  unwind handling.

**Invariant: clear on release, not on acquire.** `IdentifierRef::SpecificEnv`
(`src/interpreter/eval.rs:25-29`) holds an `EnvRef` (`Rc<RefCell<Environment>>`).
A pooled ref-stack `Vec` sitting in the free list with a stale `SpecificEnv`
entry still in it keeps that `Rc`'s strong count elevated, which would
silently defeat `recycle_function_environment`'s
`Rc::strong_count(&env) != 1` guard (`src/interpreter/mod.rs:1994`,
called from `src/interpreter/eval.rs:6043` and `:6088`) for an unrelated
function environment recycle happening later in the same run. Both pooled
`Vec`s must be `.clear()`ed at `release_vm_operand_stack`/`release_vm_ref_stack`
time, before being pushed back onto the free list — never deferred to the next
acquire.

**Capacity is reserved, not assumed.** The original code guarantees
zero-reallocation execution via `Vec::with_capacity(chunk.max_stack)` /
`max_refs`, computed as a static upper bound by the compiler
(`src/interpreter/bytecode/compiler.rs:133-134,145`). A pooled `Vec` acquired
from a smaller prior chunk must call `.reserve(needed)` (a no-op if already
sufficient) to preserve that guarantee, rather than relying on `push`'s
amortized growth to eventually catch up.

**Pool caps mirror the existing precedent.** Add
`MAX_POOLED_VM_OPERAND_STACKS` / `MAX_POOLED_VM_REF_STACKS` (start at 256,
matching `MAX_POOLED_FUNCTION_ENVIRONMENTS`) so one outlier chunk with an
unusually large `max_stack` doesn't pin an oversized buffer in the pool for
the rest of the process; also cap the *capacity* a `Vec` is allowed to carry
back into the pool (mirroring
`MAX_POOLED_FUNCTION_BINDING_CAPACITY = 256` at `src/interpreter/mod.rs:433`),
dropping instead of pooling a `Vec` that grew unusually large.

## 5. What this plan deliberately does *not* attempt, and why

Investigation before writing this plan (reading `Environment::declare` at
`src/interpreter/types.rs:928-938` and the tree-walker's
`instantiate_body_declarations` at `src/interpreter/exec.rs:182-207`) found
that the **var_names prologue is symmetric between engines**, not a bytecode-
specific cost as first hypothesized in the issue's item 2. Both paths do the
exact same `contains_key` check followed by `declare`, and `declare` always
allocates a fresh `String` via `name.to_string()` for the `HashMap` key
(`src/interpreter/types.rs:929-930`) — the tree-walker's `hoist_cache` only
memoizes *which* names to declare, not the per-invocation declare cost itself.
There is no asymmetry here to fix, confirming the issue's own suspicion
("likely a wash — worth confirming rather than assuming"). This plan does not
touch the var_names prologue. If slice 1's measurement (below) contradicts
this reading once real timing data exists, that's a new, separate follow-up —
not a silent scope change to this PR.

`Op::Call`'s own overhead (`take_call_operands`'s `split_off` allocation,
`unroot_stack_value`'s O(n) `rposition`+`remove`) is explicitly out of scope —
the issue itself notes mandreel reaches 97% of its compiled bodies from AST
call sites, not `Op::Call`, so this bench's `b` is an upper bound on what
mandreel actually pays here. Tracked as a `Call`-opcode follow-up, separate
from jsse#538 (`Op::GetElement` fast path, also out of scope here).

## 6. TDD slices

### Slice 1 — measure before fixing (throwaway instrumentation, not shipped as a counter)

This is *investigation*, done with a scratch, uncommitted `Instant::now()`
patch in a local `--features perf-counters` build — not new production code.
Per `perf_counters.rs:8-10`, every counter in that module is a deterministic
*count*; `gc_nanos` is the one existing exception, justified because
collections are rare (thousands, not millions). Chunk entries are 12.7M on
mandreel — timing every one with `Instant::now()` is exactly the
"inflates the very measurement overhead this feature is supposed to keep
negligible" case the `gc_safepoint` comment (`src/interpreter/gc.rs:277-282`)
already warns about. So: measure locally, record the finding in
`docs/perf/2026-09-02/entry-cost-breakdown.md`, and do **not** land the timing
scaffolding itself unless the finding is surprising enough to be worth a
permanent, reusable counter (in which case split it into its own follow-up
slice with its own CLAUDE.md update, not bundled silently into this PR).

Three spans to isolate, on `bench_opmix.js`'s `called` variant (call-bearing,
so it stresses per-entry cost) built with `--features perf-counters`:

1. The two `Vec::with_capacity` calls in `run_chunk_inner`.
2. The `var_names` prologue loop.
3. Everything else on the entry path: the `bytecode_cache` arena lookup +
   `Rc` clone, `enter_ic_body`/`leave_ic_body`, and the `this_val.clone()` at
   the `vm::run_chunk` call site (`src/interpreter/eval.rs:2017`).

Exit condition: confirm span 1 is the dominant, unambiguously-VM-added cost
(expected, per the issue's own framing) before proceeding to slice 2a. If span
2 turns out to be large and asymmetric after all (contradicting §5's reading),
stop and revise this plan rather than silently expanding scope.

### Slice 2a — pool the operand stack

- **Test** (`src/interpreter/bytecode/tests.rs`, new, mirroring
  `function_environment_pool_reuses_and_resets_unescaped_storage` at
  `src/interpreter/tests.rs:115-142`): compile and run a trivial function body
  twice via `run_chunk`/`eval_with_mode` helpers already in that file; after
  the first call, assert `interp.vm_operand_stack_pool.len() == 1`; run a
  second call and assert the result is still correct and the pool did not
  grow unboundedly (`len() == 1` still, the same `Vec` got reused — assert via
  a capacity/identity check analogous to `Rc::as_ptr` in the env-pool test,
  adapted for `Vec`'s allocation pointer via `.as_ptr()`).
- **Production**: add `vm_operand_stack_pool: Vec<Vec<JsValue>>` +
  `MAX_POOLED_VM_OPERAND_STACKS` to `Interpreter`
  (`src/interpreter/mod.rs`), `acquire_vm_operand_stack(&mut self, needed:
  usize) -> Vec<JsValue>` and `release_vm_operand_stack(&mut self, stack:
  Vec<JsValue>)` methods. Change `run_chunk_inner` in `vm.rs` to call these
  instead of `Vec::with_capacity(chunk.max_stack as usize)`, wrapping the
  existing dispatch loop (extracted verbatim into `run_chunk_loop`) with
  acquire-before/release-after, per §4.

### Slice 2b — pool the reference stack

- **Test**: a nested-compiled-call test — function `a` and function `b` both
  compile, `a` calls `b` in its body — asserting correct results and
  `interp.vm_ref_stack_pool.len() == 2` after both calls complete sequentially
  (one `Vec` returned per completed call). Plus a stale-`Rc`-clearing
  regression test: a compiled body using `Op::ResolveName`/`StoreResolvedName`
  against a `with`-scoped binding (so `IdentifierRef::SpecificEnv` is
  populated, not `Unresolvable`), followed by a call to
  `recycle_function_environment` on an unrelated environment with
  `Rc::strong_count == 1` before the ref-stack `Vec` was released — asserting
  the recycle still succeeds (proves clearing precedes pooling, not the other
  way around; a bug here would manifest as the *unrelated* env recycle
  silently failing its `strong_count` guard).
- **Production**: same shape as 2a, `vm_ref_stack_pool: Vec<Vec<IdentifierRef>>`,
  `acquire_vm_ref_stack`/`release_vm_ref_stack`, both explicitly `.clear()`ing
  before pushing back to the pool.

### Slice 3 — deep-recursion-then-throw regression

- **Test**: a compiled function that recurses until `CALL_DEPTH_HARD_LIMIT`
  trips the `RangeError` (mirroring however the existing call-depth tests in
  the repo already trigger it — reuse that pattern rather than inventing a new
  one), caught by a `try`/`catch` in a *tree-walker* (uncompiled) wrapper, then
  a second, unrelated compiled function called afterward. Assert: the second
  call returns the correct result, and both pools' lengths are `<=` their
  `MAX_POOLED_*` caps (proving the pool survived the deep throw bounded, not
  unboundedly grown, and remains usable).

### Slice 4 — validation (no new test; empirical)

Not a code slice — a measurement pass against the finished 2a+2b build, using
the exact tooling `docs/perf/2026-08-26/mandreel-bytecode-work-share.md`
documents under "Method", not an approximation of it:

- `benchmarks/scripts/bench_opmix.js`: pinned/idle-gated timing via the same
  recipe (`taskset` to one CPU if heterogeneous, medians of >=3 repeats,
  `[min-max]` reported) the doc used. Expect `called`'s absolute saving to
  move from **0.635 µs** toward the call-free rows' **~1.18 µs**; `arith`
  (which touches no `Op::Call`, hence no pool churn beyond entry/exit) must
  not regress from its **1.169 µs** baseline.
  Opcode-mix counts for the "opcodes/entries per iter" columns:
  `cargo build --release --features perf-counters`, run each variant, read
  `PERF vm_ops` / `PERF body_dispatch_compiled` from stderr, exactly as
  `opmix-opcounts.tsv` was produced.
- mandreel sweep: `uv run python scripts/run-jetstream.py --test mandreel
  --repeats 3 --json <out>.json` (or the `gen-mandreel-phases.py` phase driver
  if a phase-level breakdown is wanted, per `docs/perf/2026-08-26/`'s method),
  comparing default vs `--engine`-equivalent `--bytecode` run. The issue's own
  prediction: mandreel moves from flat (+1.3%, noise-indistinguishable-from-0)
  to a measurable gain once break-even drops below its bodies' 15.87-opcode
  average — record whatever the sweep actually shows, including a null result,
  in `docs/perf/2026-09-02/`.
- `uv run python scripts/run-test262.py --bytecode`: no regression against the
  `origin/main` baseline (`test262-pass.txt`, read-only per this repo's
  convention — do not pass `--update-baseline`).
- `uv run python scripts/run-test262.py` (default, non-bytecode): no
  regression either — confirms the pooling change has zero effect when
  `bytecode_enabled` is off, since none of this touches the tree-walker.

## 7. Test surface

- No `test262/test/...` directory is specific to this change — it's VM
  internals, not observable JS semantics. The `--bytecode` full-suite run
  (slice 4) is the correctness gate for "did pooling break any compiled
  program," standing in for a targeted test262 subdirectory.
- New unit tests belong in `src/interpreter/bytecode/tests.rs` (pool
  acquire/release correctness, slices 2a/2b/3) — this is VM-internal
  white-box testing of `pub(crate)` pool fields/methods, the same pattern
  `src/interpreter/tests.rs:115-168` already uses for `function_env_pool`.
  Nothing here needs `test262-extra/`: there is no new spec-observable
  behavior to pin down, only an internal reuse discipline.
- Gate: `cargo test --release` (per this repo's bin-only crate convention,
  `cargo test --bin jsse`) plus the two `run-test262.py` invocations in
  slice 4.

## 8. Regression risk

- **`test262-pass.txt` baseline**: should not move in either direction. The
  change touches only `run_chunk_inner`'s frame-entry bookkeeping; no opcode's
  semantics change, no `Completion` value changes, and the `bytecode_enabled`
  default-off tree-walker path is untouched. If the baseline *does* move,
  that's a signal the pool introduced an observable bug (most likely: a
  cleared-too-early or cleared-too-late `Vec` corrupting operand or reference
  state across nested/sequential frames) — stop and treat it as a correctness
  bug, not a baseline update.
- **GC rooting (`gc_bytecode_roots`, `gc_safepoint()`)**: unaffected by
  construction. Rooting/unrooting operates on a wholly separate `Vec<u64>` of
  object ids (`src/interpreter/mod.rs:271`), pushed/popped by `push_value`/
  `pop_value`/`root_stack_value`/`unroot_stack_value` in `vm.rs`, independent
  of whichever `Vec<JsValue>` container currently holds the *values*. Clearing
  or pooling the container `Vec` never touches `gc_bytecode_roots`. This plan
  changes zero lines in `gc.rs`.
- **Bytecode fast path itself**: this *is* the bytecode fast path — the whole
  point of the change is to make `vm.rs` cheaper to enter. The risk is
  concentrated entirely there; nothing in `property.rs`, `eval.rs`'s
  tree-walker arms, or the `ObjectKind` matches is touched.
- **`IdentifierRef::SpecificEnv` / environment-pool interaction**: covered
  explicitly by slice 2b's stale-`Rc` regression test — this is the one place
  a subtle bug (clearing on acquire instead of release) would silently corrupt
  an *unrelated* subsystem (`function_env_pool`'s `strong_count` guard)
  instead of failing loudly near the actual defect.
- **Node-compat library harnesses** (`scripts/run-library-tests.sh`): these
  already run under both engine modes incidentally when `--bytecode`-eligible
  functions appear in bundled library code. No plan to run them specially for
  this issue — a correctness bug here would already surface as a `--bytecode`
  test262 regression first, which is the cheaper, faster-to-bisect signal.

## 9. Out of scope

- The shared-base-offset stack design the issue's own "Suggested approach"
  describes — superseded by the free-list pool design in §4, which achieves
  the same allocation-elimination goal without the cross-frame-leak hazard.
- The `var_names` prologue (§5) — measured-symmetric between engines, not a
  bytecode-specific cost.
- `Op::Call`'s `take_call_operands`/`unroot_stack_value` overhead (issue item
  3) — a separate, smaller lever the issue itself says is an upper bound as
  measured, not what mandreel actually pays.
- jsse#538 (`Op::GetElement` numeric-index fast path) — explicitly a separate
  issue, not bundled here.
- #524 (eligibility expansion to labeled loops) — independent of per-entry
  cost; its target bodies are far above break-even regardless of what this
  issue does.
- Shipping the slice-1 entry-cost timing counters as a permanent, reusable
  `perf-counters` feature — only done if slice 1's finding is surprising
  enough to justify the recurring per-entry `Instant::now()` overhead in that
  build; default is throwaway instrumentation, written up in
  `docs/perf/2026-09-02/` and then reverted.
