# `ExprBox` wraps `Box<Expression>` links to make Drop iterative; `Expression` itself stays `Drop`-free

`Expression`'s recursive fields — `Binary`'s two operands, `Member`'s object,
`Call`'s callee, and so on — were plain `Box<Expression>`. Two parser
productions build long chains of these deliberately without recursive-descent
recursion, specifically so they are *not* bounded by `MAX_PARSE_DEPTH`: the
binary/logical operator-precedence loop (`a+1+1+…`) and the
member/call/optional-chain continuation loop (`a.b.b.b…`, `a()()()…`). #612
and #613 made the other three mandatory (cannot-report-failure) passes over
these chains — `assign_expr_sites`, `expr_uses_arguments`,
`clear_expr_ic_sites`/`clear_stmt_ic_sites` — iterative, moving the debug
abort ceiling from ~122k to ~1–1.5M operands. The compiler-derived `Drop`
glue for `Expression` was the fourth such pass and the one #613 left alone: it
walks a chain with one native stack frame per level, and because that
recursion is release/debug-independent in kind (only the frame size differs),
it is also why the *release* ceiling did not move at all — both profiles
still abort in the same ~2–2.5M range regardless of #613.

The obvious fix, `impl Drop for Expression` with a hand-written worklist, is
not available directly: implementing `Drop` for `Expression` turns every
existing *by-value* destructure of an owned `Expression` in the crate into an
`E0509` (`cannot move out of a type which implements the Drop trait`).
`expr_to_pattern` (`src/parser/mod.rs`) is the clearest example — it moves
boxed children out of `Expression::Assign`/`Spread`/`Object` arms — but
`interpreter/generator_transform.rs` and `generator_analysis.rs` also
rewrite expressions by value throughout their replay-based generator
transform.

## Decision

Introduce `ExprBox`, a newtype around `Box<Expression>` defined in
`src/ast.rs`, and move every `Box<Expression>` field of `Expression` itself
onto it: `Unary`, `Binary`, `Logical`, `Update`, `Assign`, `Conditional`,
`Call`, `New`, `Member`, `OptionalChain`, `Spread`, `Yield`, `Await`,
`TaggedTemplate`, `Typeof`, `Void`, `Delete`, `Import`/`ImportDefer`/
`ImportSource`. `ExprBox` owns a hand-written, worklist-based `Drop`:

```rust
impl Drop for ExprBox {
    fn drop(&mut self) {
        let mut stack = Vec::new();
        push_children(&mut self.0, &mut stack);
        while let Some(mut node) = stack.pop() {
            push_children(&mut node, &mut stack);
        }
    }
}
```

`push_children` steals every `ExprBox` child a node owns — via
`mem::replace(&mut *child.0, Expression::This)`, an in-place swap with the
box's existing allocation reused for a childless placeholder, no new
allocation — and pushes the extracted `Expression` onto the heap-backed
worklist. By the time a node's own (compiler-derived, still-recursive)
destructor actually runs, every one of its `ExprBox` fields already holds a
placeholder leaf, so that destructor call is O(1) rather than proportional to
the original subtree depth. Native stack usage per node is therefore
constant regardless of chain length. `push_children` matches every
`Expression` variant explicitly with no wildcard arm — mirroring the
discipline `gc::trace_object_fields` already applies to `ObjectKind` — so a
future variant that adds an `ExprBox` field fails to compile here instead of
silently reintroducing native recursion for it.

`Expression` itself gains no `Drop` impl. `ExprBox` provides
`into_expression(self) -> Expression`, implemented as
`mem::replace(&mut *self.0, Expression::This)`, as the one legal way to move
an `Expression` out of an `ExprBox` by value — `*owned_expr_box` doesn't
compile once `ExprBox` implements `Drop`, since only `Box<T>` itself gets the
compiler's built-in deref-move; a custom wrapper around a `Box` does not
inherit that. Every site that previously did `*boxed` on an owned
`Box<Expression>` (`expr_to_pattern`'s three arms, and about a dozen sites in
`generator_transform.rs`'s `rewrite_expr`/`transform_yielding_expression`)
now calls `.into_expression()` instead. `ExprBox` also provides `Deref`,
`DerefMut`, `AsRef<Expression>`, `Clone` (delegating to `Box<Expression>`'s
clone — still recursive, unchanged from before, out of scope for this
decision), and a hand-written `Debug` that delegates to the inner
`Expression`'s `Debug` so printed output is unchanged from when the field was
a bare `Box<Expression>` (`Box<T>`'s own `Debug` impl is already
transparent).

For uniformity — one link type and one extraction idiom (`.into_expression()`)
in `ast.rs`, not two — the remaining `Box<Expression>` fields reachable from
`Expression`/`Pattern`/`ExportDeclaration` were converted too:
`MemberProperty::Computed`, `PropertyKey::Computed`, `Pattern::Assign`'s and
`Pattern::MemberExpression`'s expression operands, `ClassDecl`/
`ClassExpr::super_class`, and `ExportDeclaration::Default`. None of these are
required for the stack-safety fix itself — they are all reached through
genuine recursive-descent parsing, bounded by `MAX_PARSE_DEPTH` — but leaving
them as raw `Box<Expression>` alongside `ExprBox` elsewhere in the same file
would mean two incompatible link types with no safety benefit from the split.

## Alternatives considered

- **Arena / index-based AST** (`ExprId` into a `Vec<Expression>`, jsse#614's
  own "Option 2"). Drop becomes O(1) by construction and every future pass is
  naturally iterative — no `push_children` exhaustiveness discipline to
  maintain by hand. This is the durable answer if the AST keeps accumulating
  mandatory whole-tree passes, but it is a whole-engine representation
  change, not a targeted fix for one pass. Deferred as a follow-up; not
  implemented here.
- **Leave it.** #614 itself notes ~1–1.5M operands on debug is far past any
  plausible real input, and the three passes #613 already fixed were the
  ones reachable at ~122k. Rejected because this is *also* the release-ceiling
  bottleneck — #613 alone provably does not move the release abort point at
  all (verified in the issue: both pre- and post-#613 binaries pass at 2M
  operands and abort at 2.5M) — so leaving `Drop` recursive leaves the
  original problem (a native, uncatchable stack overflow on release) exactly
  where it was.

## Consequences

- `Expression` must never itself implement `Drop`. Any future PR that adds
  one reopens the `E0509` failure this ADR exists to avoid; the fix belongs
  on the boxed-link type, not the payload enum.
- `push_children`'s match must stay exhaustive with no `_` arm. Adding an
  `Expression` variant with a new `ExprBox`/`Option<ExprBox>` field requires
  a corresponding arm here, or that field's chain silently regains native
  recursion on drop.
- `ExprBox::clone()` still recurses per nesting level exactly as
  `Box<Expression>`'s clone always did — this ADR is about `Drop` only.
  Nothing in the crate currently clones a chain deep enough for that to
  matter, but it is a known, deliberately out-of-scope gap, not an oversight.
- If the AST keeps accumulating mandatory whole-tree passes beyond the four
  #612/#613/this-ADR have already made iterative, the arena/`ExprId`
  alternative above is the next escalation, not another hand-rolled
  worklist on a fifth pass.
