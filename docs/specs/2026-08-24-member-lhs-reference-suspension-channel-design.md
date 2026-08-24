# Member LHS reference suspension channel

## Goal

Remove the mutable resume-value out-parameter from
`eval_member_lhs_ref` without changing assignment or destructuring behavior.
The helper's interface must make a suspended member-reference evaluation
impossible for any caller to overlook while remaining narrower than the
interpreter's general `Completion` type.

## Specification constraints

ECMA-262 member-expression evaluation first evaluates the base and then, for a
computed member, evaluates the key. Either evaluation can suspend at a
`YieldExpression`. `EvaluatePropertyAccessWithExpressionKey` deliberately
retains the raw key value: `ToPropertyKey` for `a[b] = c` is deferred until
after the right-hand side has been evaluated.

Destructuring assignment adds an ordering constraint. Iterator
`AssignmentElement` and `AssignmentRestElement` evaluation computes a
non-pattern target reference before stepping or draining the iterator.
`KeyedDestructuringAssignmentEvaluation` likewise computes the target
reference before reading the source property. A suspension at that point must
therefore leave later iterator/property access and write-back unperformed.

## Existing seam

`eval_member_lhs_ref` pre-evaluates member, private, and `super` references for
the assignment/destructuring module. It currently returns
`Result<Option<DestructLRef>, JsValue>` while separately writing a yielded
value through `&mut Option<JsValue>`. `Ok(None)` means the target is not a
member and remains on the caller's lazy `put_value_to_target` path; `Err`
contains a thrown JavaScript value.

The three callers are two array-destructuring paths and one
object-destructuring path. Array destructuring already has a local
`yield_val` because defaults and lazy target write-back can also suspend, and
because all such suspensions must pass through shared iterator-close/replay
bookkeeping. That local state is not part of the helper's interface and
remains useful.

## Approaches considered

1. **Return a private two-variant evaluation result.** Selected. Add
   `MemberLhsRef::Ref(Option<DestructLRef>)` and
   `MemberLhsRef::Suspended(JsValue)`, retaining `Result::Err(JsValue)` for
   throws. Every caller must exhaustively handle suspension while the existing
   optional-reference fallback stays intact.
2. **Return `Result<Option<DestructLRef>, Completion>`.** Rejected. It would
   remove the out-parameter, but `Completion` admits break, continue, return,
   await, and other states this helper cannot legitimately produce. That
   broadens the interface and obscures the useful distinction between a throw
   and a resumable suspension.
3. **Add separate `NotMember`, `Ref`, and `Suspended` variants.** Rejected.
   This makes the same states exhaustive, but expands the new interface only
   to replace an `Option` that every caller already consumes during its
   write-back dispatch. Nesting that existing optional result in `Ref` keeps
   the suspension channel to the two states that matter at this seam.

Storing suspension on `Interpreter` was also excluded because it would turn a
visible return obligation into hidden mutable state and make locality worse.

## Design

Define the private `MemberLhsRef` enum beside `DestructLRef`. Change
`eval_member_lhs_ref` to accept only the target and environment and to return
`Result<MemberLhsRef, JsValue>`. Normal member and non-member outcomes become
`Ref(Some(...))` and `Ref(None)` respectively. Each of the helper's three
`Completion::Yield` branches returns `Suspended` directly. Throw branches and
the timing of `ToPropertyKey` remain unchanged.

At both array-destructuring callers, exhaustively match the helper result.
`Ref` continues through the existing iterator and write-back algorithm;
`Suspended` stores the value in the function-local `yield_val` and breaks to
the shared suspension/iterator bookkeeping; `Err` stays on the shared throw
path. At the object-destructuring caller, `Suspended` returns
`Completion::Yield` immediately because that path has no open iterator to
close or remember.

No generated `yield_val` temporary names or ordinary
`Completion::Yield(yield_val)` bindings are in scope. No new adapter or public
seam is introduced.

## Error handling and observable order

Thrown base/key evaluation and `ToPropertyKey` errors remain in the existing
`Result::Err` channel. A yielded value is moved into `Suspended` without
cloning. All later observable work remains after the exhaustive match, so a
suspension still prevents iterator stepping, source property reads,
initializer evaluation, and target writes.

The refactor does not change GC rooting. Array destructuring keeps the
iterator rooted until its existing shared exit path, and the pre-evaluated
reference continues to own its base and raw key across later user code.

## Verification

Use the existing test262 assignment-destructuring coverage for computed-key
suspension in array elements, array rest targets, object properties, and
iterator closing. Add a focused `test262-extra` characterization for
suspension while evaluating a target's base and a `super` computed key if the
pinned test262 revision does not cover those helper branches.

Run `cargo fmt --check`, `cargo clippy`, `cargo build --release`,
`cargo test --release`, the focused `test262-extra` characterization, the full
`test262/test/language/expressions/assignment/dstr/` directory, and the full
test262 suite. The refactor must not change the baseline pass count.
