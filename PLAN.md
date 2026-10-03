# Plan: issue #807 — refactor(gc): unify or assert gc_bytecode_roots, the third root stack

## 1. Problem restated

`gc_bytecode_roots` (`src/interpreter/mod.rs`, manipulated entirely from
`src/interpreter/bytecode/vm.rs`) is a raw `Vec<u64>` that roots the bytecode
VM's operand stack. Unlike `gc_temp_roots`, which is a `RootStack`
(`src/interpreter/root_stack.rs`) with a debug-asserted LIFO discipline
(`pop_expected` panics on a non-top release, `truncate` panics if asked to
shrink below the live depth) and, since #810, equality balance checks
(`gc_assert_root_depth`) at native-call/microtask-job/timer-callback/program-run
boundaries, `gc_bytecode_roots` has none of that: it is pushed with
`Vec::push` and released with a silent `rposition`+`remove` that does nothing
if the id isn't found anywhere, and truncated with a silent `Vec::truncate`
that does nothing if asked to shrink below the live depth. #331's balance
assertions deliberately excluded it because a mid-chunk `Throw` can leave the
bytecode operand `stack: Vec<JsValue>` (and its `gc_bytecode_roots` mirror)
non-empty when an opcode handler bails via `return abrupt` without draining
the rest of the stack — so a naive equality check at an arbitrary point would
be unsound. That reasoning is correct for *inside* a chunk, but it was used to
justify giving this stack no discipline at all, anywhere. The fix is to give
`gc_bytecode_roots` the same `RootStack` type (closing the "no assert on
misuse" gap unconditionally) and to add the one equality check that *is*
sound: at chunk exit, where `run_chunk_with_var_prologue` already
unconditionally truncates back to the entry depth on every return path.

## 2. Spec basis

N/A: no JavaScript behavior change. This retypes an internal GC root-tracking
field and adds debug-only balance assertions (`debug_assert!`/
`debug_assert_eq!`, compiled out entirely in plain release builds). Release
behavior is bit-for-bit identical: `RootStack::pop_expected`'s non-debug
fallback is the same rposition-and-remove the code does today, and
`RootStack::truncate` is the same `Vec::truncate`. No parser, interpreter, or
builtin semantics change; nothing here is observable from JavaScript.

## 3. Files to touch

- `src/interpreter/mod.rs` — retype the field:
  `pub(crate) gc_bytecode_roots: Vec<u64>` → `pub(crate) gc_bytecode_roots:
  root_stack::RootStack`; update the `Interpreter::new()` initializer from
  `Vec::new()` to `root_stack::RootStack::default()`.
- `src/interpreter/gc.rs` — `collect_gc_roots`'s
  `roots.extend_from_slice(&self.gc_bytecode_roots)` becomes
  `roots.extend_from_slice(self.gc_bytecode_roots.as_slice())` (no `Deref` on
  `RootStack` by design).
- `src/interpreter/bytecode/vm.rs`:
  - `unroot_stack_value` drops its manual `iter().rposition(...)` +
    `Vec::remove` and calls `interp.gc_bytecode_roots.pop_expected(object_id)`
    instead (same early-return-on-non-object guard as today).
  - `run_chunk_with_var_prologue` gains a chunk-exit balance check before the
    existing truncate: when `result` is anything other than
    `Completion::Throw` or `Completion::Exit`, debug-assert
    `interp.gc_bytecode_roots.len() == gc_frame`. `Throw`/`Exit` are exempt
    because that's precisely the case #331 identified as unsound to assert
    on; every other completion (`Normal`, `Return`, `Empty`, `TailCall`) is
    produced only after the chunk's own opcode handlers (`Op::Return`,
    `Op::ReturnCompletion`, the strict-tail-call early return in `Op::Call`)
    have already popped/unrooted their one live value, so the stack is
    already back to `gc_frame` in every one of those cases (verified against
    the compiler's `current_stack`/`pop_n` bookkeeping — see slice 2 below).
    `RootStack::truncate`'s own existing underflow `debug_assert!` already
    covers the `Throw`/`Exit` case's one real failure mode (shrinking below
    the entry depth), so nothing new is needed there.
  - No change to `push_value`, `root_stack_value`, `take_call_operands`,
    `release_call_operands`, `take_construct_operands`,
    `release_construct_operands`, or any opcode handler: they already route
    every mutation through these four helpers, which is the existing funnel
    this plan reuses (mirrors how `gc_root_id`/`gc_unroot_id` funnel
    `gc_temp_roots` mutation).
- `src/interpreter/root_stack.rs`:
  - Generalize the type's doc comment: it's no longer only "the temporary GC
    root stack" backing `gc_temp_roots`; say it also backs the bytecode VM's
    operand-stack roots (`gc_bytecode_roots`).
  - Generalize `pop_expected`'s panic message from `"temp root {id} released
    out of LIFO order..."` to `"root {id} released out of LIFO order..."` —
    keep the substring `"released out of LIFO order"` intact so the existing
    `#[should_panic(expected = "released out of LIFO order")]` test in
    `src/interpreter/tests.rs` keeps passing unchanged. `truncate`'s message
    ("outlived its roots") is already stack-agnostic; no change needed there.
- `src/interpreter/bytecode/tests.rs` — add the three tests from slice list
  below. The two existing `interp.gc_bytecode_roots.is_empty()` assertions
  (lines ~1641, ~1804) keep compiling unchanged: `RootStack::is_empty()` is
  already `#[cfg(test)] pub(crate)`.
- `CLAUDE.md` — extend the "GC Root-Stack Discipline" section: note that
  `gc_bytecode_roots` now shares `RootStack` and carries its own chunk-exit
  balance check (not the native-call/job/run equality checks #810 added for
  `gc_temp_roots` — those remain future work, see §7).
- No `docs/adr/` entry: this is a straightforward "use the existing tool"
  fix, not a new architectural decision — nothing here is a design
  alternative a future reader would need recorded as "why we didn't do X."

## 4. TDD slices

1. **Red: retype compiles.** Change `gc_bytecode_roots`'s type in
   `src/interpreter/mod.rs` and fix the two call sites (`gc.rs`'s
   `as_slice()`, `vm.rs`'s `pop_expected`). This alone is red until done
   (`Vec<u64>` has no `pop_expected`/the old code has no `as_slice` user);
   green once the three call sites compile. No new test needed for this step
   — the existing `cargo build`/`cargo test` compile is the check.

2. **Red → green: out-of-order release panics.** In
   `src/interpreter/bytecode/tests.rs`, add (mirroring
   `root_stack_discipline` in `src/interpreter/tests.rs:5837-5849`, gated the
   same way):
   ```rust
   #[cfg(debug_assertions)]
   #[test]
   #[should_panic(expected = "released out of LIFO order")]
   fn unrooting_a_non_top_bytecode_root_asserts() {
       let mut interp = Interpreter::new();
       let (first, second) = (interp.create_object_id(), interp.create_object_id());
       interp.gc_bytecode_roots.push(first);
       interp.gc_bytecode_roots.push(second);
       interp.gc_bytecode_roots.pop_expected(first);
   }
   ```
   Red before slice 1 (no `pop_expected` method on `Vec<u64>`); green after.
   Proves the field is actually wired to `RootStack`, not just type-compatible
   by accident.

3. **Green: multi-statement completion-value regression.** Add a test
   exercising the `Op::SetCompletion`/`Op::ReturnCompletion` path this plan's
   reasoning depends on (compiler's `current_stack` returns to 0 between
   statements — confirmed by reading `compile_statement`'s `Statement::If`/
   `Statement::While`/`Statement::For` arms, each ending in
   `debug_assert_eq!(self.current_stack, 0)` before the backedge, and
   `Statement::Expression` always paired with `pop_n(1)`). Use the existing
   `Object()`-returns-an-object idiom from
   `script_completion_value_is_rooted_across_nested_gc`
   (`bytecode/tests.rs:1618`) since object literals aren't bytecode-eligible:
   ```rust
   #[test]
   fn multi_statement_script_completion_stays_balanced_under_bytecode() {
       let source = "Object(); if (true) { Object(); } for (var i = 0; i < 2; i++) { Object(); } Object();";
       let mut interp = Interpreter::new();
       interp.bytecode_enabled = true;
       let program = /* parse source */;
       let completion = interp.run(&program);
       assert!(interp.bytecode_chunks_executed >= 1);
       assert!(matches!(completion, Completion::Normal(_)));
       assert!(interp.gc_bytecode_roots.is_empty());
   }
   ```
   This is green on today's behavior too (it's a regression guard, not a bug
   fix) but must stay green after slices 1-2 wire in the new chunk-exit
   `debug_assert_eq!` — if the SetCompletion-stays-on-top reasoning in §3 is
   wrong, this is exactly the test that turns red first.

4. **Green: throw with a live outer operand doesn't trip the new assert.**
   Add a test where a bytecode-compiled outer function has an object value
   still on its operand stack when a nested call throws (e.g.
   `(function(x){ return x.y + (function(){ throw new TypeError(); })(); })(Object())`
   compiled to bytecode) and assert `interp.run(...)` returns `Throw` cleanly
   with no panic, and `gc_bytecode_roots.is_empty()` afterward. This is the
   exact scenario #331 called out as unsound to assert on unconditionally —
   it must keep working because the new check in `run_chunk_with_var_prologue`
   is gated on `result` not being `Throw`/`Exit`.

5. **Docs.** Update `CLAUDE.md` and `root_stack.rs`'s doc comments (slice 3 of
   §3). No code behavior depends on this; do it last so the prose matches the
   landed shape of the code.

## 5. Test surface

No `test262/` directory is specific to this change — it's engine-internal GC
bookkeeping, not observable JS behavior. Gates, in order:

- `cargo test` — runs the new `bytecode/tests.rs` tests and the full suite in
  a debug build (debug assertions on), including the existing
  `root_stack_discipline` module in `src/interpreter/tests.rs` (unaffected,
  but must keep passing since `pop_expected`'s message changed).
- `cargo build --profile release-checked`, then:
  `uv run python scripts/run-test262.py --jsse ./target/release-checked/jsse --test262 ./test262 test262-extra/ --bytecode --fail-on-failures`
  — this is the CI job (`.github/workflows/ci.yml:99`) that actually exercises
  the new chunk-exit assertion against real bytecode-compiled code; run it
  locally before pushing.
- Because CI's `--bytecode` coverage is scoped to `test262-extra/` only (the
  10% sample at `ci.yml:102` runs the tree-walker, not bytecode), also run a
  **broader, local-only** sweep before declaring this done — not part of CI,
  but the strongest available signal that the "stack returns to `gc_frame` on
  every non-throw exit" reasoning holds across real-world bytecode-eligible
  code, not just the narrow `test262-extra/` corpus:
  `uv run python scripts/run-test262.py --jsse ./target/release-checked/jsse --test262 ./test262 --bytecode --sample 0.1 --seed 807`
  A debug_assert firing here means the design in §3/§4 slice 3 has a case
  this plan didn't account for — resolve it in the implementation stage
  rather than suppressing the assert.
- Repeat the `release-checked` + `--bytecode` test262-extra run with
  `JSSE_GC_STRESS=16` (small `N` is impractical here since `--bytecode`
  safepoints are already sparse per `CLAUDE.md`'s GC Stress Mode section) to
  catch any rooting gap this change's reasoning missed, independent of the
  new debug_assert's own correctness.
- `./scripts/lint.sh` — standard gate, unrelated to this change but always
  run before a PR.

## 6. Regression risk

- **Cannot move `test262-pass.txt`.** Release-mode behavior is unchanged
  (both `RootStack::pop_expected`'s fallback and `RootStack::truncate` match
  today's `Vec` behavior exactly; the new chunk-exit check is
  `debug_assert_eq!`, compiled out in plain release). The only way this PR
  moves anything is by a debug/release-checked build now panicking where it
  previously ran silently — which is the explicit point of the change, not a
  regression, provided the panic only fires on a genuine bug.
- **Main risk: a false-positive panic.** If any bytecode-eligible construct
  leaves the operand `stack` non-empty at a non-`Throw`/`Exit` chunk exit that
  slice 3's reasoning didn't cover (e.g. a future compiler change, or an
  existing construct this plan's trace of `compile_statement` missed), the
  new `debug_assert_eq!` in `run_chunk_with_var_prologue` fires on previously
  "working" (silently-tolerant) code. Mitigated by slices 3-4's regression
  tests plus the broader local `--bytecode --sample` sweep in §5 before
  opening the PR — if that sweep is clean, CI's narrower `test262-extra`
  bytecode gate will be too.
- **Shared machinery leaned on:** GC rooting and `gc_safepoint()` directly (by
  construction — this *is* a GC-rooting change); the bytecode fast path
  (`bytecode/vm.rs`, and `bytecode/compiler.rs`'s `current_stack`/`pop_n`
  bookkeeping, which this plan's soundness argument depends on but does not
  modify). Not leaned on: the tree-walker hot paths, the property MOP
  (`property.rs`), the exhaustive `ObjectKind` matches, or the Node-compat
  library harnesses — none of those execute through `gc_bytecode_roots`.
- **`RootStack`'s existing consumers (`gc_temp_roots`) are untouched** except
  for the one generalized panic message string; the `#[should_panic(expected
  = "released out of LIFO order")]` test in `src/interpreter/tests.rs:5842`
  is the one place that could break from a careless rewording, and slice 2's
  plan preserves the exact substring.

## 7. Out of scope

- **Boundary-level equality assertions mirroring #810's
  `gc_assert_root_depth`** for `gc_bytecode_roots` at the native-call
  (`eval.rs:5217,5223`), microtask-job (`mod.rs:5680,5928`), timer-callback
  (`mod.rs:5805`), and program-run (`mod.rs:2451,2480`) boundaries. These
  boundaries are sound to assert on too (every nested `run_chunk` call
  truncates unconditionally before control returns past them, so
  `gc_bytecode_roots` should already be back at its pre-boundary depth by the
  time execution reaches any of those seven sites), but auditing all seven
  call sites and writing their regression tests is a second, additive change
  with its own review surface — a natural immediate follow-up issue, not
  bundled here.
- **Merging `gc_bytecode_roots` and `gc_temp_roots` into one physical
  stack.** Rejected: `root_operand_stack` (`bytecode/vm.rs:37-43`) roots a
  *snapshot* of the live operand stack into `gc_temp_roots` for the duration
  of a nested operation (a getter, a proxy trap, `ToPropertyKey`), on top of
  entries that `gc_bytecode_roots` already holds for those same values and
  that must independently outlive that snapshot's frame. A single merged
  stack would make the snapshot's bulk `gc_unroot_frame` truncate also
  remove the bytecode-owned entries underneath it, or require re-deriving an
  order-preserving separation — more risk for no behavioral gain over keeping
  two `RootStack` instances.
- **Expanding bytecode-eligible statement/expression coverage** (the
  `Err(CompileError::Unsupported(...))` fallbacks throughout
  `compiler.rs`) or any try/catch/finally bytecode lowering — unrelated to
  rooting discipline and out of scope for a GC-discipline fix.
- **Any change to `push_value`/`root_stack_value`/`take_call_operands`/
  `release_call_operands`** or introducing new `Interpreter`-level wrapper
  methods (e.g. a `gc_root_bytecode_value` mirroring `gc_root_id`) — the four
  existing private helpers in `vm.rs` already are the single funnel this
  stack needs; adding a parallel public API surface is unjustified
  indirection for an internal, single-module concern.
