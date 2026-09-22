# Async destructuring patterns: lowering `await` in defaults and keys

Issue #709: `generator_transform.rs` decided what to lower by asking
`generator_analysis.rs` whether a statement contains a suspension, and both
looked only at *expressions* — `VariableDeclarator::init`, never the
declarator's `Pattern`. An `await` in a destructuring default
(`var {a = await 1} = {}`) or computed key (`var {[await k]: a} = o`) was
therefore invisible: the statement was emitted intact into a state body and the
tree-walker evaluated the `await` through `eval_expr`'s blocking `await_value`,
draining the microtask queue inline before the async function had returned to
its caller. The observable result was jobs running in the wrong order
(`w1,a1,sync-end,…` instead of `sync-end,w1,a1,…`).

## Decision

Lower an object binding pattern that reaches an `await` into a sequence of
state-machine steps over temps, following `KeyedBindingInitialization`
(`sec-runtime-semantics-keyedbindinginitialization`):

```
$src = <init, suspension-lowered as before>
<kind> {} = $src                          // RequireObjectCoercible, once
per property, in source order:
  [computed key with a suspension]  $k = <key, lowered>   // at its own position
  no suspension in the property:    <kind> {<key>: <element>} = $src
  otherwise:
    $v = $src[<key>]                                       // exactly one GetV
    [default]  ConditionalGoto(typeof $v === "undefined")
                 true -> $v = <default, lowered to Await/Yield states>
    recurse on the element's inner pattern with source $v
```

- **Detection and lowering travel together.** `pattern_needs_lowering`
  (`generator_analysis.rs`) is what `contains_suspension` consults for a
  declaration's pattern, and it is true only for shapes `lower_pattern_binding`
  handles. An async function whose only suspension sits in an unsupported
  pattern therefore still takes the simple, fully tree-walked machine exactly as
  before, instead of becoming a full machine with an intact statement inside.
- **Defaults are conditional.** Unlike the `Expression::Object`/`Array`
  *literal* arms of `transform_yielding_expression`, which hoist every
  suspension unconditionally, a default runs only when the value is
  `undefined`; a present property runs no default and costs no tick.
  `typeof $v === "undefined"` is used rather than the identifier `undefined`,
  which user code can shadow.
- **One `GetV` per key, computed keys at their own position.** Each key's temp
  is evaluated after the earlier properties have been read and before its own
  read, so an earlier getter is never deferred behind a later key's `await`,
  and no getter fires twice.
- **Everything that does not suspend stays with the tree-walker.** Sibling
  properties and inner patterns without a suspension are bound through an
  ordinary sub-pattern statement (`<kind> {<key>: <element>} = $src`), so
  NamedEvaluation of anonymous functions, `let`/`const`/`var` kinds, TDZ, and
  `with` scopes are the tree-walker's, unchanged.
- **The trigger is `await`, not `yield`.** In an async function the
  `await`-to-`yield` rewrite copies patterns verbatim, so they still hold raw
  `Await` nodes, which `transform_yielding_expression` already lowers straight
  to `StateTerminator::Await`. In an async generator only an `await` in the
  pattern triggers lowering; a yield-only pattern keeps its current path (a
  known gap, below). Once a pattern is lowered, its yields and awaits are
  suspended alike (`pattern_contains_suspension`).
- **No new `StateTerminator`**, so the other two drivers are untouched. The
  temps (`$dstr_src`, `$dstr_key`, `$dstr_val`) live in the function-env
  `temp_vars`, already GC-rooted as locals.

_Superseded in part by ADR-2026-09-22-1815: destructuring-**assignment**
forms (`[a = await 1] = []`, `({a = await 1} = {})`) no longer hang — the
async-function `await`-to-`yield` rewrite no longer touches a
destructuring-assignment left side, and object assignment patterns get the
same lowering this ADR gives declaration patterns._

## What this change does not cover

Each of these is unchanged behavior, tracked as a follow-up (#724 assignment forms,
#725 array patterns and object rest, #726 catch/for heads, #727 `yield` in a
declaration pattern):

- **Array patterns** (`var [a = await 1] = []`): iterator steps are observable
  and the `await` must land *between* steps, which needs the iterator record
  held across states and closed exactly once on any abrupt exit — new
  interpreter-internal helpers, not a transform-only change.
- **An object rest beside a suspending sibling** (`{a = await 1, ...rest}`):
  `CopyDataProperties` needs the consumed-key exclusion list without re-reading
  the source.
- **Catch parameters and for-in/of heads** (`catch ({a = await 5})`,
  `for (var {a = await 1} of …)`): the driver binds these in
  `EnterCatch`/`ForOfHead` where no state boundary exists.
- **Destructuring *assignment* forms** (`[a = await 1] = []`,
  `({a = await 1} = {})`): fixed by ADR-2026-09-22-1815 — object patterns now
  get the same lowering, array patterns fall back to the blocking-tree-walker
  path below instead of hanging.
- **`for (var {a = await 1} = …;;)` initializers.**
- **`yield` in a declaration pattern** (`var {a = yield 1} = {}`), in sync and
  async generators, which silently never yields.

Function *parameter* patterns are not lowering sites: `await` in async
formals and `yield` in generator formals are early SyntaxErrors.

Accepted imprecision: a `let`/`const` binding created mid-machine gets the same
TDZ precision `SentValueBindingKind::Pattern` already gives `let {a} = await p`.
