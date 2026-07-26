# Bytecode direct-call slice

## Context

Issue #388 found that mandreel's hot `render()` path repeatedly calls small
register-style helper functions, but the bytecode compiler rejects every
`CallExpression` and therefore falls back for each containing Body. PR #397
separately covers member/array-element reads and simple-assignment writes.
This slice covers the remaining call-shaped blocker without duplicating that
open PR.

The relevant ECMAScript algorithms are Function Calls, `EvaluateCall`, and
`ArgumentListEvaluation` (§13.3.6 in the checked-in spec). They require callee
evaluation before arguments, left-to-right argument evaluation, the
environment record's `WithBaseObject()` as `this` for an identifier resolved
through `with`, direct-eval handling, callable validation, and proper tail
calls in strict tail positions.

## Approaches considered

1. **Compile bare identifier calls through the existing universal call
   dispatcher.** Selected. This covers mandreel's translated helper-call shape
   (`helper(r0, r1)`) while preserving one call implementation. A callee
   independently uses bytecode when eligible and otherwise follows the
   existing AST path, so recursive/transitive eligibility needs no new compile
   graph.
2. **Compile arbitrary callee expressions and member calls in one slice.**
   Rejected. A member call must preserve a property Reference's receiver as
   `this` and depends on PR #397's property operations. Optional calls, private
   methods, and `super` add distinct semantics. Combining them would duplicate
   an open patch and substantially enlarge the correctness surface.
3. **Add call-site inline-cache probing to the VM immediately.** Rejected for
   this slice. The AST call IC uses a Body-local `CallSiteId` and mutable
   `current_ic_handle`. Bytecode dispatch does not yet install its Body's IC
   handle. The direct bridge is useful without that plumbing, while adding an
   unchecked slot lookup with a stale handle could panic. IC integration
   remains follow-up work under #398.

## Eligibility boundary

The compiler accepts:

- `Identifier(args)` calls;
- any non-spread argument expression already supported by the compiler;
- calls to native functions, bytecode-eligible user functions, and
  bytecode-ineligible user functions.

The compiler rejects the entire containing Body for:

- a callee other than an Identifier, including member, optional, `super`, and
  dynamically selected callees;
- a bare identifier named `eval`, because it can resolve to the realm's
  intrinsic direct eval and needs the caller's lexical environment;
- spread arguments;
- `new`.

Rejecting every bare `eval` is intentionally conservative: a shadowing
non-intrinsic function named `eval` could use the ordinary bridge, but the
compiler cannot prove the runtime binding identity. Falling back preserves
both direct-eval semantics and shadowed-eval behavior.

## Bytecode and stack layout

Opcode values 43–46 are left reserved for PR #397. This slice adds:

- `LoadCalleeName name_index` (47), which resolves the identifier Reference,
  loads its value, and pushes both the callable and the Reference-derived
  `this`;
- `Call argc` (48), which invokes the existing `call_function` bridge and
  pushes its result;
- `ReturnCall argc` (49), which returns a `TailCall` completion in a strict
  function and otherwise behaves like `Call`.

The operand layout immediately before either call opcode is:

```text
[..., callee, this, arg0, ..., argN]
```

`LoadCalleeName` clears the prior receiver marker and uses the same
single-pass `resolve_identifier` path as the tree-walker. A binding resolved
through `with` records its object as `this`; other identifier references use
`undefined`. Keeping resolution and value retrieval in one pass also avoids
repeating an observable Proxy `has` trap for inherited global bindings.
Arguments compile left to right after both values are on the stack. The call
bridge therefore matches `EvaluateCall`'s required ordering and keeps a
getter-produced callee stable while arguments execute.

The opcode stores `argc` as `u16`. Bodies whose call argument count or
resulting operand height does not fit are ineligible rather than truncated.

## GC rooting

`JsValue::Object` holds an arena id, not an owning pointer, and the tracing GC
does not scan the VM's Rust `Vec<JsValue>`. Calls make that existing limitation
immediately unsafe: the callee and earlier arguments remain pending while
later arguments can call arbitrary JavaScript and collect, and all operands
must remain live throughout the callee's execution.

The VM now mirrors every object-valued operand-stack entry into its dedicated
`gc_bytecode_roots` stack, which the collector scans alongside
`gc_temp_roots`:

- pushing a value roots it;
- a non-calling pop removes its matching root;
- binary and unary operations retain popped operand roots until coercion
  finishes, then release them before pushing the result;
- property operations keep the complete pending stack temporarily rooted
  across getters, setters, proxy traps, and key coercion, then release the
  dedicated roots for their consumed operands;
- call operands retain their roots after removal from the operand vector,
  across the complete nested `call_function`, and are released in reverse
  stack order after it returns;
- a frame marker around each `run_chunk` truncates bytecode roots on every
  normal or abrupt exit.

This is a lifetime invariant rather than a scan before only the `Call` opcode.
It protects a callee or earlier argument while a later nested call runs, and
also protects older pending expression operands while name resolution or
coercion invokes user code. Numeric mandreel operands take only the cheap
non-object branch and add no root entries.

Keeping operand roots separate is also an ownership invariant. Native callees
such as host timers may intentionally leave callback roots in
`gc_temp_roots`; bytecode-frame cleanup must not truncate those persistent
roots.

## Tail calls

The tree-walker returns `Completion::TailCall` for direct calls in strict
return position so `call_function` can drive recursion iteratively. Compiling
`return helper(args)` as an ordinary recursive call would turn previously
catchable/iterative recursion into native Rust recursion.

The compiler therefore emits `ReturnCall` followed by the ordinary `Return`.
In strict functions, `ReturnCall` returns the tail-call completion before
invoking the callee. In non-strict functions it invokes normally, pushes the
result, and the following `Return` completes the caller.

Calls nested in a tail-position conditional, logical RHS, or final sequence
element remain ineligible for this slice. Calls under a non-tail operator,
such as `return helper(x) | 0`, use ordinary `Call`; this is important for
C-to-JavaScript output.

## Completion and error handling

`Completion::Normal(value)` and a defensive `Completion::Return(value)` from
the bridge become the call expression's pushed result. `Completion::Empty`
becomes `undefined`. Throws and other abrupt completions propagate out of the
chunk. Callable validation, proxy and bound-function behavior, the catchable
call-depth guard, parameter binding, and compiled-versus-AST callee selection
remain owned by `call_function`.

## Validation

Bytecode unit/end-to-end coverage verifies:

- compiled caller to compiled callee;
- compiled caller to AST-fallback callee;
- native calls;
- persistent callback roots installed by native callees;
- calls in a numeric loop;
- `with`-environment receiver preservation;
- global-prototype Proxy bindings invoke their observable `has` trap once,
  including when the trap result is stateful;
- member-access loops release consumed base and key roots before chunk exit;
- strict direct-return recursion beyond the normal soft call-depth limit;
- non-callable error propagation;
- explicit rejection of direct eval, spread, member calls, and unsupported
  nested tail positions;
- a freshly allocated first argument surviving a forced GC in a later
  argument;
- a getter-produced callee surviving a forced GC while its argument is
  evaluated.

The targeted test262 call-expression directory must pass under `--bytecode`,
and a release-mode isolated loop repeatedly calling a small numeric helper
must show a wall-clock improvement over the tree-walker before the slice is
published.

## Success criteria

An otherwise eligible numeric loop containing `helper(args)` executes a
bytecode Chunk; each helper independently compiles or falls back; call
ordering, `this`, abrupt completions, direct eval, and strict tail calls match
the tree-walker; forced collection cannot reclaim pending callees or
arguments; targeted and full test262 runs have no baseline regression; and
the isolated call-heavy loop is measurably faster with `--bytecode`.
