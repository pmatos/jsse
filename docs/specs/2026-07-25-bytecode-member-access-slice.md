# Bytecode member/array-element access slice

## Context

Issue #388 found that the mandreel benchmark's heap-copy loop,
`for (i=0;i<N;i++) heap32[i]=my_heap32[i]`, is a textbook eligible-shaped
numeric loop that still falls back to the tree-walker, purely because its body
is a `MemberExpression` read plus a `MemberExpression` assignment — explicitly
out of scope for the numeric-loop slice (docs/specs/2026-07-20-bytecode-loop-slice-design.md).
The issue's own suggested first slice bundled member/array-element access with
a restricted `Call` opcode. This slice deliberately narrows that to member
access only: `Call` carries its own hazard surface (GC-rooting across a call,
IC-handle plumbing, a bail list for `eval`/spread/optional-call/`super()`/`new`,
call-depth parity with the tree-walker's catchable guard) large enough to
warrant its own follow-up issue and PR, decided explicitly with the user
before this work began.

## Approaches considered

1. **Extend the existing stack VM with dedicated Get/Set opcodes, reusing the
   tree-walker's shared MOP entry points in `property.rs`.** Selected. Mirrors
   how `Op::Add`/`Op::Sub`/etc. already reuse `eval_binary` rather than
   re-deriving arithmetic semantics in the VM. Four new opcodes cover static
   (`a.b`) and computed (`a[k]`) access, read and simple-assignment write.
2. **Also support compound-assignment (`a[k] += v`) and update (`a[k]++`) on
   member targets.** Rejected for this slice. Both require re-reading the
   current property value using the *same* base+key without re-evaluating
   either (to avoid double side effects), which forces a choice between a new
   "peek-without-pop" opcode shape or a parallel `PropRef` stack analogous to
   the existing `IdentifierRef`/`ResolveName`/`LoadResolvedName` machinery —
   real mechanical cost not justified by the motivating workload, since
   mandreel's heap-copy loop is a plain `Assign`, not a compound op. Left
   unsupported; the whole containing function bails to the tree-walker via the
   existing eligibility membrane, exactly as it did before this slice.
3. **Compile every `Member`/`MemberProperty` shape, including private fields,
   optional chaining, and `delete`.** Rejected — matches issue #67's own
   rejected "compile everything, enable by default" approach. Each of these
   shapes has its own semantics to get exactly right (private-field brand
   checks, optional short-circuit, `[[Delete]]`'s return-value semantics) and
   none appear in the motivating numeric-loop workload.

## Design

The bytecode module remains a deep module with one external seam:
`compile_body` either returns a complete `Chunk` or rejects the entire `Body`.
Four new opcodes were added (`op.rs`, values 43-46): `GetProp`/`SetProp` (dot
access, `u16` name-pool index operand) and `GetElement`/`SetElement` (computed
access, key already on the operand stack). `compiler.rs` gained an
`Expression::Member` rvalue case and extended `Expression::Assign`'s target
match to accept a `Member` target restricted to `AssignOp::Assign` — any
compound `AssignOp` on a `Member` target still falls through to
`CompileError::Unsupported`, bailing the whole function. The `Expression::Update`
arm needed no change: its existing `Identifier`-only guard already rejected
`Member` targets structurally.

Because compound-assign/update-on-member are out of scope, no new ref-stack or
`Chunk` field was needed: base, (computed) key, and RHS are pushed onto the
existing operand stack in source order and consumed by a single terminal
opcode, exactly symmetric with the existing `Binary` op's `pop_n`/`push_n`
accounting.

Each new opcode's `member_get`/`member_get_computed`/`member_set`/
`member_set_computed` helper (free functions in `vm.rs`) replicates the
tree-walker's exact evaluation order and error-message text rather than
re-deriving it from spec text, since the tree-walker is this project's proven
100%-test262 parity baseline:

- **Read**: null/undefined-base check (throwing the tree-walker's exact
  message — the computed case's message has **no key interpolated**, unlike
  the Dot case, which does) → (computed only) `to_property_key` → primitive
  boxing via `to_object` if the base isn't already an object →
  `get_object_property` (property.rs).
- **Write (simple assign only)**: (computed only) `to_property_key` on the
  already-evaluated raw key → null/undefined-base check → `set_object_with_key`
  (`eval.rs`, bumped from private to `pub(super)` — the same reuse target
  `eval_update`'s write path already uses, in preference to `eval_assign`'s own
  Member-write arm, which duplicates most of `[[Set]]` inline rather than
  calling the canonical `property.rs::proxy_set`).

### GC rooting (the load-bearing new invariant)

`JsValue::Object` is a bare arena `u64` id with no `Rc`/`Drop` — holding a
Rust-local clone does not keep it alive. `gc_safepoint()` builds its root set
from an explicit, hand-maintained list (realms, call-stack envs/frames,
`gc_temp_roots`) and never scans the VM's native `Vec<JsValue>` operand stack.
A collection can run **synchronously nested** inside any of the new opcodes'
own property-access call, since a getter, a proxy trap, or `ToPropertyKey`
coercion can itself execute arbitrary JS, and any JS function body reaches a
real `gc_safepoint()` at the top of its first statement.

An initial design rooted only each opcode's own operands immediately before
its own property.rs call. Adversarial review found this insufficient: a
chained expression like `a.b.value = a.c` compiles to `GetProp(b)` [pushes
`R1`, an object] → the RHS sub-expression `a.c`'s own `GetProp(c)` [invokes a
getter that can itself force a GC] → `SetProp(value)` [finally consumes `R1`].
`R1` sits unrooted on the operand stack for the entire window between the
first and last opcode, spanning an arbitrary number of intervening,
GC-capable opcodes — rooting only the *current* opcode's own operands misses a
value pushed by an *earlier* opcode and consumed by a *later* one.

The fix: every one of the four new opcodes roots the **entire current operand
stack** (not just its own operands) before calling into `get_object_property`/
`to_property_key`/`to_object`/`set_object_with_key`, via a small
`root_operand_stack` helper built on the existing `gc_root_frame`/
`gc_root_value`/`gc_unroot_frame` machinery (the same mechanism `eval_binary`
already uses to root its operands across `ToPrimitive` coercion). Rooting is
by object id and does not require continued ownership of the `JsValue`, so
rooting before popping this opcode's own operands already covers both any
older pending stack entry and this opcode's own soon-to-be-popped locals via
the same ids.

## Error handling and limits

Every new opcode returns `Completion::Throw`/`Err` on a JS-level failure
(null/undefined base, a rejected strict-mode `[[Set]]`, a `ToPropertyKey`
failure) exactly like existing opcodes — never `.expect()` outside a genuine
internal invariant violation (stack underflow), matching existing style.
Unsupported member-access shapes (private fields, optional chaining, member
access as a call callee, compound-assign/update on a member target) reject the
whole `Body` via `CompileError::Unsupported`, per the existing whole-body
eligibility-membrane design — there is no per-instruction fallback.

## Validation

- Unit + end-to-end tests in `bytecode/tests.rs` cover: dot and computed reads
  (plain array and typed array), dot and computed simple-assignment writes,
  null/undefined-base error-message parity between AST and bytecode modes
  (compared against each other, not a hardcoded string, so a future
  tree-walker message change can't silently desync the two), and explicit
  bail-out tests for compound-assign-on-member, update-on-member, private
  fields, and optional member access.
- A dedicated two-hop GC-hazard regression test
  (`end_to_end_member_chain_base_survives_gc_during_rhs_evaluation`) exercises
  exactly the `a.base.value = a.rhs` shape described above, with `a.rhs`'s
  getter forcing a collection (`$262.gc()`) while `a.base`'s freshly-allocated,
  otherwise-unreferenced result is still pending on the operand stack.
  Verified to actually discriminate: temporarily reverting to the
  per-op-only rooting design made this specific test fail (`observed` never
  got set, i.e. the base was corrupted), confirming it is not a
  vacuously-passing test.
- A release-mode timing comparison (tree-walker vs `--bytecode`) on the exact
  motivating heap-copy-loop shape (`dst[i] = src[i]` over `Int32Array`s, 20
  iterations of a 200,000-element copy) showed a stable ~33% wall-clock
  reduction (≈9.0s → ≈6.0s across two runs each) — a genuine, reproducible win
  on issue #388's own motivating workload, evidence rather than a unit-test
  assertion. This is despite computed-key access unconditionally routing
  through `to_property_key` (an allocate-then-parse round trip for a raw
  `Number` key that the tree-walker's own read fast path in
  `eval/access.rs` avoids) — a further optimization opportunity noted as a
  natural fast-follow, not required to land this slice since a net win is
  already demonstrated.
- Full test262 suite held 100% (0 regressions) alongside the full unit suite
  and `./scripts/lint.sh`.

## Success criteria

`heap32[i] = my_heap32[i]` inside an otherwise-eligible numeric `for` loop
executes a bytecode `Chunk`, returns the same result as the tree-walker for
both dot and computed, read and write, member access, preserves error-message
parity with the tree-walker for null/undefined bases, survives a GC collection
triggered while a member-access result is pending on the operand stack, and
does not introduce a test262 baseline regression.
